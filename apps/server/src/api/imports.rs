//! T53 Importación del vault: subida (ZIP o carpeta) → dry-run con resumen/advertencias/conflictos → aplicar.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use persistence::repos::imports::{self as repo, ImportRun};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::integrations::importer::{self, ImportError, Limits, Plan, VaultFile};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_runs, upload_zip))
        .routes(routes!(upload_folder))
        .routes(routes!(get_run))
        .routes(routes!(apply_run))
        // La subida de un vault excede el límite general de 2 MB; el tamaño real se valida en el handler.
        .layer(DefaultBodyLimit::max(60 * 1024 * 1024))
}

#[derive(Serialize, ToSchema)]
pub struct PlanItemView {
    pub source_key: String,
    pub path: String,
    pub title: String,
    pub entity_type: String,
    /// `create | link | unchanged | conflict`
    pub action: String,
    pub reason: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct FileRefView {
    pub path: String,
    pub reason: String,
}

#[derive(Serialize, ToSchema)]
pub struct ImportRunView {
    pub run: ImportRun,
    pub items: Vec<PlanItemView>,
    /// Archivos no soportados conservados como referencia (adjuntos, dibujos).
    pub references: Vec<FileRefView>,
    pub ignored: Vec<FileRefView>,
}

fn view(run: ImportRun, plan: &Plan) -> ImportRunView {
    ImportRunView {
        run,
        items: plan
            .items
            .iter()
            .take(2000)
            .map(|i| PlanItemView { source_key: i.source_key.clone(), path: i.path.clone(), title: i.title.clone(), entity_type: i.entity_type.clone(), action: i.action.clone(), reason: i.reason.clone() })
            .collect(),
        references: plan.references.iter().map(|f| FileRefView { path: f.path.clone(), reason: f.reason.clone() }).collect(),
        ignored: plan.ignored.iter().map(|f| FileRefView { path: f.path.clone(), reason: f.reason.clone() }).collect(),
    }
}

fn import_error(e: ImportError) -> ApiError {
    ApiError::validation("file", format!("Importación rechazada: {e}"))
}

async fn dry_run(s: &AppState, user: Uuid, source: &str, files: Vec<VaultFile>) -> ApiResult<ImportRunView> {
    let mut plan = importer::build_plan(&files);
    importer::resolve_actions(s, user, &mut plan).await?;
    let conflicts: Vec<Value> = plan.items.iter().filter(|i| i.action == "conflict").map(|i| json!({ "path": i.path, "title": i.title, "reason": i.reason })).collect();
    let run = repo::create(&s.pool, user, source, &serde_json::to_value(&plan).map_err(anyhow::Error::from)?, &plan.summary(), &json!(plan.warnings), &json!(conflicts)).await?;
    Ok(view(run, &plan))
}

#[derive(Deserialize, IntoParams)]
pub struct UploadQuery {
    /// Nombre del archivo para mostrar.
    pub name: Option<String>,
}

/// Sube un ZIP del vault (`Content-Type: application/zip`, cuerpo binario) y genera el dry-run. No escribe datos.
#[utoipa::path(post, path = "/imports", tag = "imports", params(UploadQuery), request_body(content = Vec<u8>, content_type = "application/zip"),
    responses((status = 201, body = ImportRunView), (status = 422, body = crate::error::ErrorBody)))]
async fn upload_zip(State(s): State<AppState>, auth: AuthUser, headers: HeaderMap, Query(q): Query<UploadQuery>, body: Bytes) -> ApiResult<(StatusCode, Json<ImportRunView>)> {
    if body.len() > s.config.import_max_bytes {
        return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", "El archivo excede el tamaño permitido"));
    }
    let ct = headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("");
    if !(ct.starts_with("application/zip") || ct.starts_with("application/octet-stream") || ct.starts_with("application/x-zip")) {
        return Err(ApiError::validation("content-type", "Envía el ZIP como application/zip"));
    }
    let limits = Limits { max_total_bytes: (s.config.import_max_bytes as u64 * 4).max(50 * 1024 * 1024), ..Limits::default() };
    let files = importer::read_zip(&body, limits).map_err(import_error)?;
    let name = q.name.unwrap_or_else(|| "vault.zip".into()).chars().take(200).collect::<String>();
    let v = dry_run(&s, auth.user_id, &name, files).await?;
    Ok((StatusCode::CREATED, Json(v)))
}

#[derive(Deserialize, ToSchema)]
pub struct FolderFile {
    /// Ruta relativa dentro del vault (ej. `Daily/2026-11-02.md`).
    pub path: String,
    /// Contenido de texto (Markdown). Archivos binarios: omite `content` para registrarlos como referencia.
    pub content: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct FolderUpload {
    pub name: Option<String>,
    pub files: Vec<FolderFile>,
}

/// Sube una carpeta del vault (selección de carpeta en el navegador) y genera el dry-run.
#[utoipa::path(post, path = "/imports/folder", tag = "imports", request_body = FolderUpload, responses((status = 201, body = ImportRunView), (status = 422, body = crate::error::ErrorBody)))]
async fn upload_folder(State(s): State<AppState>, auth: AuthUser, Json(b): Json<FolderUpload>) -> ApiResult<(StatusCode, Json<ImportRunView>)> {
    let limits = Limits::default();
    if b.files.len() > limits.max_files {
        return Err(import_error(ImportError::TooManyFiles(limits.max_files)));
    }
    let mut files = Vec::new();
    let mut total = 0u64;
    for f in b.files {
        let path = importer::safe_path(&f.path).map_err(import_error)?;
        let bytes = f.content.unwrap_or_default().into_bytes();
        if bytes.len() as u64 > limits.max_file_bytes {
            return Err(import_error(ImportError::FileTooLarge(path)));
        }
        total += bytes.len() as u64;
        if total > limits.max_total_bytes {
            return Err(import_error(ImportError::TooLarge));
        }
        files.push(VaultFile { path, bytes });
    }
    let v = dry_run(&s, auth.user_id, &b.name.unwrap_or_else(|| "carpeta".into()), files).await?;
    Ok((StatusCode::CREATED, Json(v)))
}

#[utoipa::path(get, path = "/imports", tag = "imports", responses((status = 200, body = Vec<ImportRun>)))]
async fn list_runs(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<ImportRun>>> {
    Ok(Json(repo::list(&s.pool, auth.user_id).await?))
}

#[utoipa::path(get, path = "/imports/{id}", tag = "imports", responses((status = 200, body = ImportRunView)))]
async fn get_run(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<Json<ImportRunView>> {
    let run = repo::get(&s.pool, auth.user_id, id).await?;
    let (_, plan) = repo::plan(&s.pool, auth.user_id, id).await?;
    let plan: Plan = serde_json::from_value(plan).unwrap_or_default();
    Ok(Json(view(run, &plan)))
}

/// Confirma y aplica un dry-run en una sola transacción. Las acciones se recalculan contra el estado actual
/// (si algo cambió desde el dry-run, se respeta lo que hay en la app).
#[utoipa::path(post, path = "/imports/{id}/apply", tag = "imports", responses((status = 200, body = ImportRunView), (status = 409, body = crate::error::ErrorBody)))]
async fn apply_run(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<Json<ImportRunView>> {
    let (status, plan) = repo::plan(&s.pool, auth.user_id, id).await?;
    if status != "dry_run" {
        return Err(ApiError::new(StatusCode::CONFLICT, "already_applied", "Esta importación ya se aplicó"));
    }
    let mut plan: Plan = serde_json::from_value(plan).map_err(|_| ApiError::internal())?;
    importer::resolve_actions(&s, auth.user_id, &mut plan).await?;
    // Marcar primero evita aplicar dos veces el mismo dry-run en paralelo.
    let claimed = sqlx::query("UPDATE import_runs SET status = 'applied' WHERE user_id = $1 AND id = $2 AND status = 'dry_run'")
        .bind(auth.user_id)
        .bind(id)
        .execute(&s.pool)
        .await?
        .rows_affected();
    if claimed == 0 {
        return Err(ApiError::new(StatusCode::CONFLICT, "already_applied", "Esta importación ya se aplicó"));
    }
    match importer::apply_plan(&s, auth.user_id, id, &plan).await {
        Ok(rep) => {
            let mut warnings = plan.warnings.clone();
            warnings.extend(rep.warnings.iter().cloned());
            let summary = json!({ "plan": plan.summary(), "result": { "created": rep.created, "linked": rep.linked, "skipped": rep.skipped, "conflicts": rep.conflicts } });
            let run = repo::mark(&s.pool, auth.user_id, id, "applied", &summary, &json!(warnings)).await?;
            audit(&s, auth.user_id, "import", Some(id), "apply", summary.clone()).await;
            Ok(Json(view(run, &plan)))
        }
        Err(e) => {
            let run = repo::mark(&s.pool, auth.user_id, id, "failed", &json!({ "error": e }), &json!(plan.warnings)).await?;
            let _ = run;
            Err(ApiError::validation("import", format!("No se aplicó nada: {e}")))
        }
    }
}
