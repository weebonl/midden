use std::{collections::BTreeMap, net::IpAddr, path::PathBuf, time::Instant};

use axum::{
    Router,
    body::Bytes,
    extract::{Multipart, Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{delete, get, options, patch, post},
};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::BodyExt;
use prometheus_client::encoding::text::encode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

use crate::{
    app::{AppError, AppResult, AppState},
    config::{
        ActionRule, ContentDispositionMode, DeletePolicy, FeatureConfig, HomepageBlock,
        PolicyConfig, RateLimitBackend, RateLimitConfig, RuntimeSettings, ScanDecision,
        ScannerAdapterConfig, SignupMode,
    },
    db::{FileItem, NewFileItem, NewPaste, NewUploadSession, Paste, Role, User},
    policy, processing, quota,
    scanner::{self, ScanInput},
    util,
};

mod account;
mod admin;
mod api;
mod auth;
mod browse;
mod files;
mod items;
mod oidc;
mod pastes;
mod support;
mod system;
mod tus;
mod upload;

use account::*;
use admin::*;
use api::*;
use auth::*;
use browse::*;
use files::*;
use items::*;
use pastes::*;
use support::*;
use system::*;
use tus::*;
use upload::*;

#[cfg(test)]
mod tests;

const CSRF_COOKIE: &str = "midden_csrf";
const CSRF_FIELD: &str = "csrf_token";
const TWO_FACTOR_CHALLENGE_COOKIE: &str = "midden_2fa_challenge";

#[derive(Clone)]
pub struct RequestContext {
    pub templates: crate::templates::Templates,
    pub settings: crate::config::RuntimeSettings,
    pub current_user: Option<crate::db::User>,
    pub is_htmx: bool,
}

tokio::task_local! {
    pub static REQUEST_CONTEXT: RequestContext;
}

async fn request_context_middleware(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let settings = match state.settings().await {
        Ok(s) => s,
        Err(err) => {
            tracing::error!(error = %err, "failed to load settings in middleware");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("internal server error".to_string())
            ).into_response();
        }
    };
    let current_user = current_user(&state, &jar).await.unwrap_or(None);
    let is_htmx = htmx_request(&headers);

    let ctx = RequestContext {
        templates: state.templates.clone(),
        settings,
        current_user,
        is_htmx,
    };

    REQUEST_CONTEXT.scope(ctx, async {
        next.run(request).await
    }).await
}

pub fn router(state: AppState) -> Router {
    let metrics_state = state.clone();
    let csrf_state = state.clone();
    Router::new()
        .route("/", get(index).post(upload_form_file))
        .route("/url-upload", get(url_upload_form).post(url_upload))
        .route("/static/{*path}", get(static_asset))
        .route("/browse", get(public_browse))
        .route("/robots.txt", get(robots_txt))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/api/docs", get(api_docs))
        .route("/api/openapi.json", get(api_openapi))
        .route("/api/v1/me/files", get(api_list_my_files))
        .route("/api/v1/me/pastes", get(api_list_my_pastes))
        .route("/api/v1/files", post(api_upload_file))
        .route("/api/v1/files/{id}", delete(api_delete_file))
        .route("/api/v1/pastes", post(api_create_paste))
        .route("/api/v1/pastes/{id}", delete(api_delete_paste))
        .route("/api/v1/reports", post(api_create_report))
        .route("/api/v1/claim/{kind}/{id}", post(api_claim_item))
        .route(
            "/api/v1/tokens",
            get(api_list_tokens).post(api_create_token),
        )
        .route("/api/v1/tokens/{id}", delete(api_revoke_token))
        .route("/api/v1/admin/reports", get(api_admin_reports))
        .route("/api/v1/admin/reports/{id}", patch(api_admin_update_report))
        .route(
            "/api/v1/admin/items/{kind}/{id}",
            patch(api_admin_update_item),
        )
        .route("/api/v1/admin/search", get(api_admin_search))
        .route("/p/new", get(new_paste).post(create_paste))
        .route("/p/{id}/edit", get(edit_paste_form).post(update_paste))
        .route("/p/{id}", get(show_paste))
        .route("/p/{id}/raw", get(raw_paste))
        .route("/files/{id}/raw", get(raw_file))
        .route("/internal/files/{id}/raw", get(internal_raw_file))
        .route("/report/{kind}/{id}", get(report_form).post(create_report))
        .route("/delete/{kind}/{id}", get(delete_form).post(delete_item))
        .route("/claim/{kind}/{id}", get(claim_form).post(claim_item))
        .route("/auth/login", get(login_form).post(login))
        .route("/auth/logout", post(logout))
        .route(
            "/auth/password-reset",
            get(password_reset_request_form).post(password_reset_request),
        )
        .route(
            "/auth/password-reset/{token}",
            get(password_reset_form).post(password_reset_submit),
        )
        .route("/auth/verify-email/{token}", get(verify_email))
        .route("/auth/2fa", get(two_factor_form).post(two_factor_submit))
        .route("/auth/oidc/login", get(oidc::login))
        .route("/auth/oidc/callback", get(oidc::callback))
        .route("/register", get(register_form).post(register))
        .route("/account", get(account))
        .route(
            "/account/email-verification",
            post(account_send_email_verification),
        )
        .route("/account/oidc/link", get(oidc::account_link))
        .route("/account/password", post(account_change_password))
        .route(
            "/account/two-factor/enable",
            post(account_enable_two_factor),
        )
        .route(
            "/account/two-factor/disable",
            post(account_disable_two_factor),
        )
        .route("/account/deactivate", post(account_deactivate))
        .route("/account/tokens", post(account_create_token))
        .route("/account/tokens/{id}/revoke", post(account_revoke_token))
        .route("/account/items/bulk", post(account_bulk_items))
        .route("/admin", get(admin))
        .route("/admin/jobs", get(admin_jobs).post(admin_jobs_run_once))
        .route("/admin/search", get(admin_search))
        .route("/admin/users", get(admin_users).post(admin_create_user))
        .route("/admin/users/invites", post(admin_create_invite))
        .route(
            "/admin/users/invites/{id}/revoke",
            post(admin_revoke_invite),
        )
        .route("/admin/users/{id}/role", post(admin_set_user_role))
        .route("/admin/users/{id}/disable", post(admin_disable_user))
        .route("/admin/users/{id}/enable", post(admin_enable_user))
        .route("/admin/settings", post(admin_update_settings))
        .route("/admin/reports", get(admin_reports))
        .route("/admin/reports/bulk", post(admin_bulk_update_reports))
        .route("/admin/reports/{id}", post(admin_update_report))
        .route(
            "/admin/items/{kind}/{id}",
            get(admin_item).post(admin_update_item),
        )
        .route("/tus", options(tus_options).post(tus_create))
        .route(
            "/tus/{id}",
            options(tus_options).head(tus_head).patch(tus_patch),
        )
        .route("/{slug}", get(file_slug))
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn(api_error_middleware))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            request_context_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            metrics_state,
            request_metrics_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            csrf_state,
            csrf_cookie_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn csrf_cookie_middleware(
    State(state): State<AppState>,
    jar: CookieJar,
    request: Request,
    next: Next,
) -> Response {
    let needs_cookie = jar.get(CSRF_COOKIE).is_none();
    let mut response = next.run(request).await;
    if needs_cookie {
        let mut cookie = Cookie::new(CSRF_COOKIE, util::secret_token());
        cookie.set_path("/");
        cookie.set_same_site(SameSite::Lax);
        cookie.set_secure(state.config.security.secure_cookies);
        if let Ok(value) = HeaderValue::from_str(&cookie.to_string()) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

async fn request_metrics_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let response = next.run(request).await;
    state
        .metrics
        .request_latency
        .observe(started.elapsed().as_secs_f64());
    response
}

async fn api_error_middleware(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let wants_json = path.starts_with("/api/");
    let response = next.run(request).await;
    if wants_json && (response.status().is_client_error() || response.status().is_server_error()) {
        let status = response.status();
        let code = status.canonical_reason().unwrap_or("error");
        return (
            status,
            axum::Json(serde_json::json!({
                "error": {
                    "status": status.as_u16(),
                    "code": code.to_ascii_lowercase().replace(' ', "_"),
                    "message": code,
                }
            })),
        )
            .into_response();
    }
    response
}
