//! Presupuestos mensuales (snapshot por mes) y rubros por proyecto (T39).
//! Al crearse un mes se copian líneas y rubros del mes anterior más reciente; editar un mes no toca los demás.
//! Si una categoría tiene rubros en el mes, su línea = suma de los rubros (se mantiene aquí).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgExecutor};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct BudgetMonth {
    pub id: Uuid,
    pub month: String,
    pub copied_from: Option<String>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct BudgetLine {
    pub category_id: Uuid,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct BudgetItem {
    #[serde(default)]
    pub id: Option<Uuid>,
    pub project_id: Uuid,
    pub category_id: Uuid,
    /// Rubro (ej. Servidores, WhatsApp, Ads).
    pub name: String,
    pub amount_cents: i64,
    /// Palabras clave que sugieren el rubro cuando el gasto no tiene asignación explícita.
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub position: i32,
}

pub async fn get_month(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    month: &str,
) -> RepoResult<Option<BudgetMonth>> {
    Ok(sqlx::query_as::<_, BudgetMonth>(
        "SELECT id, month, copied_from, version, updated_at FROM budget_months WHERE user_id = $1 AND month = $2",
    )
    .bind(user_id)
    .bind(month)
    .fetch_optional(db)
    .await?)
}

/// Mes existente anterior más reciente (para inicializar/proponer).
pub async fn previous_month(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    month: &str,
) -> RepoResult<Option<BudgetMonth>> {
    Ok(sqlx::query_as::<_, BudgetMonth>(
        "SELECT id, month, copied_from, version, updated_at FROM budget_months WHERE user_id = $1 AND month < $2
         ORDER BY month DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(month)
    .fetch_optional(db)
    .await?)
}

pub async fn lines(db: impl PgExecutor<'_>, user_id: Uuid, month_id: Uuid) -> RepoResult<Vec<BudgetLine>> {
    Ok(sqlx::query_as::<_, BudgetLine>(
        "SELECT category_id, amount_cents FROM budget_lines WHERE user_id = $1 AND budget_month_id = $2",
    )
    .bind(user_id)
    .bind(month_id)
    .fetch_all(db)
    .await?)
}

pub async fn items(db: impl PgExecutor<'_>, user_id: Uuid, month_id: Uuid) -> RepoResult<Vec<BudgetItem>> {
    Ok(sqlx::query_as::<_, BudgetItem>(
        "SELECT id, project_id, category_id, name, amount_cents, keywords, position FROM budget_items
         WHERE user_id = $1 AND budget_month_id = $2 ORDER BY project_id, position, name",
    )
    .bind(user_id)
    .bind(month_id)
    .fetch_all(db)
    .await?)
}

/// Crea el snapshot del mes si no existe, copiando el mes anterior más reciente. Seguro con concurrencia.
pub async fn ensure_month(tx: &mut Tx<'_>, user_id: Uuid, month: &str) -> RepoResult<BudgetMonth> {
    if let Some(m) = get_month(&mut **tx, user_id, month).await? {
        return Ok(m);
    }
    let prev = previous_month(&mut **tx, user_id, month).await?;
    let created: Option<BudgetMonth> = sqlx::query_as(
        "INSERT INTO budget_months (user_id, month, copied_from) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, month) DO NOTHING RETURNING id, month, copied_from, version, updated_at",
    )
    .bind(user_id)
    .bind(month)
    .bind(prev.as_ref().map(|p| p.month.clone()))
    .fetch_optional(&mut **tx)
    .await?;
    let Some(m) = created else {
        return get_month(&mut **tx, user_id, month)
            .await?
            .ok_or(RepoError::NotFound);
    };
    if let Some(p) = prev {
        sqlx::query(
            "INSERT INTO budget_lines (user_id, budget_month_id, category_id, amount_cents)
             SELECT user_id, $3, category_id, amount_cents FROM budget_lines WHERE user_id = $1 AND budget_month_id = $2",
        )
        .bind(user_id)
        .bind(p.id)
        .bind(m.id)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO budget_items (user_id, budget_month_id, project_id, category_id, name, amount_cents, keywords, position)
             SELECT user_id, $3, project_id, category_id, name, amount_cents, keywords, position
             FROM budget_items WHERE user_id = $1 AND budget_month_id = $2",
        )
        .bind(user_id)
        .bind(p.id)
        .bind(m.id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(m)
}

/// Reemplaza líneas y rubros del mes (control de versión del mes). Las líneas de categorías con rubros = suma.
pub async fn replace(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    month: &BudgetMonth,
    version: i32,
    new_lines: &[BudgetLine],
    new_items: &[BudgetItem],
) -> RepoResult<BudgetMonth> {
    let bumped: Option<BudgetMonth> = sqlx::query_as(
        "UPDATE budget_months SET version = version + 1 WHERE user_id = $1 AND id = $2 AND version = $3
         RETURNING id, month, copied_from, version, updated_at",
    )
    .bind(user_id)
    .bind(month.id)
    .bind(version)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(bumped) = bumped else {
        return Err(conflict_or_missing(
            get_month(&mut **tx, user_id, &month.month)
                .await?
                .ok_or(RepoError::NotFound),
        ));
    };
    // Rubros: se conservan los IDs existentes (los gastos con asignación explícita siguen apuntando a su rubro);
    // se agregan los nuevos y se eliminan solo los que ya no vienen.
    let keep: Vec<Uuid> = new_items.iter().filter_map(|i| i.id).collect();
    sqlx::query(
        "DELETE FROM budget_items WHERE user_id = $1 AND budget_month_id = $2 AND NOT (id = ANY($3))",
    )
    .bind(user_id)
    .bind(month.id)
    .bind(&keep)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM budget_lines WHERE user_id = $1 AND budget_month_id = $2")
        .bind(user_id)
        .bind(month.id)
        .execute(&mut **tx)
        .await?;
    for l in new_lines {
        sqlx::query(
            "INSERT INTO budget_lines (user_id, budget_month_id, category_id, amount_cents) VALUES ($1, $2, $3, $4)
             ON CONFLICT (budget_month_id, category_id) DO UPDATE SET amount_cents = EXCLUDED.amount_cents",
        )
        .bind(user_id)
        .bind(month.id)
        .bind(l.category_id)
        .bind(l.amount_cents)
        .execute(&mut **tx)
        .await?;
    }
    for (i, it) in new_items.iter().enumerate() {
        let position = if it.position == 0 { i as i32 } else { it.position };
        let updated = match it.id {
            Some(id) => sqlx::query(
                "UPDATE budget_items SET project_id = $4, category_id = $5, name = $6, amount_cents = $7, keywords = $8,
                    position = $9 WHERE user_id = $1 AND budget_month_id = $2 AND id = $3",
            )
            .bind(user_id)
            .bind(month.id)
            .bind(id)
            .bind(it.project_id)
            .bind(it.category_id)
            .bind(it.name.trim())
            .bind(it.amount_cents)
            .bind(&it.keywords)
            .bind(position)
            .execute(&mut **tx)
            .await?
            .rows_affected(),
            None => 0,
        };
        if updated == 0 {
            sqlx::query(
                "INSERT INTO budget_items (user_id, budget_month_id, project_id, category_id, name, amount_cents, keywords, position)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(user_id)
            .bind(month.id)
            .bind(it.project_id)
            .bind(it.category_id)
            .bind(it.name.trim())
            .bind(it.amount_cents)
            .bind(&it.keywords)
            .bind(position)
            .execute(&mut **tx)
            .await?;
        }
    }
    sync_lines_with_items(tx, user_id, month.id).await?;
    Ok(bumped)
}

/// La línea de una categoría con rubros es la suma de sus rubros.
pub async fn sync_lines_with_items(tx: &mut Tx<'_>, user_id: Uuid, month_id: Uuid) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO budget_lines (user_id, budget_month_id, category_id, amount_cents)
         SELECT user_id, budget_month_id, category_id, sum(amount_cents) FROM budget_items
         WHERE user_id = $1 AND budget_month_id = $2 GROUP BY user_id, budget_month_id, category_id
         ON CONFLICT (budget_month_id, category_id) DO UPDATE SET amount_cents = EXCLUDED.amount_cents",
    )
    .bind(user_id)
    .bind(month_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
