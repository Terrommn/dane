//! ClickUp (solo lectura): token personal cifrado, **solo la vista configurada**, tareas del usuario o sin asignar.
//! Estados mapeados a `open | need_info | closed` con un mapa configurable. Orden propuesto para la daily (vault):
//! primero 3–5 abiertas fáciles con justificación determinista, luego el resto, al final `need info`.
//! Marcar una tarea en la app nunca modifica ClickUp.

use std::collections::BTreeMap;

use persistence::repos::integrations as repo;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Endpoints, ProviderError, SyncReport, decrypt_json, encrypt_json, net, read_json};
use crate::state::AppState;

const P: &str = "clickup";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub view_id: Option<String>,
    /// ID numérico del usuario en ClickUp (se detecta con /user al conectar).
    pub clickup_user_id: Option<i64>,
    /// Estado de ClickUp (minúsculas) → `open | need_info | closed`.
    pub status_map: BTreeMap<String, String>,
    /// Área local donde se colocan las tareas (por defecto la de slug `trabajo`).
    pub area_id: Option<Uuid>,
}

pub fn default_status_map() -> BTreeMap<String, String> {
    [("new", "open"), ("need info", "need_info"), ("closed", "closed"), ("complete", "closed"), ("done", "closed")]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[derive(Deserialize)]
struct UserResp {
    user: CuUser,
}

#[derive(Deserialize)]
struct CuUser {
    id: i64,
    username: Option<String>,
    email: Option<String>,
}

#[derive(Deserialize)]
struct TasksPage {
    #[serde(default)]
    tasks: Vec<CuTask>,
    #[serde(default)]
    last_page: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct CuTask {
    pub id: String,
    pub name: String,
    pub url: Option<String>,
    pub status: CuStatus,
    pub priority: Option<CuPriority>,
    pub assignees: Vec<CuAssignee>,
    pub tags: Vec<CuTag>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct CuStatus {
    pub status: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct CuPriority {
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct CuAssignee {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct CuTag {
    pub name: String,
}

pub fn map_status(map: &BTreeMap<String, String>, s: &CuStatus) -> String {
    let name = s.status.trim().to_lowercase();
    if let Some(c) = map.get(&name) {
        return c.clone();
    }
    match s.kind.as_deref() {
        Some("closed") | Some("done") => "closed".into(),
        _ => "open".into(),
    }
}

const EASY: [&str; 16] =
    ["config", "configur", "campo", "campos", "texto", "textos", "copy", "typo", "ajuste", "ajustar", "renombrar", "etiqueta", "label", "color", "permiso", "cambiar"];
const HARD: [&str; 9] = ["pantalla", "automatiz", "integraci", "flujo", "módulo", "modulo", "dashboard", "migraci", "nuevo sistema"];

/// Justificación determinista de "fácil" por nombre/tags; `None` si parece grande.
pub fn ease_reason(name: &str, tags: &[String]) -> Option<String> {
    let hay = format!("{} {}", name, tags.join(" ")).to_lowercase();
    if HARD.iter().any(|h| hay.contains(h)) {
        return None;
    }
    EASY.iter().find(|e| hay.contains(*e)).map(|e| format!("parece ajuste chico ('{e}')"))
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct OrderedTask {
    pub task_id: Uuid,
    pub external_id: String,
    pub title: String,
    pub url: Option<String>,
    pub priority: Option<String>,
    /// `easy | assigned | need_info`
    pub group: String,
    pub reason: Option<String>,
    pub status: String,
}

/// Orden del vault: 3–5 fáciles (heurística; si hay menos de 3, se completan con los nombres más cortos
/// explicándolo), luego el resto de abiertas, al final need info.
pub fn order(tasks: Vec<(Uuid, String, String, Option<String>, Option<String>, Vec<String>, String)>) -> Vec<OrderedTask> {
    // (id, external_id, title, url, priority, tags, category)
    let mut open: Vec<_> = tasks.iter().filter(|t| t.6 == "open").collect();
    let need: Vec<_> = tasks.iter().filter(|t| t.6 == "need_info").collect();
    open.sort_by(|a, b| a.2.cmp(&b.2));
    let mut easy: Vec<(usize, String)> = open.iter().enumerate().filter_map(|(i, t)| ease_reason(&t.2, &t.5).map(|r| (i, r))).take(5).collect();
    if easy.len() < 3 {
        let mut rest: Vec<(usize, usize)> = open
            .iter()
            .enumerate()
            .filter(|(i, t)| !easy.iter().any(|(j, _)| j == i) && !HARD.iter().any(|h| t.2.to_lowercase().contains(h)))
            .map(|(i, t)| (i, t.2.chars().count()))
            .collect();
        rest.sort_by_key(|x| x.1);
        for (i, _) in rest.into_iter().take(3 - easy.len()) {
            easy.push((i, "nombre corto: alcance aparentemente acotado".into()));
        }
    }
    let mk = |t: &&(Uuid, String, String, Option<String>, Option<String>, Vec<String>, String), group: &str, reason: Option<String>| OrderedTask {
        task_id: t.0,
        external_id: t.1.clone(),
        title: t.2.clone(),
        url: t.3.clone(),
        priority: t.4.clone(),
        group: group.into(),
        reason,
        status: t.6.clone(),
    };
    let mut out: Vec<OrderedTask> = easy.iter().map(|(i, r)| mk(&open[*i], "easy", Some(r.clone()))).collect();
    out.extend(open.iter().enumerate().filter(|(i, _)| !easy.iter().any(|(j, _)| j == i)).map(|(_, t)| mk(t, "assigned", None)));
    out.extend(need.iter().map(|t| mk(t, "need_info", Some("bloqueada: necesita información".into()))));
    out
}

/// Valida el token con `/user` y guarda la conexión (token cifrado) y la configuración.
pub async fn connect(state: &AppState, user_id: Uuid, endpoints: &Endpoints, token: &str, mut cfg: Config, extra: serde_json::Value) -> Result<(), ProviderError> {
    let resp = state.http.get(format!("{}/user", endpoints.clickup)).header("Authorization", token).send().await.map_err(net(P))?;
    let me: UserResp = read_json(P, resp).await?;
    cfg.clickup_user_id = cfg.clickup_user_id.or(Some(me.user.id));
    if cfg.status_map.is_empty() {
        cfg.status_map = default_status_map();
    }
    let mut config = serde_json::to_value(&cfg).map_err(anyhow::Error::from)?;
    if let (Some(o), Some(e)) = (config.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    let label = me.user.username.or(me.user.email);
    let enc = encrypt_json(state, &Credentials { token: token.to_string() })?;
    repo::upsert_connected(&state.pool, user_id, P, &["read".to_string()], label.as_deref(), &enc, &config).await?;
    Ok(())
}

pub async fn sync(state: &AppState, user_id: Uuid) -> Result<SyncReport, ProviderError> {
    let mut rep = SyncReport { provider: P.into(), kind: "tasks".into(), ..Default::default() };
    let conn = repo::get(&state.pool, user_id, P).await?.ok_or(ProviderError::NotConnected("ClickUp"))?;
    let endpoints = Endpoints::resolve(state, Some(&conn.config));
    let creds: Credentials = decrypt_json(state, conn.credentials_enc.as_deref().ok_or(ProviderError::NotConnected("ClickUp"))?)?;
    let cfg: Config = serde_json::from_value(conn.config.clone()).unwrap_or_default();
    let view = cfg.view_id.clone().filter(|v| !v.trim().is_empty()).ok_or_else(|| ProviderError::Config("Configura la vista de ClickUp (view_id)".into()))?;
    let me = cfg.clickup_user_id;
    let status_map = if cfg.status_map.is_empty() { default_status_map() } else { cfg.status_map.clone() };
    let area_id = match cfg.area_id {
        Some(a) => Some(a),
        None => sqlx::query_scalar("SELECT id FROM areas WHERE user_id = $1 AND slug = 'trabajo'").bind(user_id).fetch_optional(&state.pool).await?,
    };
    let today = super::today(state, user_id).await;
    let mut all = Vec::new();
    for page in 0..50 {
        let resp = state
            .http
            .get(format!("{}/view/{}/task", endpoints.clickup, url::form_urlencoded::byte_serialize(view.as_bytes()).collect::<String>()))
            .header("Authorization", &creds.token)
            .query(&[("page", page.to_string())])
            .send()
            .await
            .map_err(net(P))?;
        let pg: TasksPage = read_json(P, resp).await?;
        let n = pg.tasks.len();
        all.extend(pg.tasks);
        if pg.last_page.unwrap_or(n == 0) || n == 0 {
            break;
        }
    }
    let mine: Vec<CuTask> = all.into_iter().filter(|t| t.assignees.is_empty() || me.is_none_or(|m| t.assignees.iter().any(|a| a.id == m))).collect();
    let now = state.now();
    let mut rows = Vec::new();
    let mut tx = state.pool.begin().await?;
    for t in &mine {
        let category = map_status(&status_map, &t.status);
        let priority = t.priority.as_ref().and_then(|p| p.priority.clone());
        let tags: Vec<String> = t.tags.iter().map(|x| x.name.clone()).collect();
        let title: String = t.name.trim().chars().take(300).collect();
        if title.is_empty() {
            continue;
        }
        let url = t.url.clone().filter(|u| u.starts_with("https://"));
        let row: (Uuid, String) = sqlx::query_as(
            "INSERT INTO tasks (user_id, title, area_id, status, planned_on, done_at, external_source, external_id, external_url,
                                external_status, external_status_category, external_priority, external_synced_at)
             VALUES ($1, $2, $3, CASE WHEN $8 = 'closed' THEN 'done' ELSE 'open' END, $4, CASE WHEN $8 = 'closed' THEN $10::timestamptz END,
                     'clickup', $5, $6, $7, $8, $9, $10)
             ON CONFLICT (user_id, external_source, external_id) DO UPDATE SET
                title = EXCLUDED.title, external_url = EXCLUDED.external_url, external_status = EXCLUDED.external_status,
                external_status_category = EXCLUDED.external_status_category, external_priority = EXCLUDED.external_priority,
                external_synced_at = EXCLUDED.external_synced_at,
                status = CASE WHEN EXCLUDED.external_status_category = 'closed' AND tasks.status = 'open' THEN 'done' ELSE tasks.status END,
                done_at = CASE WHEN EXCLUDED.external_status_category = 'closed' AND tasks.status = 'open' THEN EXCLUDED.external_synced_at ELSE tasks.done_at END,
                version = tasks.version + 1
             RETURNING id, (xmax = 0)::text",
        )
        .bind(user_id)
        .bind(&title)
        .bind(area_id)
        .bind(today)
        .bind(&t.id)
        .bind(&url)
        .bind(&t.status.status)
        .bind(&category)
        .bind(&priority)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        if row.1 == "true" {
            sqlx::query("INSERT INTO task_events (user_id, task_id, kind, data) VALUES ($1, $2, 'created', $3)")
                .bind(user_id)
                .bind(row.0)
                .bind(serde_json::json!({ "source": "clickup", "status": t.status.status }))
                .execute(&mut *tx)
                .await?;
        }
        rep.upserted += 1;
        rows.push((row.0, t.id.clone(), title, url, priority, tags, category));
    }
    // Pistas de facilidad persistidas (se recalculan en cada sincronización).
    let ordered = order(rows);
    for o in &ordered {
        sqlx::query("UPDATE tasks SET ease_hint = $3 WHERE user_id = $1 AND id = $2")
            .bind(user_id)
            .bind(o.task_id)
            .bind(if o.group == "easy" { o.reason.clone() } else { None })
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(rep)
}

/// Tareas abiertas de ClickUp ordenadas para la daily.
pub async fn ordered_tasks(state: &AppState, user_id: Uuid) -> Result<Vec<OrderedTask>, ProviderError> {
    let rows: Vec<(Uuid, String, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT id, external_id, title, external_url, external_priority, coalesce(external_status_category, 'open')
         FROM tasks WHERE user_id = $1 AND external_source = 'clickup' AND status = 'open' AND archived_at IS NULL",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(order(rows.into_iter().map(|(a, b, c, d, e, f)| (a, b, c, d, e, Vec::new(), f)).collect()))
}

pub async fn disconnect(state: &AppState, user_id: Uuid) -> Result<(), ProviderError> {
    // Los tokens personales de ClickUp no tienen endpoint de revocación: se borran localmente y se indica revocarlo en ClickUp.
    repo::disconnect(&state.pool, user_id, P, "disconnected").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(title: &str, cat: &str) -> (Uuid, String, String, Option<String>, Option<String>, Vec<String>, String) {
        (Uuid::new_v4(), title.into(), title.into(), None, None, vec![], cat.into())
    }

    #[test]
    fn easy_first_then_rest_then_need_info() {
        let o = order(vec![
            t("Nueva pantalla de reportes", "open"),
            t("Cambiar texto del botón", "open"),
            t("Config de webhooks", "open"),
            t("Revisar cliente X", "need_info"),
            t("Ajustar campos del formulario", "open"),
            t("Automatización completa de cobros", "open"),
        ]);
        let groups: Vec<&str> = o.iter().map(|x| x.group.as_str()).collect();
        assert_eq!(&groups[..3], &["easy", "easy", "easy"]);
        assert_eq!(groups.last(), Some(&"need_info"));
        assert!(o.iter().filter(|x| x.group == "easy").all(|x| x.reason.is_some()));
        assert!(!o.iter().any(|x| x.group == "easy" && x.title.contains("pantalla")));
    }

    #[test]
    fn status_mapping_is_configurable() {
        let mut m = default_status_map();
        assert_eq!(map_status(&m, &CuStatus { status: "NEW".into(), kind: Some("open".into()) }), "open");
        assert_eq!(map_status(&m, &CuStatus { status: "need info".into(), kind: Some("custom".into()) }), "need_info");
        assert_eq!(map_status(&m, &CuStatus { status: "shipped".into(), kind: Some("closed".into()) }), "closed");
        m.insert("en espera".into(), "need_info".into());
        assert_eq!(map_status(&m, &CuStatus { status: "En espera".into(), kind: None }), "need_info");
    }
}
