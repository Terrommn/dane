//! T36–T43: finanzas de punta a punta contra PostgreSQL real.
//! Reloj de pruebas: 2026-11-12 18:00 UTC = 12:00 en America/Monterrey.

mod common;

use axum::http::{Method, StatusCode};
use common::{Client, TestApp};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn category(c: &Client, kind: &str, name: &str) -> String {
    let (_, cats) = c.get("/finance/categories").await;
    cats.as_array()
        .unwrap()
        .iter()
        .find(|x| x["kind"] == kind && x["name"] == name)
        .unwrap_or_else(|| panic!("categoría {name}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn account(c: &Client, body: Value) -> String {
    let (st, a) = c.post("/finance/accounts", body).await;
    assert_eq!(st, StatusCode::CREATED, "{a}");
    a["account"]["id"].as_str().unwrap().to_string()
}

async fn debit(c: &Client, name: &str) -> String {
    account(c, json!({ "name": name, "kind": "debit", "bank": name })).await
}

async fn card(c: &Client, name: &str, limit: i64, cut: i16) -> String {
    account(c, json!({ "name": name, "kind": "credit_card", "credit_limit_cents": limit, "cut_day": cut, "pay_day": 3 })).await
}

async fn tx(c: &Client, body: Value) -> Value {
    let (st, r) = c.post("/finance/transactions", body).await;
    assert!(st == StatusCode::CREATED || st == StatusCode::OK, "{st} {r}");
    r["transaction"].clone()
}

async fn snapshot(c: &Client, acc: &str, body: Value) -> Value {
    let (st, v) = c.post(&format!("/finance/accounts/{acc}/snapshots"), body).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    v
}

async fn dashboard(c: &Client, month: &str) -> Value {
    let (st, d) = c.get(&format!("/finance/dashboard?month={month}")).await;
    assert_eq!(st, StatusCode::OK, "{d}");
    d
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn batch_capture_is_exact_and_idempotent_per_item(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let comida = category(&a, "expense", "Comida").await;
    let item = |key: &str, amount: &str| json!({ "kind": "expense", "occurred_on": "2026-11-12", "concept": "Café", "amount": amount, "category_id": comida, "client_key": key });
    let batch = json!({ "items": [item("cafe-0000001", "0.10"), item("cafe-0000002", "0.10"), item("cafe-0000003", "0.20")] });
    let (st, r) = a.post("/finance/transactions/batch", batch.clone()).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert!(
        r["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["created"] == true)
    );
    // Reintento del mismo lote (p. ej. tras perder la respuesta): no duplica.
    let (_, r2) = a.post("/finance/transactions/batch", batch).await;
    assert!(
        r2["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["created"] == false)
    );
    assert_eq!(
        r["results"][0]["transaction"]["id"],
        r2["results"][0]["transaction"]["id"]
    );
    // Dos compras idénticas con claves distintas son legítimas y se conservan.
    let (_, list) = a.get("/finance/transactions?from=2026-11-01&to=2026-11-30").await;
    assert_eq!(list["total"], 3);
    assert_eq!(list["totals"]["expense_cents"], 40, "0.10 + 0.10 + 0.20 exacto");
    // Un elemento inválido rechaza todo el lote (atómico) y dice cuál.
    let (st, e) = a
        .post(
            "/finance/transactions/batch",
            json!({ "items": [item("cafe-0000004", "5"), item("cafe-0000005", "1.999")] }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        e["error"]["field"].as_str().unwrap().starts_with("items[1]"),
        "{e}"
    );
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM transactions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 3);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn card_purchase_payment_and_transfer_are_not_double_counted(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let bank = debit(&a, "Banco").await;
    let savings = account(&a, json!({ "name": "Ahorro", "kind": "savings" })).await;
    let visa = card(&a, "Visa", 2_000_000, 20).await;
    snapshot(
        &a,
        &bank,
        json!({ "balance_cents": 0, "observed_at": "2026-11-01T06:00:00Z" }),
    )
    .await;
    snapshot(
        &a,
        &visa,
        json!({ "balance_cents": 2_000_000, "observed_at": "2026-11-01T06:00:00Z" }),
    )
    .await;
    let sueldo = category(&a, "income", "Sueldo").await;
    let comida = category(&a, "expense", "Comida").await;
    tx(&a, json!({ "kind": "income", "occurred_on": "2026-11-02", "concept": "Sueldo", "amount_cents": 1_000_000, "category_id": sueldo, "account_id": bank })).await;
    tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-03", "concept": "Súper", "amount_cents": 100_000, "category_id": comida, "account_id": visa })).await;
    tx(&a, json!({ "kind": "card_payment", "occurred_on": "2026-11-10", "concept": "Pago Visa", "amount_cents": 100_000, "account_id": bank, "counter_account_id": visa })).await;
    tx(&a, json!({ "kind": "transfer", "occurred_on": "2026-11-11", "concept": "Ahorro", "amount_cents": 50_000, "account_id": bank, "counter_account_id": savings })).await;

    let d = dashboard(&a, "2026-11").await;
    let tot = &d["totals"];
    assert_eq!(
        tot["consumption_cents"], 100_000,
        "la compra con tarjeta cuenta una sola vez"
    );
    assert_eq!(
        tot["debt_payments_cents"], 100_000,
        "el pago de tarjeta no es consumo"
    );
    assert_eq!(tot["earned_income_cents"], 1_000_000);
    assert_eq!(
        tot["consumption_net_cents"], 900_000,
        "neto corregido (el del vault daría 800,000)"
    );
    assert_eq!(
        tot["cash_out_cents"], 100_000,
        "la transferencia entre mis cuentas es neutra"
    );
    assert_eq!(tot["cash_net_cents"], 900_000);
    assert_eq!(d["spent_cents"], 100_000);

    let (_, accounts) = a.get("/finance/accounts").await;
    let est = |id: &str| {
        accounts
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["account"]["id"] == id)
            .unwrap()["estimated_cents"]
            .clone()
    };
    assert_eq!(est(&bank), 1_000_000 - 100_000 - 50_000);
    assert_eq!(
        est(&visa),
        2_000_000,
        "compra y pago se compensan en el disponible"
    );
    assert_eq!(est(&savings), Value::Null, "sin snapshot no se inventa saldo");
    // Una transferencia a una tarjeta no es transferencia: debe registrarse como pago.
    let (st, _) = a
        .post("/finance/transactions", json!({ "kind": "transfer", "occurred_on": "2026-11-12", "concept": "x", "amount_cents": 1, "account_id": bank, "counter_account_id": visa }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn reconciliation_same_day_retroactive_and_legacy_semantics(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let bank = debit(&a, "BBVA").await;
    let comida = category(&a, "expense", "Comida").await;
    let exp = |on: &str, cents: i64| json!({ "kind": "expense", "occurred_on": on, "concept": "Gasto", "amount_cents": cents, "category_id": comida, "account_id": bank });
    // Movimiento de hoy registrado ANTES de observar el saldo: ya está reflejado en él.
    tx(&a, exp("2026-11-12", 1_000)).await;
    t.clock.advance(chrono::Duration::minutes(30));
    let v = snapshot(&a, &bank, json!({ "balance_cents": 50_000 })).await;
    assert_eq!(v["estimated_cents"], 50_000);
    assert_eq!(v["snapshot_age_days"], 0);
    t.clock.advance(chrono::Duration::minutes(30));
    // Ajuste del mismo día registrado después: sí se resta.
    tx(&a, exp("2026-11-12", 2_000)).await;
    // Retroactivo (ayer) cargado hoy: el saldo observado ya lo incluía; no se resta otra vez.
    tx(&a, exp("2026-11-11", 4_000)).await;
    // Con hora explícita anterior a la observación: tampoco.
    tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-12", "occurred_at": "2026-11-12T17:00:00Z", "concept": "Antes", "amount_cents": 8_000, "category_id": comida, "account_id": bank })).await;
    let (_, acc) = a.get(&format!("/finance/accounts/{bank}")).await;
    assert_eq!(acc["estimated_cents"], 48_000);

    // Semántica legacy de cuentas del vault: saldo al iniciar el día base → incluye gastos de ese día.
    let legacy = debit(&a, "Legacy").await;
    tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-10", "concept": "Base", "amount_cents": 300, "category_id": comida, "account_id": legacy })).await;
    tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-09", "concept": "Antes", "amount_cents": 700, "category_id": comida, "account_id": legacy })).await;
    let v = snapshot(&a, &legacy, json!({ "balance_cents": 10_000, "observed_on": "2026-11-10", "semantics": "legacy_day_start_inclusive" })).await;
    assert_eq!(v["estimated_cents"], 9_700);
    // Tarjetas del vault: solo después del día base.
    let visa = card(&a, "Legacy card", 100_000, 5).await;
    tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-10", "concept": "Base", "amount_cents": 300, "category_id": comida, "account_id": visa })).await;
    tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-11", "concept": "Después", "amount_cents": 500, "category_id": comida, "account_id": visa })).await;
    let v = snapshot(
        &a,
        &visa,
        json!({ "balance_cents": 50_000, "observed_on": "2026-11-10", "semantics": "legacy_after_day" }),
    )
    .await;
    assert_eq!(v["estimated_cents"], 49_500);
    assert_eq!(v["usage_pct"], 51);
    // Legacy sin fecha base: inválido.
    let (st, _) = a
        .post(
            &format!("/finance/accounts/{visa}/snapshots"),
            json!({ "balance_cents": 1, "semantics": "legacy_after_day" }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn foreign_currency_requires_explicit_conversion(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let bank = debit(&a, "Banco").await;
    let free = category(&a, "income", "Freelance").await;
    // USD sobre cuenta MXN sin conversión: rechazado (no se suman monedas distintas).
    let (st, _) = a
        .post("/finance/transactions", json!({ "kind": "income", "occurred_on": "2026-11-05", "concept": "Cliente", "amount_cents": 10_000, "currency": "USD", "category_id": free, "account_id": bank }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    // Sin fuente de la tasa: rechazado.
    let (st, _) = a
        .post("/finance/transactions", json!({ "kind": "income", "occurred_on": "2026-11-05", "concept": "Cliente", "category_id": free, "account_id": bank,
            "original": { "amount_cents": 10_000, "currency": "USD", "fx_rate": "17.25", "fx_date": "2026-11-05", "fx_source": " " } }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let t1 = tx(&a, json!({ "kind": "income", "occurred_on": "2026-11-05", "concept": "Cliente", "category_id": free, "account_id": bank,
        "original": { "amount": "100.00", "currency": "usd", "fx_rate": "17.2345", "fx_date": "2026-11-05", "fx_source": "App del banco" } })).await;
    assert_eq!(t1["amount_cents"], 172_345);
    assert_eq!(t1["currency"], "MXN");
    assert_eq!(t1["original_amount_cents"], 10_000);
    assert_eq!(t1["original_currency"], "USD");
    // Sin tasa y sin cuenta: se guarda en USD y queda fuera de los totales en MXN.
    tx(&a, json!({ "kind": "income", "occurred_on": "2026-11-06", "concept": "Otro cliente", "amount_cents": 5_000, "currency": "USD", "category_id": free })).await;
    let d = dashboard(&a, "2026-11").await;
    assert_eq!(d["totals"]["earned_income_cents"], 172_345);
    assert_eq!(d["totals"]["excluded_foreign_count"], 1);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn family_loan_counts_once_and_repayment_is_not_consumption(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let uid: Uuid = a.user_id.parse().unwrap();
    let sister: Uuid = sqlx::query_scalar(
        "INSERT INTO people (user_id, name, relationship) VALUES ($1, 'Ana', 'familia') RETURNING id",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (_, s) = a.get("/settings").await;
    a.put(
        "/settings",
        json!({ "version": s["version"], "track_family_loans": true }),
    )
    .await;
    let bank = debit(&a, "Banco").await;
    let comida = category(&a, "expense", "Comida").await;
    // Pagado por otra persona: no sale de mis cuentas.
    let (st, _) = a
        .post("/finance/transactions", json!({ "kind": "expense", "occurred_on": "2026-11-03", "concept": "Cena", "amount_cents": 30_000, "category_id": comida, "account_id": bank, "financed_by_person_id": sister }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let lent = tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-03", "concept": "Cena", "amount_cents": 30_000, "category_id": comida, "financed_by_person_id": sister })).await;
    let debt_id = lent["debt_id"]
        .as_str()
        .expect("seguimiento configurado crea la deuda familiar")
        .to_string();
    let (_, debts) = a.get("/finance/debts").await;
    assert_eq!(debts[0]["debt"]["kind"], "family");
    assert_eq!(debts[0]["remaining_cents"], 30_000);
    tx(&a, json!({ "kind": "debt_payment", "occurred_on": "2026-11-10", "concept": "Devolución a Ana", "amount_cents": 30_000, "account_id": bank, "debt_id": debt_id })).await;
    let (_, d) = a.get(&format!("/finance/debts/{debt_id}")).await;
    assert_eq!(d["debt"]["status"], "paid");
    assert_eq!(d["remaining_cents"], 0);
    let dash = dashboard(&a, "2026-11").await;
    assert_eq!(
        dash["totals"]["consumption_cents"], 30_000,
        "el gasto financiado cuenta una vez"
    );
    assert_eq!(
        dash["totals"]["debt_payments_cents"], 30_000,
        "devolverlo no es otro consumo"
    );
    assert_eq!(dash["totals"]["cash_out_cents"], 30_000);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn paid_debt_completes_linked_goal_once_partial_does_not(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let uid: Uuid = a.user_id.parse().unwrap();
    let goal: Uuid = sqlx::query_scalar(
        "INSERT INTO goals (user_id, kind, period_key, starts_on, ends_on, title, progress_type)
         VALUES ($1, 'month', '2026-11', '2026-11-01', '2026-11-30', 'Liquidar préstamo', 'milestone') RETURNING id",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    let bank = debit(&a, "Banco").await;
    let (st, d) = a.post("/finance/debts", json!({ "name": "Préstamo auto", "kind": "loan", "principal_cents": 100_000, "opened_on": "2026-10-01", "goal_id": goal })).await;
    assert_eq!(st, StatusCode::CREATED, "{d}");
    let debt = d["debt"]["id"].as_str().unwrap().to_string();
    let pay = |cents: i64, key: &str| json!({ "kind": "debt_payment", "occurred_on": "2026-11-05", "concept": "Abono", "amount_cents": cents, "account_id": bank, "debt_id": debt, "client_key": key });
    tx(&a, pay(40_000, "abono-000001")).await;
    let events = |pool: PgPool| async move {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM goal_progress_events WHERE source_type = 'debt'")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    assert_eq!(events(pool.clone()).await, 0, "abono parcial no completa el hito");
    let (_, v) = a.get(&format!("/finance/debts/{debt}")).await;
    assert_eq!(v["remaining_cents"], 60_000);
    let last = tx(&a, pay(60_000, "abono-000002")).await;
    let (_, v) = a.get(&format!("/finance/debts/{debt}")).await;
    assert_eq!(v["debt"]["status"], "paid");
    assert_eq!(events(pool.clone()).await, 1);
    // Reintento del mismo pago: no duplica ni el pago ni el hito.
    tx(&a, pay(60_000, "abono-000002")).await;
    assert_eq!(events(pool.clone()).await, 1);
    // Anular el último abono reabre la deuda y retira el aporte.
    let (st, _) = a
        .post(
            &format!("/finance/transactions/{}/void", last["id"].as_str().unwrap()),
            json!({ "version": last["version"] }),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let (_, v) = a.get(&format!("/finance/debts/{debt}")).await;
    assert_eq!(v["debt"]["status"], "open");
    assert_eq!(events(pool).await, 0);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn recurring_rules_short_months_catch_up_and_concurrency(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let subs = category(&a, "expense", "Suscripciones").await;
    for (name, kind, day) in [
        ("Día 31", "day", Some(31)),
        ("Día 29", "day", Some(29)),
        ("Último", "last", None),
        ("Penúltimo", "penultimate", None),
    ] {
        let (st, r) = a
            .post("/finance/recurring", json!({ "kind": "subscription", "name": name, "amount_cents": 10_000, "day_kind": kind, "day": day, "category_id": subs, "starts_on": "2026-01-01" }))
            .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
    }
    // Dos procesos materializando a la vez (p. ej. dos réplicas del server tras reiniciar).
    let s1 = t.state.clone();
    let s2 = t.state.clone();
    let (r1, r2) = tokio::join!(
        server::api::finance::materialize_all(&s1),
        server::api::finance::materialize_all(&s2)
    );
    assert_eq!(
        r1.unwrap() + r2.unwrap(),
        40,
        "4 reglas × 10 meses vencidos (ene–oct), cada ocurrencia una vez"
    );
    let (_, again) = a.post("/finance/recurring/materialize", json!({})).await;
    assert_eq!(again["created"], 0);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM transactions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 40);
    let date = |concept: &str, period: &str| {
        let pool = pool.clone();
        let concept = concept.to_string();
        let period = period.to_string();
        async move {
            sqlx::query_scalar::<_, chrono::NaiveDate>(
                "SELECT occurred_on FROM transactions WHERE concept = $1 AND recurrence_period = $2",
            )
            .bind(format!("Suscripción {concept}"))
            .bind(period)
            .fetch_one(&pool)
            .await
            .unwrap()
            .to_string()
        }
    };
    assert_eq!(date("Día 31", "2026-02").await, "2026-02-28");
    assert_eq!(date("Día 31", "2026-04").await, "2026-04-30");
    assert_eq!(date("Día 29", "2026-02").await, "2026-02-28");
    assert_eq!(date("Penúltimo", "2026-02").await, "2026-02-27");
    assert_eq!(date("Último", "2026-10").await, "2026-10-31");
    // Noviembre: todas caen después de hoy (12) → proyectadas, no registradas.
    let (_, occ) = a.get("/finance/recurring/occurrences?month=2026-11").await;
    assert_eq!(occ.as_array().unwrap().len(), 4);
    assert!(
        occ.as_array().unwrap().iter().all(|o| o["state"] == "projected"),
        "{occ}"
    );
    // Lo pendiente de conciliar no afecta lo confirmado.
    let d = dashboard(&a, "2026-10").await;
    assert_eq!(d["totals"]["consumption_cents"], 0);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn confirming_occurrence_updates_same_row_once(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let uid: Uuid = a.user_id.parse().unwrap();
    let habit: Uuid = sqlx::query_scalar("INSERT INTO habits (user_id, name, kind, created_on) VALUES ($1, 'Registrar gastos', 'custom', '2026-11-01') RETURNING id")
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO habit_links (user_id, trigger, habit_id) VALUES ($1, 'expense_logged', $2)")
        .bind(uid)
        .bind(habit)
        .execute(&pool)
        .await
        .unwrap();
    let subs = category(&a, "expense", "Suscripciones").await;
    let card_id = card(&a, "Visa", 1_000_000, 20).await;
    let (_, rule) = a
        .post("/finance/recurring", json!({ "kind": "subscription", "name": "Netflix", "amount_cents": 29_900, "day_kind": "day", "day": 5, "category_id": subs, "account_id": card_id, "starts_on": "2026-11-01" }))
        .await;
    a.post("/finance/recurring/materialize", json!({})).await;
    let (_, occ) = a.get("/finance/recurring/occurrences?month=2026-11").await;
    assert_eq!(occ[0]["state"], "pending");
    let id = occ[0]["transaction_id"].as_str().unwrap().to_string();
    let d = dashboard(&a, "2026-11").await;
    assert_eq!(
        d["totals"]["consumption_cents"], 0,
        "pendiente de conciliar: solo proyección"
    );
    assert_eq!(d["subscriptions_month_cents"], 29_900);
    assert!(d["projection_cents"].as_i64().unwrap() >= 29_900);
    let (_, c1) = a
        .post(
            &format!("/finance/transactions/{id}/confirm"),
            json!({ "version": occ[0]["transaction_version"], "amount_cents": 30_900 }),
        )
        .await;
    assert_eq!(c1["status"], "confirmed");
    assert_eq!(c1["id"], id.as_str());
    // Confirmar otra vez y rematerializar no crea nada nuevo ni repite efectos.
    let (st, _) = a
        .post(
            &format!("/finance/transactions/{id}/confirm"),
            json!({ "version": c1["version"] }),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    a.post("/finance/recurring/materialize", json!({})).await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM transactions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM habit_entries WHERE habit_id = $1")
        .bind(habit)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(entries, 1, "registrar gasto → hábito, una sola ocurrencia");
    let d = dashboard(&a, "2026-11").await;
    assert_eq!(d["totals"]["consumption_cents"], 30_900);
    assert_eq!(
        d["subscriptions_month_cents"], 30_900,
        "la suscripción se cuenta una vez con su monto real"
    );
    assert_eq!(d["variable_spent_cents"], 0, "lo fijo no entra al ritmo variable");
    // Cambiar o cancelar la regla no altera la ocurrencia histórica.
    let (st, _) = a
        .put(&format!("/finance/recurring/{}", rule["id"].as_str().unwrap()), json!({ "kind": "subscription", "name": "Netflix", "amount_cents": 39_900, "day_kind": "day", "day": 5, "category_id": subs, "status": "cancelled", "version": rule["version"] }))
        .await;
    assert_eq!(st, StatusCode::OK);
    let amount: i64 = sqlx::query_scalar("SELECT amount_cents FROM transactions WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(amount, 30_900);
    let (_, dec) = a.get("/finance/recurring/occurrences?month=2026-12").await;
    assert_eq!(
        dec.as_array().unwrap().len(),
        0,
        "regla cancelada: sin proyección futura"
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn card_suggestion_explains_exclusions(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let bank = account(
        &a,
        json!({ "name": "Débito BBVA", "kind": "debit", "is_default_debit": true }),
    )
    .await;
    let _ = bank;
    let good = card(&a, "Buena", 1_000_000, 10).await; // corte pasó el 10 → 28 días
    let near = card(&a, "Cerca", 1_000_000, 20).await; // 8 días
    let today_cut = card(&a, "Corte hoy", 1_000_000, 12).await;
    let used = card(&a, "Usada", 100_000, 1).await;
    for (c, avail) in [
        (&good, 1_000_000),
        (&near, 1_000_000),
        (&today_cut, 1_000_000),
        (&used, 75_000),
    ] {
        snapshot(&a, c, json!({ "balance_cents": avail })).await;
    }
    let (_, s) = a.get("/finance/card-suggestion?amount_cents=50000").await;
    assert_eq!(s["card_id"], good.as_str(), "{s}");
    assert_eq!(s["provisional"], false);
    let reasons = |name: &str| {
        s["cards"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()["reasons"]
            .to_string()
    };
    assert!(reasons("Corte hoy").contains("corte"));
    assert!(reasons("Usada").contains("30%"), "{}", reasons("Usada"));
    // Sin monto: provisional.
    let (_, s) = a.get("/finance/card-suggestion").await;
    assert_eq!(s["provisional"], true);
    // Plan excluye la buena y la cercana tiene deuda abierta prohibida → solo queda débito.
    let (_, plan) = a.get("/finance/credit-plan").await;
    let (st, _) = a
        .put("/finance/credit-plan", json!({ "version": plan["version"], "max_usage_pct": 30, "excluded_account_ids": [good], "rules": [{ "text": "Pagar siempre el total" }] }))
        .await;
    assert_eq!(st, StatusCode::OK);
    a.post("/finance/debts", json!({ "name": "Saldo Cerca", "kind": "credit_card", "account_id": near, "opened_on": "2026-11-01" })).await;
    // Usada: 25% usado + $100 → 35% > 30% (excluida); las demás excluidas por plan, deuda o corte.
    let (_, s) = a.get("/finance/card-suggestion?amount_cents=10000").await;
    assert_eq!(s["card_id"], Value::Null, "{s}");
    assert_eq!(s["debit_account_name"], "Débito BBVA");
    assert!(s["explanation"].as_str().unwrap().contains("débito"));
    // Tarjeta prevista para cancelar.
    let (_, view) = a.get(&format!("/finance/accounts/{used}")).await;
    a.put(&format!("/finance/accounts/{used}"), json!({ "name": "Usada", "kind": "credit_card", "credit_limit_cents": 100_000, "cut_day": 1, "status": "to_cancel", "version": view["account"]["version"] })).await;
    let (_, s) = a.get("/finance/card-suggestion?amount_cents=1000").await;
    let usada = s["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Usada")
        .unwrap()
        .clone();
    assert!(usada["reasons"].to_string().contains("cancelar"));
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn budget_snapshots_items_and_project_report(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let comida = category(&a, "expense", "Comida").await;
    let negocio = category(&a, "expense", "Negocio").await;
    let (_, projects) = a.get("/projects").await;
    let proj = projects
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["slug"] == "negocio")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let items = json!([
        { "project_id": proj, "category_id": negocio, "name": "Servidores", "amount_cents": 30_000, "keywords": ["Render", "servidor"] },
        { "project_id": proj, "category_id": negocio, "name": "WhatsApp", "amount_cents": 100_000, "keywords": ["WhatsApp"] },
        { "project_id": proj, "category_id": negocio, "name": "Ads", "amount_cents": 650_000, "keywords": ["Ads", "anuncio"] }
    ]);
    let (st, oct) = a
        .put("/finance/budgets/2026-10", json!({ "version": 0, "lines": [{ "category_id": comida, "amount_cents": 300_000 }, { "category_id": negocio, "amount_cents": 1 }], "items": items }))
        .await;
    assert_eq!(st, StatusCode::OK, "{oct}");
    let line = |b: &Value, cat: &str| {
        b["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["category_id"] == cat)
            .unwrap()["amount_cents"]
            .clone()
    };
    assert_eq!(
        line(&oct, &negocio),
        780_000,
        "la línea de la categoría es la suma de rubros"
    );
    // Noviembre aún sin snapshot: propuesta copiada de octubre.
    let (_, nov) = a.get("/finance/budgets/2026-11").await;
    assert_eq!(nov["persisted"], false);
    assert_eq!(nov["copied_from"], "2026-10");
    assert_eq!(line(&nov, &comida), 300_000);
    // El primer gasto del mes crea el snapshot de noviembre (copia).
    let item_id = |b: &Value, name: &str| {
        b["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-03", "concept": "Ads de Facebook", "amount_cents": 200_000, "category_id": negocio, "project_id": proj })).await;
    let (_, nov) = a.get("/finance/budgets/2026-11").await;
    assert_eq!(nov["persisted"], true);
    let whatsapp_nov = item_id(&nov, "WhatsApp");
    // Asignación explícita prevalece sobre la palabra clave "Render".
    tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-04", "concept": "Render servidor", "amount_cents": 50_000, "category_id": negocio, "project_id": proj, "budget_item_id": whatsapp_nov })).await;
    tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-05", "concept": "Dominio", "amount_cents": 10_000, "category_id": negocio, "project_id": proj })).await;
    // Editar noviembre no reescribe octubre.
    let (st, _) = a.put("/finance/budgets/2026-11", json!({ "version": nov["version"], "lines": [{ "category_id": comida, "amount_cents": 450_000 }], "items": nov["items"] })).await;
    assert_eq!(st, StatusCode::OK);
    let (_, oct2) = a.get("/finance/budgets/2026-10").await;
    assert_eq!(line(&oct2, &comida), 300_000);
    // Versión vieja: conflicto.
    let (st, _) = a
        .put(
            "/finance/budgets/2026-11",
            json!({ "version": nov["version"], "lines": [] }),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);

    let (st, rep) = a
        .get(&format!("/finance/projects/{proj}/report?month=2026-11"))
        .await;
    assert_eq!(st, StatusCode::OK, "{rep}");
    let r = |name: &str| {
        rep["rubros"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == name)
            .unwrap()["spent_cents"]
            .clone()
    };
    assert_eq!(r("Ads"), 200_000, "por palabra clave");
    assert_eq!(r("WhatsApp"), 50_000, "asignación explícita");
    assert_eq!(r("Servidores"), 0);
    assert_eq!(r("Sin rubro"), 10_000);
    assert_eq!(rep["spent_cents"], 260_000);
    assert_eq!(rep["budget_cents"], 780_000);
    // Dashboard: presupuesto restante y categoría vs presupuesto.
    let d = dashboard(&a, "2026-11").await;
    assert_eq!(d["spending_cap_cents"], 450_000 + 780_000);
    assert_eq!(d["budget_remaining_cents"], 450_000 + 780_000 - 260_000);
    let neg = d["by_category"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["category_name"] == "Negocio")
        .unwrap()
        .clone();
    assert_eq!(neg["remaining_cents"], 780_000 - 260_000);
    assert_eq!(d["by_project"][0]["spent_cents"], 260_000);
    assert_eq!(d["top_expenses"][0]["amount_cents"], 200_000);
    // Proyección: 260,000 en 12 días de 30 → 650,000 (sin suscripciones).
    assert_eq!(d["projection_cents"], 650_000);
    assert_eq!(d["daily"][11]["spent_cents"], 260_000);
    assert_eq!(d["daily"][12]["spent_cents"], Value::Null);
    assert_eq!(d["daily"][29]["pace_cents"], 1_230_000);
    assert_eq!(d["last_six_months"].as_array().unwrap().len(), 6);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn finance_data_is_isolated_between_users(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let b = t.login(222).await;
    let bank = debit(&a, "Banco A").await;
    let comida = category(&a, "expense", "Comida").await;
    let t1 = tx(&a, json!({ "kind": "expense", "occurred_on": "2026-11-12", "concept": "Privado", "amount_cents": 1_000, "category_id": comida, "account_id": bank })).await;
    let id = t1["id"].as_str().unwrap();
    assert_eq!(
        b.get(&format!("/finance/transactions/{id}")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        b.get(&format!("/finance/accounts/{bank}")).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, list) = b.get("/finance/transactions").await;
    assert_eq!(list["total"], 0);
    // B intenta usar la cuenta y la categoría de A.
    let b_comida = category(&b, "expense", "Comida").await;
    let (st, _) = b
        .post("/finance/transactions", json!({ "kind": "expense", "occurred_on": "2026-11-12", "concept": "x", "amount_cents": 1, "category_id": b_comida, "account_id": bank }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, _) = b
        .post("/finance/transactions", json!({ "kind": "expense", "occurred_on": "2026-11-12", "concept": "x", "amount_cents": 1, "category_id": comida }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, _) = b
        .req(
            Method::POST,
            &format!("/finance/transactions/{id}/void"),
            Some(json!({ "version": 1 })),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let d = dashboard(&b, "2026-11").await;
    assert_eq!(d["totals"]["consumption_cents"], 0);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn edits_use_versions_and_period_summary_contract(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let uid: Uuid = a.user_id.parse().unwrap();
    let comida = category(&a, "expense", "Comida").await;
    let body = json!({ "kind": "expense", "occurred_on": "2026-11-02", "concept": "Tacos", "amount_cents": 15_000, "category_id": comida });
    let t1 = tx(&a, body.clone()).await;
    let id = t1["id"].as_str().unwrap();
    let mut upd = body.clone();
    upd["amount_cents"] = json!(18_000);
    upd["version"] = t1["version"].clone();
    let (st, t2) = a.put(&format!("/finance/transactions/{id}"), upd.clone()).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(t2["amount_cents"], 18_000);
    let (st, conflict) = a.put(&format!("/finance/transactions/{id}"), upd).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["current"]["amount_cents"], 18_000);
    // Cambiar el tipo no se permite (se anula y se registra otro).
    let mut wrong = body.clone();
    wrong["kind"] = json!("income");
    wrong["version"] = t2["version"].clone();
    assert_eq!(
        a.put(&format!("/finance/transactions/{id}"), wrong).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let s = persistence::repos::finance::period_summary(
        &pool,
        uid,
        chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        chrono::NaiveDate::from_ymd_opt(2026, 11, 30).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(s.consumption_cents, 18_000);
    assert_eq!(s.cash_out_cents, 18_000);
    let (_, list) = a.get("/finance/transactions?q=taco&sort=amount_desc").await;
    assert_eq!(list["total"], 1, "búsqueda sin acentos ni mayúsculas");
}
