//! Contratos compartidos: vínculo explícito acción→hábito y aportes a objetivos sin doble conteo.

mod common;

use chrono::NaiveDate;
use persistence::hooks::{self, HabitTrigger, ProgressValue};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

async fn setup(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid) {
    let t = common::TestApp::new(pool.clone());
    let a = t.login(111).await;
    let user: Uuid = a.user_id.parse().unwrap();
    let habit: Uuid = sqlx::query_scalar("INSERT INTO habits (user_id, name, kind, created_on) VALUES ($1, 'Pesarme', 'custom', '2026-11-01') RETURNING id")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap();
    let month: Uuid = sqlx::query_scalar(
        "INSERT INTO goals (user_id, kind, period_key, starts_on, ends_on, title, progress_type, unit, target)
         VALUES ($1, 'month', '2026-11', '2026-11-01', '2026-11-30', 'Leer 4 libros', 'cumulative', 'libros', 4) RETURNING id",
    )
    .bind(user)
    .fetch_one(pool)
    .await
    .unwrap();
    let week: Uuid = sqlx::query_scalar(
        "INSERT INTO goals (user_id, kind, period_key, starts_on, ends_on, title, parent_goal_id, progress_type, unit, target)
         VALUES ($1, 'week', '2026-W46', '2026-11-09', '2026-11-15', 'Terminar libro', $2, 'cumulative', 'libros', 1) RETURNING id",
    )
    .bind(user)
    .bind(month)
    .fetch_one(pool)
    .await
    .unwrap();
    (user, habit, month, week)
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn linked_habit_updates_single_occurrence_only_when_configured(pool: PgPool) {
    let (user, habit, _, _) = setup(&pool).await;
    let day = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        hooks::mark_linked_habit(&mut tx, user, HabitTrigger::WeightLogged, day)
            .await
            .unwrap(),
        None,
        "sin vínculo no marca nada"
    );
    sqlx::query("INSERT INTO habit_links (user_id, trigger, habit_id) VALUES ($1, 'weight_logged', $2)")
        .bind(user)
        .bind(habit)
        .execute(&mut *tx)
        .await
        .unwrap();
    for _ in 0..3 {
        assert_eq!(
            hooks::mark_linked_habit(&mut tx, user, HabitTrigger::WeightLogged, day)
                .await
                .unwrap(),
            Some(habit)
        );
    }
    tx.commit().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM habit_entries WHERE habit_id = $1")
        .bind(habit)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    // Una marca manual del usuario no se borra al revertir la automática.
    sqlx::query("UPDATE habit_entries SET source = 'manual' WHERE habit_id = $1")
        .bind(habit)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    hooks::unmark_linked_habit(&mut tx, user, HabitTrigger::WeightLogged, day)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM habit_entries WHERE habit_id = $1")
        .bind(habit)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn one_source_contributes_once_to_each_linked_goal(pool: PgPool) {
    let (user, _, month, week) = setup(&pool).await;
    let reading = Uuid::new_v4();
    let day = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
    let mut tx = pool.begin().await.unwrap();
    for _ in 0..2 {
        for g in [month, week] {
            hooks::record_goal_progress(
                &mut tx,
                user,
                g,
                day,
                ProgressValue::Amount(Decimal::ONE),
                "reading",
                reading,
                "",
            )
            .await
            .unwrap();
        }
    }
    tx.commit().await.unwrap();
    let sums: Vec<(Uuid, Decimal)> =
        sqlx::query_as("SELECT goal_id, sum(amount) FROM goal_progress_events WHERE source_id = $1 GROUP BY goal_id ORDER BY goal_id")
            .bind(reading)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(sums.len(), 2);
    assert!(sums.iter().all(|(_, s)| *s == Decimal::ONE), "{sums:?}");
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        hooks::remove_goal_progress(&mut tx, user, "reading", reading, None)
            .await
            .unwrap(),
        2
    );
    tx.commit().await.unwrap();
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn progress_cannot_target_another_users_goal(pool: PgPool) {
    let (_, _, month, _) = setup(&pool).await;
    let t = common::TestApp::new(pool.clone());
    let b: Uuid = t.login(222).await.user_id.parse().unwrap();
    let mut tx = pool.begin().await.unwrap();
    let r = hooks::record_goal_progress(
        &mut tx,
        b,
        month,
        NaiveDate::from_ymd_opt(2026, 11, 12).unwrap(),
        ProgressValue::Completed,
        "manual",
        Uuid::new_v4(),
        "",
    )
    .await;
    assert!(
        r.is_err(),
        "FK compuesta debe impedir aportar al objetivo de otro usuario"
    );
}
