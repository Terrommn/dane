//! T34 notas, búsqueda global, enlaces/backlinks, favoritos, archivados; T35 dibujos.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn search_is_accent_insensitive_prefix_and_isolated(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let b = t.login(222).await;
    a.post("/notes", json!({ "title": "Canción para la oración", "body_md": "Reflexión sobre la **pasión**", "tags": ["#Fe", "musica"] })).await;
    a.post("/people", json!({ "name": "José Martínez", "aliases": ["Pepe"] }))
        .await;
    a.post("/tasks", json!({ "title": "Revisar migración de índices" }))
        .await;
    b.post("/notes", json!({ "title": "Canción secreta de B" })).await;

    let (st, hits) = a.get("/search?q=cancion%20oracion").await;
    assert_eq!(st, StatusCode::OK, "{hits}");
    let hits = hits.as_array().unwrap();
    assert_eq!(hits.len(), 1, "solo la nota de A: {hits:?}");
    assert_eq!(hits[0]["entity_type"], "note");
    // Prefijo y alias.
    let (_, h) = a.get("/search?q=migra").await;
    assert_eq!(h[0]["entity_type"], "task");
    let (_, h) = a.get("/search?q=pepe").await;
    assert_eq!(h[0]["title"], "José Martínez");
    let (_, h) = a.get("/search?q=jose&types=task").await;
    assert!(h.as_array().unwrap().is_empty(), "filtro por módulo");
    // Tolerante a errores de un acento distinto y mayúsculas.
    let (_, h) = a.get("/search?q=PASIÓN").await;
    assert_eq!(h.as_array().unwrap().len(), 1);
    // Inyección: se sanea, no rompe.
    let (st, _) = a.get("/search?q=%27%29%3B%20DROP%20TABLE%20notes%3B--").await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = a.get("/search?q=%21%21").await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    // B no ve nada de A.
    let (_, hb) = b.get("/search?q=jose").await;
    assert!(hb.as_array().unwrap().is_empty());
    // Tags normalizados.
    let (_, tags) = a.get("/tags").await;
    assert!(tags.as_array().unwrap().iter().any(|t| t["tag"] == "fe"));
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn archived_items_hidden_from_search_until_requested_and_restorable(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let (_, n) = a
        .post(
            "/notes",
            json!({ "title": "Runbook de despliegue", "kind": "runbook", "body_md": "pasos" }),
        )
        .await;
    let id = n["id"].as_str().unwrap();
    assert_eq!(
        a.post(&format!("/archive/note/{id}"), json!({})).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(
        a.get("/search?q=despliegue")
            .await
            .1
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        a.get("/search?q=despliegue&archived=true")
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (_, arch) = a.get("/archive").await;
    assert_eq!(arch[0]["entity_id"], id);
    let (_, listed) = a.get("/notes?kind=runbook").await;
    assert_eq!(listed["total"], 0);
    let (_, listed) = a.get("/notes?kind=runbook&archived=true").await;
    assert_eq!(listed["total"], 1);
    assert_eq!(
        a.post(&format!("/archive/note/{id}/restore"), json!({})).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(a.get("/search?q=despliegue").await.1.as_array().unwrap().len(), 1);
    // Tipo no archivable / de otro usuario.
    assert_eq!(
        a.post(&format!("/archive/transaction/{id}"), json!({})).await.0,
        StatusCode::NOT_FOUND
    );
    let b = t.login(222).await;
    assert_eq!(
        b.post(&format!("/archive/note/{id}"), json!({})).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn links_backlinks_and_favorites_respect_ownership(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let b = t.login(222).await;
    let (_, n1) = a.post("/notes", json!({ "title": "Idea de negocio" })).await;
    let (_, p) = a.post("/people", json!({ "name": "Carla" })).await;
    let (_, nb) = b.post("/notes", json!({ "title": "Nota de B" })).await;
    let (n1, p, nb) = (
        n1["id"].as_str().unwrap(),
        p["id"].as_str().unwrap(),
        nb["id"].as_str().unwrap(),
    );

    let (st, _) = a
        .post(
            "/links",
            json!({ "src_type": "note", "src_id": n1, "dst_type": "person", "dst_id": p }),
        )
        .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    // Enlazar a una entidad de otro usuario o inexistente: 404.
    let (st, _) = a
        .post(
            "/links",
            json!({ "src_type": "note", "src_id": n1, "dst_type": "note", "dst_id": nb }),
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = a
        .post(
            "/links",
            json!({ "src_type": "note", "src_id": n1, "dst_type": "planeta", "dst_id": p }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, _) = a.post("/links", json!({ "src_type": "note", "src_id": n1, "dst_type": "person", "dst_id": p, "relation": "contributes_to" })).await;
    assert_eq!(
        st,
        StatusCode::UNPROCESSABLE_ENTITY,
        "contributes_to solo a objetivos"
    );

    // Backlinks desde la persona.
    let (_, links) = a.get(&format!("/links?type=person&id={p}")).await;
    assert_eq!(links["backlinks"][0]["src_id"], n1);
    assert_eq!(links["backlinks"][0]["other_title"], "Idea de negocio");
    let (_, det) = a.get(&format!("/notes/{n1}")).await;
    assert_eq!(det["links"][0]["dst_id"], p);
    // B no puede ver enlaces de A.
    assert_eq!(
        b.get(&format!("/links?type=person&id={p}")).await.0,
        StatusCode::NOT_FOUND
    );
    // Quitar enlace.
    let (st, _) = a
        .req(
            Method::DELETE,
            "/links",
            Some(json!({ "src_type": "note", "src_id": n1, "dst_type": "person", "dst_id": p })),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert!(
        a.get(&format!("/links?type=person&id={p}")).await.1["backlinks"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Favoritos.
    assert_eq!(
        a.put(&format!("/favorites/note/{n1}"), json!({})).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        a.put(&format!("/favorites/note/{n1}"), json!({})).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        a.put(&format!("/favorites/note/{nb}"), json!({})).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, favs) = a.get("/favorites").await;
    assert_eq!(favs.as_array().unwrap().len(), 1);
    assert_eq!(favs[0]["title"], "Idea de negocio");
    assert_eq!(a.get(&format!("/notes/{n1}")).await.1["favorite"], true);
    a.delete(&format!("/favorites/note/{n1}")).await;
    assert!(a.get("/favorites").await.1.as_array().unwrap().is_empty());
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn notes_crud_versions_and_snippets_as_text(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let code = "<script>alert(1)</script>\nrm -rf /";
    let (st, n) = a.post("/notes", json!({ "kind": "snippet", "title": "Borrar todo", "language": "bash", "body_md": code, "tags": ["lang/bash"] })).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(
        n["body_md"], code,
        "se guarda como texto, sin ejecutar ni alterar"
    );
    let id = n["id"].as_str().unwrap();
    let (st, _) = a.post("/notes", json!({ "kind": "poema", "title": "x" })).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, _) = a
        .post("/notes", json!({ "title": "x", "tags": ["con espacio"] }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, up) = a.put(&format!("/notes/{id}"), json!({ "kind": "snippet", "title": "Borrar", "body_md": "ls", "aliases": ["limpiar"], "tags": ["lang/bash"], "version": 1 })).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(up["version"], 2);
    let (st, c) = a
        .put(
            &format!("/notes/{id}"),
            json!({ "kind": "snippet", "title": "Viejo", "version": 1 }),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(c["error"]["current"]["title"], "Borrar");
    let (_, h) = a.get("/search?q=limpiar").await;
    assert_eq!(h[0]["entity_id"], id, "los alias se indexan");
    let (_, list) = a.get("/notes?tag=lang/bash").await;
    assert_eq!(list["total"], 1);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn drawings_persist_scene_link_note_and_limit_size(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let (_, note) = a.post("/notes", json!({ "title": "Mapa mental" })).await;
    let scene = json!({ "elements": [{ "type": "rectangle", "x": 1, "y": 2, "id": "r1" }], "appState": { "viewBackgroundColor": "#fff" } });
    let (st, d) = a
        .post(
            "/drawings",
            json!({ "title": "Arquitectura", "scene": scene, "note_id": note["id"] }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{d}");
    let id = d["id"].as_str().unwrap();
    let (_, got) = a.get(&format!("/drawings/{id}")).await;
    assert_eq!(got["scene"]["elements"][0]["id"], "r1");
    let (_, list) = a
        .get(&format!("/drawings?note_id={}", note["id"].as_str().unwrap()))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(
        list[0].get("scene").is_none(),
        "la lista no transfiere escenas pesadas"
    );
    let (st, up) = a.put(&format!("/drawings/{id}"), json!({ "title": "Arquitectura v2", "scene": { "elements": [] }, "note_id": note["id"], "version": 1 })).await;
    assert_eq!(st, StatusCode::OK, "{up}");
    let big = "x".repeat(1_950_000);
    let (st, _) = a
        .post(
            "/drawings",
            json!({ "title": "Enorme", "scene": { "elements": [], "files": { "f": big } } }),
        )
        .await;
    assert!(
        st == StatusCode::UNPROCESSABLE_ENTITY || st == StatusCode::PAYLOAD_TOO_LARGE,
        "{st}"
    );
    let (st, _) = a
        .post("/drawings", json!({ "title": "Mal", "scene": [1, 2] }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let b = t.login(222).await;
    assert_eq!(b.get(&format!("/drawings/{id}")).await.0, StatusCode::NOT_FOUND);
    // B no puede enlazar su dibujo a la nota de A.
    let (st, _) = b
        .post("/drawings", json!({ "title": "x", "note_id": note["id"] }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn manual_agenda_uses_local_time(pool: PgPool) {
    let t = common::TestApp::new(pool.clone());
    let a = t.login(111).await;
    let (st, e) = a.post("/agenda", json!({ "on_date": "2026-11-12", "title": "Dentista", "start_time": "10:00", "end_time": "11:00" })).await;
    assert_eq!(st, StatusCode::CREATED, "{e}");
    // 10:00 en Monterrey (UTC−6) = 16:00 UTC.
    assert_eq!(e["starts_at"], "2026-11-12T16:00:00Z");
    a.post(
        "/agenda",
        json!({ "on_date": "2026-11-12", "title": "Cumpleaños", "all_day": true }),
    )
    .await;
    let (_, day) = a.get("/agenda/day/2026-11-12").await;
    assert_eq!(day.as_array().unwrap().len(), 2);
    assert_eq!(day[0]["title"], "Cumpleaños", "todo el día primero");
    let (st, _) = a
        .post("/agenda", json!({ "on_date": "2026-11-12", "title": "Sin hora" }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    // Un evento de Google no se edita ni borra desde la agenda manual.
    let uid: uuid::Uuid = a.user_id.parse().unwrap();
    let gid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO calendar_events (user_id, source, calendar_id, provider_event_id, on_date, title) VALUES ($1, 'google', 'primary', 'g1', '2026-11-12', 'Junta') RETURNING id",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(a.delete(&format!("/agenda/{gid}")).await.0, StatusCode::NOT_FOUND);
    let id = e["id"].as_str().unwrap();
    let b = t.login(222).await;
    assert_eq!(b.delete(&format!("/agenda/{id}")).await.0, StatusCode::NOT_FOUND);
    assert_eq!(a.delete(&format!("/agenda/{id}")).await.0, StatusCode::NO_CONTENT);
}
