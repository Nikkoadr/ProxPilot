mod auth;
mod db;
mod exec;
mod models;
mod proxmox;
mod routes;
mod store;

use axum::{
    http::{header, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    let base = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let infra_dir = base.join("infra").to_string_lossy().to_string();
    std::fs::create_dir_all(format!("{infra_dir}/terraform")).ok();

    let database = db::Db::open().expect("open sqlite db");
    auth::ensure_seed(database.clone()).await;
    let state = store::AppState::new(database, infra_dir);

    // Public: login + liveness (for service checks).
    let public = Router::new()
        .route("/api/login", post(auth::login))
        .route("/api/health", get(health_open))
        .with_state(state.clone());

    // Protected API (needs panel_session cookie).
    let protected_api = routes::router(state.clone()).layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_auth,
    ));
    // Auth account endpoints (logout needs cookie read but must work to clear it;
    // me/change_password enforce login themselves).
    let account = Router::new()
        .route("/api/logout", post(auth::logout))
        .route("/api/me", get(auth::me))
        .route("/api/user/password", post(auth::change_password))
        .with_state(state.clone());

    let state2 = state.clone();
    // All static content goes through serve_spa so HTML pages get the
    // login guard (ServeDir alone would bypass auth for existing files).
    let static_svc = tower::service_fn(
        move |req: axum::http::Request<axum::body::Body>| {
            let st = state2.clone();
            async move { serve_spa(req, st).await }
        },
    );

    let app = public
        .merge(account)
        .merge(protected_api)
        .fallback_service(static_svc)
        .layer(tower_http::cors::CorsLayer::permissive());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("proxmox-panel (Rust + SB Admin 2) on http://localhost:{port}");
    tracing::info!("login default: admin / admin123 (change in Settings)");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

async fn health_open() -> impl IntoResponse {
    let wsl = exec::wsl_available();
    axum::Json(serde_json::json!({
        "ok": true,
        "server_time": chrono::Utc::now(),
        "wsl": { "available": wsl },
        "mode": if wsl { "wsl" } else { "native" },
    }))
}

/// Serve static files; HTML pages (except login) require login,
/// otherwise serve login.html. Assets (.js/.css/...) stay public
/// so the login page renders.
async fn serve_spa(
    req: axum::http::Request<axum::body::Body>,
    state: store::AppState,
) -> Result<Response, std::convert::Infallible> {
    let path = req.uri().path().to_string();
    let is_page = path == "/" || path.ends_with(".html") || !path.contains('.');
    if is_page && !path.starts_with("/login") {
        let logged_in = req
            .headers()
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|c| {
                c.split(';').any(|p| {
                    p.trim().starts_with(&format!("{}=", auth::SESSION_COOKIE))
                        && state.db.check_session(
                            p.trim()[auth::SESSION_COOKIE.len() + 1..].trim(),
                        )
                        .is_some()
                })
            })
            .unwrap_or(false);
        if !logged_in {
            let body = tokio::fs::read("static/login.html").await.unwrap_or_else(|_| b"<h1>login</h1>".to_vec());
            return Ok(Response::builder()
                .header("content-type", "text/html")
                .body(axum::body::Body::from(body))
                .unwrap());
        }
    }
    // fall through to ServeDir for the actual file
    let svc_path = if path == "/" { "/index.html".to_string() } else { path };
    let body = tokio::fs::read(format!("static{svc_path}")).await;
    match body {
        Ok(b) => {
            let ct = if svc_path.ends_with(".html") {
                "text/html"
            } else if svc_path.ends_with(".js") {
                "text/javascript"
            } else if svc_path.ends_with(".css") {
                "text/css"
            } else {
                "application/octet-stream"
            };
            Ok(Response::builder()
                .header("content-type", ct)
                .body(axum::body::Body::from(b))
                .unwrap())
        }
        Err(_) => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("not found"))
            .unwrap()),
    }
}
