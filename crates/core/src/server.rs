use std::cmp::Reverse;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use rust_embed::RustEmbed;

use crate::common::{analyze, model::Session};

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct WebAssets;

/// `lang` is decided by the caller; the core does not translate.
struct App {
    sessions: Vec<Session>,
    lang: String,
}

pub fn router(sessions: Vec<Session>, lang: String) -> Router {
    Router::new()
        .route("/api/sessions", get(list))
        .route("/api/sessions/{id}", get(report))
        .route("/api/tldr", get(tldr))
        .route("/api/config", get(config))
        .fallback(static_asset)
        .with_state(Arc::new(App { sessions, lang }))
}

type Sessions = State<Arc<App>>;

async fn config(State(app): Sessions) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "lang": app.lang }))
}

async fn list(State(app): Sessions) -> Json<Vec<analyze::Report>> {
    let mut reports: Vec<_> = app.sessions.iter().map(analyze::session).collect();
    reports.sort_by_key(|r| Reverse(r.totals.cache_read));
    for report in &mut reports {
        report.calls.clear();
    }
    Json(reports)
}

async fn tldr(State(app): Sessions) -> Json<analyze::Tldr> {
    Json(analyze::tldr(&app.sessions))
}

async fn report(State(app): Sessions, Path(id): Path<String>) -> Response {
    match app.sessions.iter().find(|s| s.id == id) {
        Some(s) => Json(analyze::session(s)).into_response(),
        None => (StatusCode::NOT_FOUND, "no such session").into_response(),
    }
}

async fn static_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    // Serving index.html for a missing API path makes the frontend parse HTML as JSON.
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "no such endpoint").into_response();
    }
    let path = if path.is_empty() { "index.html" } else { path };

    let Some((name, file)) = WebAssets::get(path)
        .map(|f| (path, f))
        .or_else(|| WebAssets::get("index.html").map(|f| ("index.html", f)))
    else {
        return (
            StatusCode::NOT_FOUND,
            "web UI is not built. Run `npm run build` in web/.",
        )
            .into_response();
    };

    ([(header::CONTENT_TYPE, content_type(name))], file.data).into_response()
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

pub async fn serve(
    sessions: Vec<Session>,
    lang: String,
    port: u16,
    on_bind: impl FnOnce(u16),
) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    on_bind(listener.local_addr()?.port());
    axum::serve(listener, router(sessions, lang)).await
}

#[cfg(test)]
mod tests;
