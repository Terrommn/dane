//! T30/T31 contenido y Swipe, T32 escuela, T33 devlog/ADRs/runbooks/TILs/snippets.

mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn content_pipeline_hooks_metrics_and_views(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let hooks: Vec<String> = (1..=10).map(|i| format!("Hook {i}")).collect();
    let (st, p) = a
        .post("/content", json!({ "title": "Reel finanzas", "idea": "Cómo uso mis tarjetas", "hooks": hooks, "chosen_hook": 3, "platforms": ["Instagram", "tiktok"],
            "publish_on": "2026-11-15", "shot_list": [{ "shot": "Plano medio", "text": "30%" }], "raw_url": "https://drive.google.com/raw" }))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{p}");
    let id = p["id"].as_str().unwrap();
    assert_eq!(p["shot_list"][0]["n"], 1);
    assert_eq!(p["platforms"], json!(["instagram", "tiktok"]));
    // Validaciones: 11 hooks, hook elegido inexistente, URL peligrosa, retención > 100.
    let eleven: Vec<String> = (1..=11).map(|i| i.to_string()).collect();
    assert_eq!(
        a.post("/content", json!({ "title": "x", "hooks": eleven }))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        a.post(
            "/content",
            json!({ "title": "x", "hooks": ["a"], "chosen_hook": 4 })
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        a.post(
            "/content",
            json!({ "title": "x", "render_url": "javascript:alert(1)" })
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        a.post("/content", json!({ "title": "x", "retention_pct": "120" }))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        a.post("/content", json!({ "title": "x", "status": "viral" }))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let (_, got) = a.get(&format!("/content/{id}")).await;
    assert_eq!(got["days_to_publish"], 3);
    assert_eq!(got["chosen_hook_text"], "Hook 4");
    let (_, board) = a.get("/content/board").await;
    assert_eq!(board.as_array().unwrap().len(), 5);
    assert_eq!(board[0]["status"], "idea");
    assert_eq!(board[0]["items"][0]["id"], id);

    // Publicar con métricas y ordenar publicados.
    let mut body = got.clone();
    body["status"] = json!("published");
    body["published_on"] = json!("2026-11-12");
    body["views"] = json!(1500);
    body["retention_pct"] = json!("42.5");
    body["render_url"] = json!("https://drive.google.com/render");
    let (st, upd) = a.put(&format!("/content/{id}"), body).await;
    assert_eq!(st, StatusCode::OK, "{upd}");
    for (title, views, ret, day) in [
        ("B", 9000, "20", "2026-11-01"),
        ("C", 300, "70", "2026-11-05"),
        ("D", 10, "5", "2026-10-01"),
        ("E", 1, "1", "2026-09-01"),
        ("F", 2, "2", "2026-08-01"),
    ] {
        a.post("/content", json!({ "title": title, "status": "published", "published_on": day, "views": views, "retention_pct": ret })).await;
    }
    let (_, by_views) = a.get("/content/published").await;
    assert_eq!(by_views[0]["title"], "B");
    let (_, by_ret) = a.get("/content/published?sort=retention").await;
    assert_eq!(by_ret[0]["title"], "C");
    // Contexto para IA: exactamente las 5 publicadas más recientes.
    let (_, ctx) = a.get("/content/ai-context").await;
    let titles: Vec<_> = ctx["recent_published"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["Reel finanzas", "C", "B", "D", "E"]);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn swipe_refs_validate_urls(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let (st, s) = a.post("/swipe", json!({ "url": "https://instagram.com/p/x", "hook": "Nadie te dice esto", "why_it_works": "Curiosidad", "tags": ["finanzas"] })).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(s["title"], "https://instagram.com/p/x");
    assert_eq!(
        a.post("/swipe", json!({ "url": "data:text/html,hi" })).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (_, list) = a.get("/swipe?tag=finanzas").await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (_, list) = a.get("/swipe?q=nadie").await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let b = t.login(222).await;
    assert_eq!(
        b.get(&format!("/swipe/{}", s["id"].as_str().unwrap())).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn school_subjects_notes_and_deliverables(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let (st, subj) = a.post("/subjects", json!({ "name": "Cálculo" })).await;
    assert_eq!(st, StatusCode::CREATED);
    assert!(subj["area_id"].is_string(), "área Escuela por defecto");
    assert_eq!(
        a.post("/subjects", json!({ "name": "Cálculo" })).await.0,
        StatusCode::CONFLICT,
        "materia duplicada"
    );
    let (st, note) = a.post("/class-notes", json!({ "subject_id": subj["id"], "on_date": "2026-11-11", "topic": "Integrales", "topics_md": "- por partes", "questions_md": "- ¿sustitución?" })).await;
    assert_eq!(st, StatusCode::CREATED);
    let nid = note["id"].as_str().unwrap();
    let (st, task) = a
        .post(
            &format!("/class-notes/{nid}/deliverables"),
            json!({ "title": "Entregar ejercicios 3.2", "due_on": "2026-11-14" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(task["class_note_id"], nid);
    assert_eq!(task["area_id"], subj["area_id"]);
    let (_, det) = a.get(&format!("/class-notes/{nid}")).await;
    assert_eq!(det["deliverables"][0]["title"], "Entregar ejercicios 3.2");
    let (_, day) = a.get("/tasks/day/2026-11-14").await;
    assert!(
        day["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["area_name"] == "Escuela" && !g["tasks"].as_array().unwrap().is_empty())
    );
    let (_, subjects) = a.get("/subjects").await;
    assert_eq!(subjects[0]["note_count"], 1);
    let b = t.login(222).await;
    assert_eq!(
        b.post("/class-notes", json!({ "subject_id": subj["id"], "topic": "x" }))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn devlog_sessions_and_pasted_text_import(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let (st, s1) = a
        .post(
            "/devlogs/2026-11-12/sessions",
            json!({ "repo": "dane", "branch": "main", "at_time": "09:15", "changes_md": "- migraciones" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{s1}");
    let text = "### api-negocio — 18:30\nrama: feat/ads\nproyectos: [[negocio]]\n## Qué hice\n- Endpoint de ads\n## Bloqueos / bugs raros\n- Timeout de Meta\n## Decisiones\n- Cachear 1h\n## Siguiente\n- [ ] Pruebas\nproyecto: [[inexistente]]";
    let (_, prev) = a.post("/devlogs/import/preview", json!({ "text": text })).await;
    assert_eq!(prev["parsed"]["repo"], "api-negocio");
    assert_eq!(prev["project_ids"].as_array().unwrap().len(), 1);
    assert_eq!(prev["unknown_projects"], json!(["inexistente"]));
    let (_, none) = a.get("/devlogs/2026-11-13").await;
    assert!(none["devlog"].is_null(), "la vista previa no guarda");
    let (st, s2) = a
        .post("/devlogs/import", json!({ "text": text, "date": "2026-11-12" }))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{s2}");
    assert_eq!(s2["raw_text"], text);
    assert_eq!(s2["at_time"], "18:30:00");
    let (_, day) = a.get("/devlogs/2026-11-12").await;
    assert_eq!(
        day["sessions"].as_array().unwrap().len(),
        2,
        "varias sesiones el mismo día"
    );
    assert_eq!(day["sessions"][0]["repo"], "dane");
    let (_, list) = a.get("/devlogs").await;
    assert_eq!(list[0]["session_count"], 2);
    // Actividad del proyecto Negocio incluye la sesión.
    let negocio = prev["project_ids"][0].as_str().unwrap();
    let (_, act) = a.get(&format!("/projects/{negocio}/activity")).await;
    assert!(
        act.as_array()
            .unwrap()
            .iter()
            .any(|x| x["entity_type"] == "devlog_session")
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn adr_numbering_per_project_and_supersede(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let (_, projects) = a.get("/projects").await;
    let p1 = projects[0]["id"].as_str().unwrap().to_string();
    let p2 = projects[1]["id"].as_str().unwrap().to_string();
    // Concurrentes en el mismo proyecto: números 1..4 sin repetir.
    let futs = (0..4).map(|i| {
        let a = a.clone();
        let p1 = p1.clone();
        async move {
            a.post(
                "/adrs",
                json!({ "project_id": p1, "title": format!("Decisión {i}") }),
            )
            .await
        }
    });
    let results = futures_join_all(futs).await;
    let mut nums: Vec<i64> = results
        .iter()
        .map(|(st, b)| {
            assert_eq!(*st, StatusCode::CREATED, "{b}");
            b["number"].as_i64().unwrap()
        })
        .collect();
    nums.sort();
    assert_eq!(nums, vec![1, 2, 3, 4]);
    let (_, other) = a
        .post(
            "/adrs",
            json!({ "project_id": p2, "title": "Primera de otro proyecto" }),
        )
        .await;
    assert_eq!(other["number"], 1, "numeración por proyecto");
    assert_eq!(other["status"], "proposed");

    let (_, list) = a.get(&format!("/adrs?project_id={p1}")).await;
    let first = list[0].clone();
    let fid = first["id"].as_str().unwrap();
    let newer = list[3]["id"].as_str().unwrap();
    // Reemplazada exige superseded_by; no a sí misma.
    let (st, _) = a
        .put(
            &format!("/adrs/{fid}"),
            json!({ "project_id": p1, "title": "Decisión", "status": "superseded", "version": 1 }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, _) = a.put(&format!("/adrs/{fid}"), json!({ "project_id": p1, "title": "Decisión", "status": "superseded", "superseded_by_id": fid, "version": 1 })).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, up) = a.put(&format!("/adrs/{fid}"), json!({ "project_id": p1, "title": "Decisión", "status": "superseded", "superseded_by_id": newer, "version": 1 })).await;
    assert_eq!(st, StatusCode::OK, "{up}");
    assert_eq!(up["superseded_by_id"], newer);
    let (_, view) = a.get("/views/projects").await;
    assert_eq!(view["adrs"].as_array().unwrap().len(), 5);
    // B no puede crear ADR en proyecto de A.
    let b = t.login(222).await;
    assert_eq!(
        b.post("/adrs", json!({ "project_id": p1, "title": "x" })).await.0,
        StatusCode::NOT_FOUND
    );
    // Runbooks, TILs y snippets son notas con tipo.
    for kind in ["runbook", "til", "snippet"] {
        assert_eq!(
            a.post(
                "/notes",
                json!({ "kind": kind, "title": format!("Un {kind}"), "project_id": p1 })
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    let (_, tils) = a.get("/notes?kind=til").await;
    assert_eq!(tils["total"], 1);
}

async fn futures_join_all<F: std::future::Future>(futs: impl IntoIterator<Item = F>) -> Vec<F::Output> {
    let handles: Vec<_> = futs.into_iter().collect();
    let mut out = Vec::new();
    // Ejecuta en paralelo real con tareas de tokio.
    let mut set = Vec::new();
    for f in handles {
        set.push(Box::pin(f));
    }
    let results = futures_lite_join(set).await;
    out.extend(results);
    out
}

async fn futures_lite_join<F: std::future::Future + Unpin>(mut futs: Vec<F>) -> Vec<F::Output> {
    use std::task::Poll;
    let mut outs: Vec<Option<F::Output>> = (0..futs.len()).map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut pending = false;
        for (i, f) in futs.iter_mut().enumerate() {
            if outs[i].is_none() {
                match std::pin::Pin::new(f).poll(cx) {
                    Poll::Ready(v) => outs[i] = Some(v),
                    Poll::Pending => pending = true,
                }
            }
        }
        if pending { Poll::Pending } else { Poll::Ready(()) }
    })
    .await;
    outs.into_iter().map(|o| o.unwrap()).collect()
}
