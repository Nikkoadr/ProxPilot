//! Login session auth (cookie `panel_session`, argon2 password hash).
//!
//! Default: admin / admin123 (seeded on first boot, overridable via
//! ADMIN_USER / ADMIN_PASS env). Changeable in Settings page.

use crate::db::Db;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::{
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json},
};
use rand_core::OsRng;
use serde::Deserialize;
use serde_json::json;

pub const SESSION_COOKIE: &str = "panel_session";
const SESSION_DAYS: i64 = 7;

pub fn default_user() -> String {
    std::env::var("ADMIN_USER").unwrap_or_else(|_| "admin".to_string())
}
pub fn default_pass() -> String {
    std::env::var("ADMIN_PASS").unwrap_or_else(|_| "admin123".to_string())
}

pub fn hash_password(pass: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pass.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn verify(pass: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(pass.as_bytes(), &parsed)
        .is_ok()
}

/// Seed default user on first boot.
pub async fn ensure_seed(db: Db) {
    let db2 = db.clone();
    let _ = tokio::task::spawn_blocking(move || {
        if db2.user_count() == 0 {
            let (u, p) = (default_user(), default_pass());
            if let Ok(h) = hash_password(&p) {
                db2.upsert_user(&u, &h);
                db2.set_setting("default_creds", "1");
                tracing::warn!("seeded default login {u} (change it in Settings!)");
            }
        }
    })
    .await;
}

pub fn session_user(db: &Db, headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(tok) = part.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            let tok = tok.trim();
            if tok.is_empty() {
                return None;
            }
            if let Some(u) = db.check_session(tok) {
                return Some(u);
            }
        }
    }
    None
}

pub async fn require_auth(
    State(state): State<crate::store::AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> impl IntoResponse {
    if session_user(&state.db, req.headers()).is_none() {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "login required"}))).into_response();
    }
    next.run(req).await
}

// ---------- handlers (mounted on public router) ----------

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
}

pub async fn login(
    State(state): State<crate::store::AppState>,
    Json(b): Json<LoginBody>,
) -> axum::response::Response {
    let db = state.db.clone();
    let (u, p) = (b.username.trim().to_string(), b.password.clone());
    let ok = tokio::task::spawn_blocking(move || {
        db.get_pass_hash(&u).map(|h| verify(&p, &h)).unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    if !ok {
        // generic message to avoid user enumeration
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid username or password"}))).into_response();
    }
    let token = uuid::Uuid::new_v4().to_string().replace('-', "");
    let exp = chrono::Utc::now().timestamp() + SESSION_DAYS * 86400;
    let db = state.db.clone();
    let u2 = b.username.trim().to_string();
    let t2 = token.clone();
    tokio::task::spawn_blocking(move || db.create_session(&t2, &u2, exp))
        .await
        .ok();
    let cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        SESSION_DAYS * 86400
    );
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(json!({"ok": true})),
    )
        .into_response()
}

pub async fn logout(State(state): State<crate::store::AppState>, req: Request<axum::body::Body>) -> impl IntoResponse {
    if let Some(tok) = current_token(req.headers()) {
        let db = state.db.clone();
        tokio::task::spawn_blocking(move || db.delete_session(&tok))
            .await
            .ok();
    }
    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            format!("{SESSION_COOKIE}=; Path=/; HttpOnly; Max-Age=0"),
        )],
        Json(json!({"ok": true})),
    )
        .into_response()
}

fn current_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        if let Some(tok) = part.trim().strip_prefix(&format!("{SESSION_COOKIE}=")) {
            return Some(tok.to_string());
        }
    }
    None
}

/// GET /api/me — whoami + whether default creds still in use.
pub async fn me(State(state): State<crate::store::AppState>, req: Request<axum::body::Body>) -> impl IntoResponse {
    let db = state.db.clone();
    match session_user(&db, req.headers()) {
        Some(u) => {
            let def = db.get_setting("default_creds").as_deref() == Some("1");
            (StatusCode::OK, Json(json!({"username": u, "default_creds": def}))).into_response()
        }
        None => (StatusCode::UNAUTHORIZED, Json(json!({"error": "login required"}))).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ChangePassBody {
    pub old_password: String,
    pub new_password: String,
}

/// POST /api/user/password — change own password (min 6 chars).
pub async fn change_password(
    State(state): State<crate::store::AppState>,
    headers: axum::http::HeaderMap,
    Json(b): Json<ChangePassBody>,
) -> impl IntoResponse {
    let db = state.db.clone();
    let username = match session_user(&db, &headers) {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "login required"}))).into_response(),
    };
    if b.new_password.len() < 6 {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "new password min 6 chars"}))).into_response();
    }
    let ok = db.get_pass_hash(&username).map(|h| verify(&b.old_password, &h)).unwrap_or(false);
    if !ok {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "old password wrong"}))).into_response();
    }
    let hash = match hash_password(&b.new_password) {
        Ok(h) => h,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
    };
    let db2 = state.db.clone();
    let u2 = username.clone();
    tokio::task::spawn_blocking(move || {
        db2.upsert_user(&u2, &hash);
        db2.set_setting("default_creds", "0");
        db2.delete_user_sessions(&u2); // force re-login everywhere
    })
    .await
    .ok();
    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            format!("{SESSION_COOKIE}=; Path=/; HttpOnly; Max-Age=0"),
        )],
        Json(json!({"ok": true, "message": "password changed, please login again"})),
    )
        .into_response()
}
