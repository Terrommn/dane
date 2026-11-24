//! Contratos compartidos entre módulos (propietario: lead). Se llaman dentro de la transacción de la acción de origen.
//!
//! * `mark_linked_habit`: vínculo explícito y configurable acción → hábito (`habit_links`), nunca por texto.
//!   Actualiza **una sola** ocurrencia (PK `(habit_id, on_date)`).
//! * `record_goal_progress` / `remove_goal_progress`: una fuente aporta como máximo una vez a cada objetivo
//!   (`UNIQUE(goal_id, source_type, source_id)`); vincular la misma acción a un objetivo semanal y a uno mensual
//!   aporta una vez a cada uno, sin multiplicarse.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::{RepoResult, Tx};

/// Disparadores de `habit_links.trigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HabitTrigger {
    WeightLogged,
    ExpenseLogged,
    PositiveReflection,
}

impl HabitTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WeightLogged => "weight_logged",
            Self::ExpenseLogged => "expense_logged",
            Self::PositiveReflection => "positive_reflection",
        }
    }
    fn source(&self) -> &'static str {
        match self {
            Self::WeightLogged => "weight",
            Self::ExpenseLogged => "expense",
            Self::PositiveReflection => "reflection",
        }
    }
}

/// Marca como cumplido el hábito vinculado al disparador en `on_date`, si el usuario configuró el vínculo.
/// Devuelve el `habit_id` afectado. No toca otras fechas ni crea hábitos.
pub async fn mark_linked_habit(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    trigger: HabitTrigger,
    on_date: NaiveDate,
) -> RepoResult<Option<Uuid>> {
    let habit: Option<Uuid> = sqlx::query_scalar(
        "SELECT l.habit_id FROM habit_links l JOIN habits h ON h.id = l.habit_id
         WHERE l.user_id = $1 AND l.trigger = $2 AND h.archived_at IS NULL",
    )
    .bind(user_id)
    .bind(trigger.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(habit_id) = habit else { return Ok(None) };
    sqlx::query(
        "INSERT INTO habit_entries (user_id, habit_id, on_date, status, source) VALUES ($1, $2, $3, 'done', $4)
         ON CONFLICT (habit_id, on_date) DO UPDATE
           SET status = 'done', reason = NULL, version = habit_entries.version + 1
           WHERE habit_entries.status <> 'done'",
    )
    .bind(user_id)
    .bind(habit_id)
    .bind(on_date)
    .bind(trigger.source())
    .execute(&mut **tx)
    .await?;
    Ok(Some(habit_id))
}

/// Revierte la marca automática si ya no hay contenido que la justifique (p. ej. reflexión borrada).
/// Solo borra entradas creadas por ese disparador; una marca manual del usuario se respeta.
pub async fn unmark_linked_habit(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    trigger: HabitTrigger,
    on_date: NaiveDate,
) -> RepoResult<()> {
    sqlx::query(
        "DELETE FROM habit_entries e USING habit_links l
         WHERE l.user_id = $1 AND l.trigger = $2 AND e.habit_id = l.habit_id AND e.on_date = $3 AND e.source = $4",
    )
    .bind(user_id)
    .bind(trigger.as_str())
    .bind(on_date)
    .bind(trigger.source())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Aporte de una fuente a un objetivo. Exactamente uno de los tres campos.
#[derive(Debug, Clone, Copy)]
pub enum ProgressValue {
    /// Incremento acumulativo (1 libro, 30 minutos, 2 sesiones, importe).
    Amount(Decimal),
    /// Medición de valor actual (peso, saldo).
    Value(Decimal),
    /// Hito completado.
    Completed,
}

/// Registra (o actualiza) el aporte de `source` al objetivo. Idempotente por `(goal_id, source_type, source_id)`.
/// Falla con FK si el objetivo no es del mismo usuario.
#[allow(clippy::too_many_arguments)] // Firma de contrato compartido entre agentes.
pub async fn record_goal_progress(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    goal_id: Uuid,
    on_date: NaiveDate,
    value: ProgressValue,
    source_type: &str,
    source_id: Uuid,
    note: &str,
) -> RepoResult<Uuid> {
    let (amount, val, completed) = match value {
        ProgressValue::Amount(a) => (Some(a), None, None),
        ProgressValue::Value(v) => (None, Some(v), None),
        ProgressValue::Completed => (None, None, Some(true)),
    };
    Ok(sqlx::query_scalar(
        "INSERT INTO goal_progress_events (user_id, goal_id, occurred_on, amount, value, completed, source_type, source_id, note)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (goal_id, source_type, source_id) DO UPDATE
           SET occurred_on = EXCLUDED.occurred_on, amount = EXCLUDED.amount, value = EXCLUDED.value,
               completed = EXCLUDED.completed, note = EXCLUDED.note
         RETURNING id",
    )
    .bind(user_id)
    .bind(goal_id)
    .bind(on_date)
    .bind(amount)
    .bind(val)
    .bind(completed)
    .bind(source_type)
    .bind(source_id)
    .bind(note)
    .fetch_one(&mut **tx)
    .await?)
}

/// Quita los aportes de una fuente (a todos sus objetivos o a uno), p. ej. si una lectura deja de estar terminada.
pub async fn remove_goal_progress(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    source_type: &str,
    source_id: Uuid,
    goal_id: Option<Uuid>,
) -> RepoResult<u64> {
    Ok(sqlx::query(
        "DELETE FROM goal_progress_events WHERE user_id = $1 AND source_type = $2 AND source_id = $3
         AND ($4::uuid IS NULL OR goal_id = $4)",
    )
    .bind(user_id)
    .bind(source_type)
    .bind(source_id)
    .bind(goal_id)
    .execute(&mut **tx)
    .await?
    .rows_affected())
}

/// Objetivos a los que una entidad está enlazada explícitamente para aportar (`entity_links.relation = 'contributes_to'`).
pub async fn contributing_goals(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    src_type: &str,
    src_id: Uuid,
) -> RepoResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT l.dst_id FROM entity_links l JOIN goals g ON g.id = l.dst_id AND g.user_id = l.user_id
         WHERE l.user_id = $1 AND l.src_type = $2 AND l.src_id = $3 AND l.dst_type = 'goal' AND l.relation = 'contributes_to'",
    )
    .bind(user_id)
    .bind(src_type)
    .bind(src_id)
    .fetch_all(&mut **tx)
    .await?)
}
