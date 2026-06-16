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
        PolicyConfig, RateLimitConfig, RuntimeSettings, ScanDecision, ScannerAdapterConfig,
        SignupMode,
    },
    db::{FileItem, NewFileItem, NewPaste, NewUploadSession, Paste, Role, User},
    policy, processing, quota,
    scanner::{self, ScanInput},
    util,
};

mod admin;
mod api;
mod oidc;
mod support;
mod tus;
mod upload;

use admin::*;
use api::*;
use support::*;
use tus::*;
use upload::*;

const CSRF_COOKIE: &str = "midden_csrf";
const CSRF_FIELD: &str = "csrf_token";
const TWO_FACTOR_CHALLENGE_COOKIE: &str = "midden_2fa_challenge";

pub fn router(state: AppState) -> Router {
    let metrics_state = state.clone();
    let csrf_state = state.clone();
    Router::new()
        .route("/", get(index).post(upload_form_file))
        .route("/upload/resumable", get(resumable_upload_form))
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
        .route("/admin", get(admin))
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

async fn index(State(state): State<AppState>, jar: CookieJar) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    let page = serde_json::json!({
        "max_upload": util::human_bytes(settings.limits.max_upload_bytes),
        "delete_policy": format!("{:?}", settings.policy.delete_policy),
    });
    render(&state, "index.html", &settings, user.as_ref(), page)
}

async fn resumable_upload_form(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    if !settings.features.files || !policy::can_upload_file(&settings, user.as_ref()) {
        return Err(AppError::Forbidden);
    }
    render(
        &state,
        "resumable_upload.html",
        &settings,
        user.as_ref(),
        serde_json::json!({
            "max_upload": util::human_bytes(settings.limits.max_tus_upload_bytes),
        }),
    )
}

async fn upload_form_file(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    multipart: Multipart,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    enforce_rate_limit(&state, &settings, "upload_file", &headers, user.as_ref()).await?;
    if !policy::can_upload_file(&settings, user.as_ref()) {
        return Err(AppError::Forbidden);
    }
    let form = read_upload_form(multipart, settings.limits.max_upload_bytes).await?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    let result = persist_file_upload(
        &state,
        &settings,
        user.as_ref(),
        form.file,
        parse_expiry_or_default(
            form.expires.as_deref(),
            settings.limits.default_file_expiry.as_deref(),
        )
        .map_err(|err| AppError::BadRequest(format!("invalid expiry: {err}")))?,
        requested_visibility(&settings, form.visibility.as_deref())?,
    )
    .await?;
    let page = serde_json::json!({
        "url": result.url,
        "raw_url": result.raw_url,
        "delete_token": result.delete_token,
        "file": result.file,
    });
    Ok(render(&state, "upload_result.html", &settings, user.as_ref(), page)?.into_response())
}

async fn url_upload_form(State(state): State<AppState>, jar: CookieJar) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    if !settings.features.upload_by_url {
        return Err(AppError::NotFound);
    }
    let user = current_user(&state, &jar).await?;
    render(
        &state,
        "url_upload.html",
        &settings,
        user.as_ref(),
        serde_json::json!({}),
    )
}

#[derive(Debug, Deserialize)]
struct UrlUploadForm {
    url: String,
    expires: Option<String>,
    visibility: Option<String>,
    csrf_token: Option<String>,
}

async fn url_upload(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    axum::Form(form): axum::Form<UrlUploadForm>,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    if !settings.features.upload_by_url {
        return Err(AppError::NotFound);
    }
    let user = current_user(&state, &jar).await?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    enforce_rate_limit(&state, &settings, "upload_by_url", &headers, user.as_ref()).await?;
    if !policy::can_upload_file(&settings, user.as_ref()) {
        return Err(AppError::Forbidden);
    }
    let url = url::Url::parse(&form.url)
        .map_err(|err| AppError::BadRequest(format!("invalid URL: {err}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::BadRequest(
            "only http and https URLs are supported".to_string(),
        ));
    }
    let fetched = fetch_url_upload(&settings, url.clone()).await?;
    let filename = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned);
    let result = persist_file_upload(
        &state,
        &settings,
        user.as_ref(),
        UploadedBytes {
            bytes: fetched.bytes,
            filename,
            content_type: fetched.content_type,
        },
        parse_expiry_or_default(
            form.expires.as_deref(),
            settings.limits.default_file_expiry.as_deref(),
        )
        .map_err(|err| AppError::BadRequest(format!("invalid expiry: {err}")))?,
        requested_visibility(&settings, form.visibility.as_deref())?,
    )
    .await?;
    let page = serde_json::json!({
        "url": result.url,
        "raw_url": result.raw_url,
        "delete_token": result.delete_token,
        "file": result.file,
    });
    Ok(render(&state, "upload_result.html", &settings, user.as_ref(), page)?.into_response())
}

async fn new_paste(State(state): State<AppState>, jar: CookieJar) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    if !policy::can_create_paste(&settings, user.as_ref()) {
        return Err(AppError::Forbidden);
    }
    render(
        &state,
        "paste_new.html",
        &settings,
        user.as_ref(),
        serde_json::json!({}),
    )
}

#[derive(Debug, Deserialize)]
struct PasteForm {
    title: Option<String>,
    syntax: Option<String>,
    expires: Option<String>,
    visibility: Option<String>,
    content: String,
    csrf_token: Option<String>,
}

async fn create_paste(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    axum::Form(form): axum::Form<PasteForm>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    enforce_rate_limit(&state, &settings, "create_paste", &headers, user.as_ref()).await?;
    if !policy::can_create_paste(&settings, user.as_ref()) {
        return Err(AppError::Forbidden);
    }
    if form.content.len() as i64 > settings.limits.max_paste_bytes {
        return Err(AppError::PayloadTooLarge);
    }

    let public_id = util::public_id();
    let delete_token = anonymous_delete_token(&settings, user.as_ref());
    let delete_hash = delete_token.as_deref().map(util::hash_token);
    let syntax = normalize_syntax(form.syntax.as_deref());
    let paste = state
        .db
        .create_paste(NewPaste {
            id: &uuid::Uuid::new_v4().to_string(),
            public_id: &public_id,
            title: form
                .title
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
            content: &form.content,
            syntax: syntax.as_deref(),
            owner_user_id: user.as_ref().map(|u| u.id.as_str()),
            delete_token_hash: delete_hash.as_deref(),
            expires_at: parse_expiry_or_default(
                form.expires.as_deref(),
                settings.limits.default_paste_expiry.as_deref(),
            )
            .map_err(|err| AppError::BadRequest(format!("invalid expiry: {err}")))?,
            visibility: requested_visibility(&settings, form.visibility.as_deref())?,
        })
        .await?;
    state.metrics.pastes.inc();
    let base = state.config.server.public_base_url.trim_end_matches('/');
    render(
        &state,
        "paste_result.html",
        &settings,
        user.as_ref(),
        serde_json::json!({
            "paste": paste,
            "url": format!("{base}/p/{public_id}"),
            "raw_url": format!("{base}/p/{public_id}/raw"),
            "delete_token": delete_token,
        }),
    )
}

async fn show_paste(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    let paste = match state.db.paste_by_public_id(&id).await {
        Ok(paste) => paste,
        Err(_) => {
            let existing = state
                .db
                .paste_by_public_id_any(&id)
                .await
                .map_err(|_| AppError::NotFound)?;
            return render_unavailable_item(
                &state,
                &settings,
                user.as_ref(),
                "paste",
                &id,
                &existing.state,
            );
        }
    };
    let rendered = render_paste_content(&paste.content, paste.syntax.as_deref());
    let can_edit = can_edit_paste(&settings, user.as_ref(), &paste);
    let revision_count = state.db.paste_revision_count(&paste.id).await.unwrap_or(0);
    let base = state.config.server.public_base_url.trim_end_matches('/');
    let page = serde_json::json!({
        "paste": paste,
        "rendered": rendered,
        "can_edit": can_edit,
        "revision_count": revision_count,
        "absolute_url": format!("{base}/p/{id}"),
        "absolute_raw_url": format!("{base}/p/{id}/raw"),
    });
    render(&state, "paste_show.html", &settings, user.as_ref(), page)
}

async fn edit_paste_form(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let paste = state
        .db
        .paste_by_public_id(&id)
        .await
        .map_err(|_| AppError::NotFound)?;
    if !can_edit_paste(&settings, Some(&user), &paste) {
        return Err(AppError::Forbidden);
    }
    render(
        &state,
        "paste_edit.html",
        &settings,
        Some(&user),
        serde_json::json!({ "paste": paste }),
    )
}

#[derive(Debug, Deserialize)]
struct PasteEditForm {
    title: Option<String>,
    syntax: Option<String>,
    content: String,
    csrf_token: Option<String>,
}

async fn update_paste(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    axum::Form(form): axum::Form<PasteEditForm>,
) -> AppResult<Redirect> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    if form.content.len() as i64 > settings.limits.max_paste_bytes {
        return Err(AppError::PayloadTooLarge);
    }
    let paste = state
        .db
        .paste_by_public_id(&id)
        .await
        .map_err(|_| AppError::NotFound)?;
    if !can_edit_paste(&settings, Some(&user), &paste) {
        return Err(AppError::Forbidden);
    }
    let syntax = normalize_syntax(form.syntax.as_deref());
    state
        .db
        .update_paste(
            &paste.id,
            form.title
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            &form.content,
            syntax.as_deref(),
            Some(&user.id),
        )
        .await?;
    Ok(Redirect::to(&format!("/p/{id}")))
}

async fn raw_paste(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Response> {
    let paste = state
        .db
        .paste_by_public_id(&id)
        .await
        .map_err(|_| AppError::NotFound)?;
    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )],
        paste.content,
    )
        .into_response())
}

async fn file_slug(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    let Some((public_id, _extension)) = util::split_slug(&slug) else {
        return Err(AppError::NotFound);
    };
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    let file = match state.db.active_file_by_public_id(public_id).await? {
        Some(file) => file,
        None => {
            let existing = state
                .db
                .file_by_public_id(public_id)
                .await
                .map_err(|_| AppError::NotFound)?;
            return render_unavailable_item(
                &state,
                &settings,
                user.as_ref(),
                "file",
                public_id,
                &existing.state,
            )
            .map(IntoResponse::into_response);
        }
    };
    if settings.features.preview_pages {
        let preview = file_preview_context(&state, &file).await?;
        let base = state.config.server.public_base_url.trim_end_matches('/');
        let slug = util::slug_with_extension(&file.public_id, file.extension.as_deref());
        let page = serde_json::json!({
            "file": file,
            "raw_url": format!("/files/{}/raw", public_id),
            "absolute_url": format!("{base}/{slug}"),
            "absolute_raw_url": format!("{base}/files/{public_id}/raw"),
            "human_size": util::human_bytes(file.size_bytes),
            "preview": preview,
        });
        Ok(render(&state, "file_preview.html", &settings, user.as_ref(), page)?.into_response())
    } else {
        serve_file(&state, file).await
    }
}

async fn file_preview_context(state: &AppState, file: &FileItem) -> AppResult<serde_json::Value> {
    let content_type = file.content_type.as_deref().unwrap_or_default();
    let is_image = matches!(content_type, "image/png" | "image/gif" | "image/jpeg");
    let is_text = content_type.starts_with("text/")
        || matches!(
            content_type,
            "application/json" | "application/xml" | "application/javascript"
        );
    let text = if is_text && file.size_bytes <= 128 * 1024 {
        let bytes = state.storage.get_blob(&file.blob_hash).await?;
        Some(
            String::from_utf8_lossy(&bytes)
                .chars()
                .take(8000)
                .collect::<String>(),
        )
    } else {
        None
    };
    Ok(serde_json::json!({
        "is_image": is_image,
        "is_text": is_text,
        "text": text,
    }))
}

fn render_unavailable_item(
    state: &AppState,
    settings: &RuntimeSettings,
    user: Option<&User>,
    kind: &str,
    id: &str,
    item_state: &str,
) -> AppResult<Html<String>> {
    render(
        state,
        "takedown.html",
        settings,
        user,
        serde_json::json!({ "kind": kind, "id": id, "state": item_state }),
    )
}

async fn raw_file(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Response> {
    let file = state
        .db
        .active_file_by_public_id(&id)
        .await?
        .ok_or(AppError::NotFound)?;
    serve_file(&state, file).await
}

#[derive(Debug, Deserialize)]
struct InternalFileQuery {
    expires: i64,
    signature: String,
}

async fn internal_raw_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<InternalFileQuery>,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    if !settings.delivery.signed_internal_urls {
        return Err(AppError::NotFound);
    }
    let secret = settings
        .delivery
        .internal_url_secret
        .as_deref()
        .filter(|secret| !secret.is_empty())
        .ok_or(AppError::NotFound)?;
    if query.expires < util::now_ts() {
        return Err(AppError::Forbidden);
    }
    let expected = sign_internal_file_url(secret, &id, query.expires);
    if !constant_time_eq(expected.as_bytes(), query.signature.as_bytes()) {
        return Err(AppError::Forbidden);
    }
    let file = state
        .db
        .active_file_by_public_id(&id)
        .await?
        .ok_or(AppError::NotFound)?;
    serve_file(&state, file).await
}

async fn serve_file(state: &AppState, file: FileItem) -> AppResult<Response> {
    let settings = state.settings().await?;
    let bytes = state.storage.get_blob(&file.blob_hash).await?;
    let content_type = file
        .content_type
        .as_deref()
        .unwrap_or("application/octet-stream")
        .parse::<HeaderValue>()
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let mut response = bytes.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    if let Some(filename) = &file.original_filename {
        let disposition_kind = match settings.security.content_disposition {
            ContentDispositionMode::Inline => "inline",
            ContentDispositionMode::Attachment => "attachment",
        };
        let disposition = format!(
            "{disposition_kind}; filename=\"{}\"",
            filename.replace('"', "")
        );
        if let Ok(value) = HeaderValue::from_str(&disposition) {
            response
                .headers_mut()
                .insert(header::CONTENT_DISPOSITION, value);
        }
    }
    insert_cache_control(
        &mut response,
        settings.delivery.public_cache_seconds,
        CacheScope::Public,
    );
    state.metrics.served_files.inc();
    Ok(response)
}

async fn report_form(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((kind, id)): Path<(String, String)>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    if !settings.features.reports {
        return Err(AppError::NotFound);
    }
    let user = current_user(&state, &jar).await?;
    render(
        &state,
        "report_form.html",
        &settings,
        user.as_ref(),
        serde_json::json!({ "kind": kind, "id": id }),
    )
}

#[derive(Debug, Deserialize)]
struct ReportForm {
    reason: String,
    details: Option<String>,
    csrf_token: Option<String>,
}

async fn create_report(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((kind, id)): Path<(String, String)>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<ReportForm>,
) -> AppResult<Redirect> {
    let settings = state.settings().await?;
    if !settings.features.reports {
        return Err(AppError::NotFound);
    }
    let user = current_user(&state, &jar).await?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    enforce_rate_limit(&state, &settings, "report", &headers, user.as_ref()).await?;
    state
        .db
        .create_report(
            &kind,
            &id,
            user.as_ref().map(|user| user.id.as_str()),
            &form.reason,
            form.details.as_deref().unwrap_or(""),
        )
        .await?;
    state.metrics.reports.inc();
    if let Some(abuse_email) = &settings.branding.abuse_email {
        let _ = state
            .mailer
            .send(
                abuse_email,
                "New Midden report",
                &format!(
                    "A report was submitted for {kind} {id}.\n\nReason: {}\n\nDetails:\n{}",
                    form.reason,
                    form.details.as_deref().unwrap_or("")
                ),
            )
            .await?;
    }
    Ok(Redirect::to("/"))
}

async fn delete_form(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((kind, id)): Path<(String, String)>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    render(
        &state,
        "delete_form.html",
        &settings,
        user.as_ref(),
        serde_json::json!({ "kind": kind, "id": id }),
    )
}

#[derive(Debug, Deserialize)]
struct DeleteForm {
    token: Option<String>,
    csrf_token: Option<String>,
}

async fn delete_item(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((kind, id)): Path<(String, String)>,
    axum::Form(form): axum::Form<DeleteForm>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    match kind.as_str() {
        "file" => {
            let file = state
                .db
                .active_file_by_public_id(&id)
                .await?
                .ok_or(AppError::NotFound)?;
            authorize_file_delete(&settings, user.as_ref(), &file, form.token.as_deref())?;
            let deleted = state
                .db
                .delete_file(
                    &file.id,
                    user.as_ref().map(|user| user.id.as_str()),
                    "web delete",
                )
                .await?;
            let remaining_refs = state.db.decrement_blob_ref(&deleted.blob_hash).await?;
            if remaining_refs == 0 {
                state.storage.delete_blob(&deleted.blob_hash).await?;
            }
        }
        "paste" => {
            let paste = state
                .db
                .paste_by_public_id(&id)
                .await
                .map_err(|_| AppError::NotFound)?;
            authorize_paste_delete(&settings, user.as_ref(), &paste, form.token.as_deref())?;
            state
                .db
                .delete_paste(
                    &paste.id,
                    user.as_ref().map(|user| user.id.as_str()),
                    "web delete",
                )
                .await?;
        }
        _ => return Err(AppError::NotFound),
    }
    render(
        &state,
        "delete_result.html",
        &settings,
        user.as_ref(),
        serde_json::json!({ "kind": kind, "id": id }),
    )
}

async fn claim_form(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((kind, id)): Path<(String, String)>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !policy::allowed(settings.policy.claim_anonymous_item, Some(&user)) {
        return Err(AppError::Forbidden);
    }
    render(
        &state,
        "claim_form.html",
        &settings,
        Some(&user),
        serde_json::json!({ "kind": kind, "id": id }),
    )
}

#[derive(Debug, Deserialize)]
struct ClaimForm {
    token: String,
    csrf_token: Option<String>,
}

async fn claim_item(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((kind, id)): Path<(String, String)>,
    axum::Form(form): axum::Form<ClaimForm>,
) -> AppResult<Redirect> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    if !policy::allowed(settings.policy.claim_anonymous_item, Some(&user)) {
        return Err(AppError::Forbidden);
    }
    let token = form.token.trim();
    if token.is_empty() {
        return Err(AppError::BadRequest("claim token is required".to_string()));
    }
    let token_hash = util::hash_token(token);
    let claimed = match kind.as_str() {
        "file" => {
            state
                .db
                .claim_file_by_public_id(&id, &user.id, &token_hash)
                .await?
        }
        "paste" => {
            state
                .db
                .claim_paste_by_public_id(&id, &user.id, &token_hash)
                .await?
        }
        _ => return Err(AppError::NotFound),
    };
    if !claimed {
        return Err(AppError::BadRequest(
            "invalid token or item is not claimable".to_string(),
        ));
    }
    Ok(Redirect::to("/account"))
}

async fn login_form(State(state): State<AppState>, jar: CookieJar) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    let oidc_enabled = oidc::enabled(&state, &settings);
    render(
        &state,
        "login.html",
        &settings,
        user.as_ref(),
        serde_json::json!({ "oidc_enabled": oidc_enabled }),
    )
}

#[derive(Debug, Deserialize)]
struct LoginForm {
    email: String,
    password: String,
    csrf_token: Option<String>,
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    axum::Form(form): axum::Form<LoginForm>,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    enforce_rate_limit(&state, &settings, "login", &headers, None).await?;
    let user = match state.db.user_by_email(&form.email).await {
        Ok(user) => user,
        Err(_) => {
            state
                .db
                .audit(None, "auth.login_failed", &form.email, "unknown email")
                .await?;
            return Err(AppError::Unauthorized);
        }
    };
    let Some(password_hash) = &user.password_hash else {
        state
            .db
            .audit(
                Some(&user.id),
                "auth.login_failed",
                &user.id,
                "local password unavailable",
            )
            .await?;
        return Err(AppError::Unauthorized);
    };
    if !util::verify_password(&form.password, password_hash) {
        state
            .db
            .audit(
                Some(&user.id),
                "auth.login_failed",
                &user.id,
                "bad password",
            )
            .await?;
        return Err(AppError::Unauthorized);
    }
    if user.email_verified_at.is_none() {
        state
            .db
            .audit(
                Some(&user.id),
                "auth.login_failed",
                &user.id,
                "email unverified",
            )
            .await?;
        return Err(AppError::BadRequest(
            "email verification is required before login".to_string(),
        ));
    }
    if user.two_factor_enabled {
        return start_two_factor_challenge(&state, jar, &user).await;
    }
    state
        .db
        .audit(Some(&user.id), "auth.login", &user.id, "password")
        .await?;
    create_session_response(&state, jar, &user).await
}

async fn start_two_factor_challenge(
    state: &AppState,
    jar: CookieJar,
    user: &User,
) -> AppResult<Response> {
    if !state.mailer.enabled() {
        state
            .db
            .audit(
                Some(&user.id),
                "auth.login_failed",
                &user.id,
                "two-factor email unavailable",
            )
            .await?;
        return Err(AppError::BadRequest(
            "two-factor email is unavailable for this instance".to_string(),
        ));
    }
    let challenge = util::secret_token();
    let code = util::public_id().to_ascii_uppercase();
    state
        .db
        .create_two_factor_challenge(
            &user.id,
            &util::hash_token(&challenge),
            &util::hash_token(&code),
            util::now_ts() + 10 * 60,
        )
        .await?;
    state
        .mailer
        .send(
            &user.email,
            "Your Midden sign-in code",
            &format!(
                "Use this code to finish signing in:\n\n{code}\n\nThe code expires in 10 minutes."
            ),
        )
        .await?;
    state
        .db
        .audit(
            Some(&user.id),
            "auth.2fa_challenge_created",
            &user.id,
            "email",
        )
        .await?;
    Ok((
        jar.add(transient_cookie(TWO_FACTOR_CHALLENGE_COOKIE, challenge)),
        Redirect::to("/auth/2fa"),
    )
        .into_response())
}

async fn create_session_response(
    state: &AppState,
    jar: CookieJar,
    user: &User,
) -> AppResult<Response> {
    let token = util::secret_token();
    let token_hash = util::hash_token(&token);
    let expires = util::now_ts() + state.config.security.session_ttl_seconds;
    state
        .db
        .create_session(&user.id, &token_hash, expires)
        .await?;
    let cookie = session_cookie(
        state,
        token,
        Some(state.config.security.session_ttl_seconds),
    );
    Ok((jar.add(cookie), Redirect::to("/account")).into_response())
}

async fn two_factor_form(State(state): State<AppState>, jar: CookieJar) -> AppResult<Html<String>> {
    if jar.get(TWO_FACTOR_CHALLENGE_COOKIE).is_none() {
        return Err(AppError::BadRequest(
            "missing two-factor challenge".to_string(),
        ));
    }
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    render(
        &state,
        "two_factor.html",
        &settings,
        user.as_ref(),
        serde_json::json!({}),
    )
}

#[derive(Debug, Deserialize)]
struct TwoFactorSubmitForm {
    code: String,
    csrf_token: Option<String>,
}

async fn two_factor_submit(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<TwoFactorSubmitForm>,
) -> AppResult<Response> {
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    let challenge = jar
        .get(TWO_FACTOR_CHALLENGE_COOKIE)
        .map(|cookie| cookie.value().to_string())
        .ok_or_else(|| AppError::BadRequest("missing two-factor challenge".to_string()))?;
    let user = match state
        .db
        .consume_two_factor_challenge(
            &util::hash_token(&challenge),
            &util::hash_token(&form.code.trim().to_ascii_uppercase()),
        )
        .await
    {
        Ok(user) => user,
        Err(_) => {
            state
                .db
                .audit(None, "auth.2fa_failed", "two_factor_challenge", "bad code")
                .await?;
            return Err(AppError::Unauthorized);
        }
    };
    state
        .db
        .audit(Some(&user.id), "auth.login", &user.id, "two-factor")
        .await?;
    create_session_response(
        &state,
        jar.remove(transient_cookie(TWO_FACTOR_CHALLENGE_COOKIE, String::new())),
        &user,
    )
    .await
}

#[derive(Debug, Deserialize)]
struct CsrfForm {
    csrf_token: Option<String>,
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<CsrfForm>,
) -> AppResult<Response> {
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    if let Some(cookie) = jar.get(&state.config.security.session_cookie_name) {
        state
            .db
            .delete_session(&util::hash_token(cookie.value()))
            .await?;
    }
    let cookie = session_cookie(&state, String::new(), Some(0));
    Ok((jar.remove(cookie), Redirect::to("/")).into_response())
}

async fn password_reset_request_form(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    render(
        &state,
        "password_reset_request.html",
        &settings,
        user.as_ref(),
        serde_json::json!({ "sent": false, "smtp_enabled": state.mailer.enabled() }),
    )
}

#[derive(Debug, Deserialize)]
struct PasswordResetRequestForm {
    email: String,
    csrf_token: Option<String>,
}

async fn password_reset_request(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    axum::Form(form): axum::Form<PasswordResetRequestForm>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    enforce_rate_limit(&state, &settings, "password_reset", &headers, user.as_ref()).await?;
    if state.mailer.enabled()
        && let Ok(reset_user) = state.db.user_by_email(&form.email).await
    {
        let token = util::secret_token();
        state
            .db
            .create_password_reset_token(
                &reset_user.id,
                &util::hash_token(&token),
                util::now_ts() + 60 * 60,
            )
            .await?;
        let reset_url = format!(
            "{}/auth/password-reset/{}",
            state.config.server.public_base_url.trim_end_matches('/'),
            token
        );
        let _ = state
            .mailer
            .send(
                &reset_user.email,
                "Reset your Midden password",
                &format!("Use this link to reset your password:\n\n{reset_url}\n\nThe link expires in one hour."),
            )
            .await?;
    }
    render(
        &state,
        "password_reset_request.html",
        &settings,
        user.as_ref(),
        serde_json::json!({ "sent": true, "smtp_enabled": state.mailer.enabled() }),
    )
}

async fn password_reset_form(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token): Path<String>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    render(
        &state,
        "password_reset_form.html",
        &settings,
        user.as_ref(),
        serde_json::json!({ "token": token }),
    )
}

#[derive(Debug, Deserialize)]
struct PasswordResetSubmitForm {
    password: String,
    csrf_token: Option<String>,
}

async fn password_reset_submit(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token): Path<String>,
    axum::Form(form): axum::Form<PasswordResetSubmitForm>,
) -> AppResult<Response> {
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    let reset_user = state
        .db
        .consume_password_reset_token(&util::hash_token(&token))
        .await
        .map_err(|_| AppError::BadRequest("invalid or expired password reset token".to_string()))?;
    let password_hash = util::hash_password(&form.password)?;
    state
        .db
        .update_user_password(&reset_user.id, &password_hash)
        .await?;
    state
        .db
        .set_user_email_verified_at(&reset_user.id, Some(util::now_ts()))
        .await?;
    state
        .db
        .audit(
            Some(&reset_user.id),
            "user.password_reset",
            &reset_user.id,
            "email token",
        )
        .await?;
    create_session_response(&state, jar, &reset_user).await
}

async fn verify_email(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token): Path<String>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let current = current_user(&state, &jar).await?;
    let verified = state
        .db
        .consume_email_verification_token(&util::hash_token(&token))
        .await
        .map_err(|_| {
            AppError::BadRequest("invalid or expired email verification token".to_string())
        })?;
    state
        .db
        .audit(
            Some(&verified.id),
            "user.email_verified",
            &verified.id,
            "email token",
        )
        .await?;
    render(
        &state,
        "email_verified.html",
        &settings,
        current.as_ref(),
        serde_json::json!({ "email": verified.email }),
    )
}

async fn register_form(State(state): State<AppState>, jar: CookieJar) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    if !matches!(
        settings.policy.signup,
        crate::config::SignupMode::Open | crate::config::SignupMode::InviteOnly
    ) {
        return Err(AppError::Forbidden);
    }
    let invite_required = matches!(
        settings.policy.signup,
        crate::config::SignupMode::InviteOnly
    );
    render(
        &state,
        "register.html",
        &settings,
        user.as_ref(),
        serde_json::json!({ "invite_required": invite_required }),
    )
}

#[derive(Debug, Deserialize)]
struct RegisterForm {
    email: String,
    username: String,
    password: String,
    invite_token: Option<String>,
    csrf_token: Option<String>,
}

async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<RegisterForm>,
) -> AppResult<Redirect> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    if !matches!(
        settings.policy.signup,
        crate::config::SignupMode::Open | crate::config::SignupMode::InviteOnly
    ) {
        return Err(AppError::Forbidden);
    }
    if user.is_some() {
        return Ok(Redirect::to("/account"));
    }
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    let password_hash = util::hash_password(&form.password)?;
    let requires_email_verification =
        matches!(settings.policy.signup, crate::config::SignupMode::Open) && state.mailer.enabled();
    let created = state
        .db
        .create_user(
            &form.email,
            &form.username,
            Some(&password_hash),
            Role::User,
        )
        .await?;
    if matches!(
        settings.policy.signup,
        crate::config::SignupMode::InviteOnly
    ) {
        let token = form
            .invite_token
            .as_deref()
            .ok_or_else(|| AppError::BadRequest("invite token is required".to_string()))?;
        let role = state
            .db
            .consume_invite_token(&util::hash_token(token), &created.id)
            .await
            .map_err(|_| AppError::BadRequest("invalid invite token".to_string()))?;
        state.db.set_user_role(&created.id, role).await?;
    }
    state
        .db
        .audit(Some(&created.id), "user.created", &created.id, "signup")
        .await?;
    if requires_email_verification {
        state
            .db
            .set_user_email_verified_at(&created.id, None)
            .await?;
        send_email_verification(&state, &created).await?;
        state
            .db
            .audit(
                Some(&created.id),
                "user.email_verification_sent",
                &created.id,
                "signup",
            )
            .await?;
    }
    Ok(Redirect::to("/auth/login"))
}

async fn send_email_verification(state: &AppState, user: &User) -> AppResult<()> {
    if !state.mailer.enabled() {
        return Err(AppError::BadRequest(
            "email verification requires SMTP".to_string(),
        ));
    }
    let token = util::secret_token();
    state
        .db
        .create_email_verification_token(
            &user.id,
            &util::hash_token(&token),
            util::now_ts() + 24 * 60 * 60,
        )
        .await?;
    let verify_url = format!(
        "{}/auth/verify-email/{}",
        state.config.server.public_base_url.trim_end_matches('/'),
        token
    );
    state
        .mailer
        .send(
            &user.email,
            "Verify your Midden email",
            &format!("Use this link to verify your email address:\n\n{verify_url}\n\nThe link expires in 24 hours."),
        )
        .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct AccountQuery {
    q: Option<String>,
}

async fn account(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<AccountQuery>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let oidc_link_enabled = oidc::enabled(&state, &settings);
    let q = query.q.unwrap_or_default();
    let (files, pastes) = user_items_for_query(&state, &settings, &user, &q).await?;
    let tokens = state.db.list_api_tokens(&user.id).await?;
    render(
        &state,
        "account.html",
        &settings,
        Some(&user),
        serde_json::json!({
            "q": q,
            "files": files,
            "pastes": pastes,
            "tokens": tokens,
            "new_token": null,
            "has_local_password": user.password_hash.is_some(),
            "email_verified": user.email_verified_at.is_some(),
            "two_factor_enabled": user.two_factor_enabled,
            "smtp_enabled": state.mailer.enabled(),
            "oidc_link_enabled": oidc_link_enabled,
        }),
    )
}

#[derive(Debug, Deserialize)]
struct AccountTokenForm {
    name: String,
    scopes: String,
    csrf_token: Option<String>,
}

async fn account_create_token(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<AccountTokenForm>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let oidc_link_enabled = oidc::enabled(&state, &settings);
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    let scopes = parse_scopes(&form.scopes);
    if scopes.is_empty() {
        return Err(AppError::BadRequest(
            "at least one scope is required".to_string(),
        ));
    }
    let token = format!("mdd_{}", util::secret_token());
    state
        .db
        .create_api_token(&user.id, &form.name, &util::hash_token(&token), &scopes)
        .await?;
    state
        .db
        .audit(Some(&user.id), "api_token.created", &user.id, &form.name)
        .await?;
    let files = state.db.recent_user_files(&user.id).await?;
    let pastes = state.db.recent_user_pastes(&user.id).await?;
    let tokens = state.db.list_api_tokens(&user.id).await?;
    render(
        &state,
        "account.html",
        &settings,
        Some(&user),
        serde_json::json!({
            "q": "",
            "files": files,
            "pastes": pastes,
            "tokens": tokens,
            "new_token": token,
            "has_local_password": user.password_hash.is_some(),
            "email_verified": user.email_verified_at.is_some(),
            "two_factor_enabled": user.two_factor_enabled,
            "smtp_enabled": state.mailer.enabled(),
            "oidc_link_enabled": oidc_link_enabled,
        }),
    )
}

async fn account_revoke_token(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    axum::Form(form): axum::Form<CsrfForm>,
) -> AppResult<Redirect> {
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    state.db.revoke_api_token(&user.id, &id).await?;
    state
        .db
        .audit(Some(&user.id), "api_token.revoked", &user.id, &id)
        .await?;
    Ok(Redirect::to("/account"))
}

async fn account_send_email_verification(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<CsrfForm>,
) -> AppResult<Redirect> {
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    if user.email_verified_at.is_some() {
        return Ok(Redirect::to("/account"));
    }
    send_email_verification(&state, &user).await?;
    state
        .db
        .audit(
            Some(&user.id),
            "user.email_verification_sent",
            &user.id,
            "account UI",
        )
        .await?;
    Ok(Redirect::to("/account"))
}

#[derive(Debug, Deserialize)]
struct AccountPasswordForm {
    current_password: String,
    new_password: String,
    csrf_token: Option<String>,
}

async fn account_change_password(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<AccountPasswordForm>,
) -> AppResult<Redirect> {
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    let Some(password_hash) = &user.password_hash else {
        return Err(AppError::BadRequest(
            "OIDC-only accounts do not have a local password".to_string(),
        ));
    };
    if !util::verify_password(&form.current_password, password_hash) {
        return Err(AppError::Unauthorized);
    }
    let new_hash = util::hash_password(&form.new_password)?;
    state.db.update_user_password(&user.id, &new_hash).await?;
    state
        .db
        .audit(
            Some(&user.id),
            "user.password_changed",
            &user.id,
            "account UI",
        )
        .await?;
    Ok(Redirect::to("/account"))
}

#[derive(Debug, Deserialize)]
struct AccountTwoFactorForm {
    current_password: String,
    csrf_token: Option<String>,
}

async fn account_enable_two_factor(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<AccountTwoFactorForm>,
) -> AppResult<Redirect> {
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    if !state.mailer.enabled() {
        return Err(AppError::BadRequest(
            "two-factor authentication requires SMTP".to_string(),
        ));
    }
    if user.email_verified_at.is_none() {
        return Err(AppError::BadRequest(
            "verify your email before enabling two-factor authentication".to_string(),
        ));
    }
    verify_current_password(&user, &form.current_password)?;
    state.db.set_user_two_factor_enabled(&user.id, true).await?;
    state
        .db
        .audit(
            Some(&user.id),
            "user.two_factor_enabled",
            &user.id,
            "account UI",
        )
        .await?;
    Ok(Redirect::to("/account"))
}

async fn account_disable_two_factor(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<AccountTwoFactorForm>,
) -> AppResult<Redirect> {
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    verify_current_password(&user, &form.current_password)?;
    state
        .db
        .set_user_two_factor_enabled(&user.id, false)
        .await?;
    state
        .db
        .audit(
            Some(&user.id),
            "user.two_factor_disabled",
            &user.id,
            "account UI",
        )
        .await?;
    Ok(Redirect::to("/account"))
}

fn verify_current_password(user: &User, password: &str) -> AppResult<()> {
    let Some(password_hash) = &user.password_hash else {
        return Err(AppError::BadRequest(
            "OIDC-only accounts do not have a local password".to_string(),
        ));
    };
    if !util::verify_password(password, password_hash) {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

async fn account_deactivate(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<CsrfForm>,
) -> AppResult<Response> {
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    state.db.set_user_disabled(&user.id, true).await?;
    state
        .db
        .audit(Some(&user.id), "user.deactivated", &user.id, "account UI")
        .await?;
    let cookie = session_cookie(&state, String::new(), Some(0));
    Ok((jar.remove(cookie), Redirect::to("/")).into_response())
}

async fn user_items_for_query(
    state: &AppState,
    settings: &RuntimeSettings,
    user: &User,
    query: &str,
) -> AppResult<(Vec<FileItem>, Vec<Paste>)> {
    if query.trim().is_empty() {
        Ok((
            state.db.recent_user_files(&user.id).await?,
            state.db.recent_user_pastes(&user.id).await?,
        ))
    } else {
        Ok((
            state.db.search_user_files(&user.id, query).await?,
            state
                .db
                .search_user_pastes(&user.id, query, settings.features.paste_content_search)
                .await?,
        ))
    }
}

#[derive(Debug, Deserialize)]
struct BrowseQuery {
    q: Option<String>,
    before: Option<i64>,
}

async fn public_browse(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<BrowseQuery>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    if !settings.features.public_browse {
        return Err(AppError::NotFound);
    }
    let user = current_user(&state, &jar).await?;
    let limit = settings.discovery.page_size.clamp(1, 100) as i64;
    let q = query.q.as_deref().filter(|q| !q.trim().is_empty());
    let files = state.db.public_files(q, query.before, limit).await?;
    let pastes = state.db.public_pastes(q, query.before, limit).await?;
    let next_cursor = files
        .iter()
        .map(|file| file.created_at)
        .chain(pastes.iter().map(|paste| paste.created_at))
        .min();
    render(
        &state,
        "browse.html",
        &settings,
        user.as_ref(),
        serde_json::json!({
            "q": query.q.unwrap_or_default(),
            "files": files,
            "pastes": pastes,
            "next_cursor": next_cursor,
        }),
    )
}

async fn robots_txt(State(state): State<AppState>) -> AppResult<Response> {
    let settings = state.settings().await?;
    let body = if settings.features.public_browse && settings.discovery.robots_index {
        "User-agent: *\nAllow: /browse\n"
    } else {
        "User-agent: *\nDisallow: /\n"
    };
    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )],
        body,
    )
        .into_response())
}

async fn healthz() -> &'static str {
    "ok\n"
}

async fn readyz(State(state): State<AppState>) -> Response {
    let database = state.db.health().await;
    let storage = state.storage.health().await;
    if database && storage {
        (StatusCode::OK, "database=true\nstorage=true\n").into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("database={database}\nstorage={storage}\n"),
        )
            .into_response()
    }
}

async fn metrics(State(state): State<AppState>) -> AppResult<Response> {
    let mut body = String::new();
    encode(&mut body, &state.registry)
        .map_err(|err| AppError::Other(anyhow::anyhow!("metrics encode failed: {err}")))?;
    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/openmetrics-text; version=1.0.0; charset=utf-8"),
        )],
        body,
    )
        .into_response())
}

async fn static_asset(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> AppResult<Response> {
    if path.contains("..") || path.starts_with('/') {
        return Err(AppError::NotFound);
    }
    let settings = state.settings().await?;
    if let Some(static_dir) = &state.config.server.static_dir {
        let disk_path = static_dir.join(&path);
        if disk_path.exists() && disk_path.is_file() {
            let bytes = tokio::fs::read(&disk_path).await?;
            let content_type = mime_guess::from_path(&disk_path).first_or_octet_stream();
            let mut response = (
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_str(content_type.as_ref()).unwrap(),
                )],
                bytes,
            )
                .into_response();
            insert_cache_control(
                &mut response,
                settings.delivery.static_cache_seconds,
                CacheScope::Public,
            );
            return Ok(response);
        }
    }
    let mut response = match path.as_str() {
        "midden.css" => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/css; charset=utf-8"),
            )],
            include_str!("../static/midden.css"),
        )
            .into_response(),
        "midden.js" => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            )],
            include_str!("../static/midden.js"),
        )
            .into_response(),
        _ => return Err(AppError::NotFound),
    };
    insert_cache_control(
        &mut response,
        settings.delivery.static_cache_seconds,
        CacheScope::Public,
    );
    Ok(response)
}

fn anonymous_delete_token(settings: &RuntimeSettings, user: Option<&User>) -> Option<String> {
    if user.is_some() {
        return None;
    }
    match settings.policy.delete_policy {
        DeletePolicy::DeleteTokens | DeletePolicy::ClaimLater => Some(util::secret_token()),
        DeletePolicy::Disabled | DeletePolicy::NoAnonymousDelete => None,
    }
}

fn authorize_file_delete(
    settings: &RuntimeSettings,
    user: Option<&User>,
    file: &FileItem,
    provided_token: Option<&str>,
) -> AppResult<()> {
    if user_can_delete_owned(settings, user, file.owner_user_id.as_deref()) {
        return Ok(());
    }
    if token_can_delete(settings, provided_token, file.delete_token_hash.as_deref()) {
        return Ok(());
    }
    Err(AppError::Forbidden)
}

fn authorize_paste_delete(
    settings: &RuntimeSettings,
    user: Option<&User>,
    paste: &Paste,
    provided_token: Option<&str>,
) -> AppResult<()> {
    if user_can_delete_owned(settings, user, paste.owner_user_id.as_deref()) {
        return Ok(());
    }
    if token_can_delete(settings, provided_token, paste.delete_token_hash.as_deref()) {
        return Ok(());
    }
    Err(AppError::Forbidden)
}

fn can_edit_paste(settings: &RuntimeSettings, user: Option<&User>, paste: &Paste) -> bool {
    if !settings.features.paste_editing {
        return false;
    }
    let Some(user) = user else {
        return false;
    };
    user.role >= Role::Admin || paste.owner_user_id.as_deref() == Some(user.id.as_str())
}

fn user_can_delete_owned(
    settings: &RuntimeSettings,
    user: Option<&User>,
    owner_user_id: Option<&str>,
) -> bool {
    let Some(user) = user else {
        return false;
    };
    if user.role >= Role::Admin {
        return true;
    }
    owner_user_id == Some(user.id.as_str())
        && policy::allowed(settings.policy.delete_own_item, Some(user))
        && !matches!(settings.policy.delete_own_item, ActionRule::Disabled)
}

fn token_can_delete(
    settings: &RuntimeSettings,
    provided_token: Option<&str>,
    stored_hash: Option<&str>,
) -> bool {
    if !matches!(
        settings.policy.delete_policy,
        DeletePolicy::DeleteTokens | DeletePolicy::ClaimLater
    ) {
        return false;
    }
    let (Some(provided), Some(stored_hash)) = (provided_token, stored_hash) else {
        return false;
    };
    util::hash_token(provided) == stored_hash
}

fn render_paste_content(content: &str, syntax: Option<&str>) -> String {
    let Some(syntax) = syntax.filter(|value| !value.trim().is_empty()) else {
        return format!(
            "<pre class=\"paste-body\"><code>{}</code></pre>",
            html_escape::encode_text(content)
        );
    };
    let syntax_set = syntect::parsing::SyntaxSet::load_defaults_newlines();
    let theme_set = syntect::highlighting::ThemeSet::load_defaults();
    let syntax_ref = syntax_set
        .find_syntax_by_token(syntax)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    match syntect::html::highlighted_html_for_string(
        content,
        &syntax_set,
        syntax_ref,
        &theme_set.themes["base16-ocean.dark"],
    ) {
        Ok(html) => html,
        Err(_) => format!(
            "<pre class=\"paste-body\"><code>{}</code></pre>",
            html_escape::encode_text(content)
        ),
    }
}

fn parse_i64_header(headers: &HeaderMap, name: &'static str) -> AppResult<i64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| AppError::BadRequest(format!("missing or invalid {name} header")))
}

fn parse_tus_metadata(headers: &HeaderMap) -> BTreeMap<String, String> {
    let Some(value) = headers
        .get("Upload-Metadata")
        .and_then(|value| value.to_str().ok())
    else {
        return BTreeMap::new();
    };
    value
        .split(',')
        .filter_map(|pair| {
            let mut parts = pair.trim().splitn(2, ' ');
            let key = parts.next()?.trim().replace('-', "_");
            let encoded = parts.next().unwrap_or_default();
            let decoded =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())?;
            Some((key, decoded))
        })
        .collect()
}

fn parse_scopes(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn requested_visibility(
    settings: &RuntimeSettings,
    value: Option<&str>,
) -> AppResult<&'static str> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("unlisted") => Ok("unlisted"),
        Some("public") if settings.features.public_browse => Ok("public"),
        Some("public") => Err(AppError::BadRequest(
            "public visibility requires public browse to be enabled".to_string(),
        )),
        _ => Err(AppError::BadRequest("invalid visibility".to_string())),
    }
}

fn parse_expiry_or_default(
    input: Option<&str>,
    default_input: Option<&str>,
) -> anyhow::Result<Option<i64>> {
    let selected = input
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            default_input
                .map(str::trim)
                .filter(|value| !value.is_empty())
        });
    util::parse_expiry(selected)
}

fn normalize_syntax(input: Option<&str>) -> Option<String> {
    let syntax = input?.trim().to_ascii_lowercase();
    if syntax.is_empty() {
        return None;
    }
    Some(
        match syntax.as_str() {
            "txt" | "plain" => "text",
            "js" | "mjs" | "cjs" => "javascript",
            "ts" => "typescript",
            "py" => "python",
            "rb" => "ruby",
            "rs" => "rust",
            "sh" | "shell" => "bash",
            "yml" => "yaml",
            "md" => "markdown",
            "htm" => "html",
            other => other,
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::Request;
    use std::sync::Arc;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use tower::ServiceExt;

    async fn test_state(issuer_url: String) -> AppState {
        let mut config = crate::config::AppConfig::default();
        config.database.url = "sqlite::memory:".to_string();
        config.database.max_connections = 1;
        config.storage.local.path =
            std::env::temp_dir().join(format!("midden-test-{}", util::public_id()));
        config.features.oidc_login = true;
        config.oidc.enabled = true;
        config.oidc.issuer_url = Some(issuer_url);
        config.oidc.client_id = Some("midden-test".to_string());
        config.oidc.allowed_domains = vec!["example.test".to_string()];
        config.oidc.allowed_groups = vec!["admins".to_string()];
        config
            .oidc
            .role_mappings
            .insert("admins".to_string(), "admin".to_string());
        let state = AppState::new(config).await.unwrap();
        state.db.migrate().await.unwrap();
        state
    }

    async fn spawn_oidc_provider(userinfo: serde_json::Value) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");
        let base_for_task = base_url.clone();
        let userinfo = Arc::new(userinfo.to_string());
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let base = base_for_task.clone();
                let userinfo = userinfo.clone();
                tokio::spawn(async move {
                    let mut buffer = [0_u8; 4096];
                    let Ok(read) = stream.read(&mut buffer).await else {
                        return;
                    };
                    let request = String::from_utf8_lossy(&buffer[..read]);
                    let path = request.split_whitespace().nth(1).unwrap_or("/");
                    let body = match path {
                        "/.well-known/openid-configuration" => serde_json::json!({
                            "authorization_endpoint": format!("{base}/authorize"),
                            "token_endpoint": format!("{base}/token"),
                            "userinfo_endpoint": format!("{base}/userinfo")
                        })
                        .to_string(),
                        "/token" => {
                            serde_json::json!({ "access_token": "mock-access-token" }).to_string()
                        }
                        "/userinfo" => userinfo.to_string(),
                        _ => "{}".to_string(),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        base_url
    }

    async fn spawn_http_app(state: AppState) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = state.router();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        format!("http://{addr}")
    }

    async fn user_with_api_token(
        state: &AppState,
        email: &str,
        username: &str,
        role: Role,
        scopes: &[&str],
    ) -> (User, String) {
        let user = state
            .db
            .create_user(email, username, Some("password-hash"), role)
            .await
            .unwrap();
        let token = format!("mdd_{}", util::secret_token());
        let scopes = scopes
            .iter()
            .map(|scope| scope.to_string())
            .collect::<Vec<_>>();
        state
            .db
            .create_api_token(&user.id, "test", &util::hash_token(&token), &scopes)
            .await
            .unwrap();
        (user, token)
    }

    fn hex_fixture(input: &str) -> Vec<u8> {
        let compact = input
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        compact
            .as_bytes()
            .chunks(2)
            .map(|chunk| {
                let text = std::str::from_utf8(chunk).unwrap();
                u8::from_str_radix(text, 16).unwrap()
            })
            .collect()
    }

    fn tus_metadata(filename: &str, content_type: &str) -> String {
        let engine = &base64::engine::general_purpose::STANDARD;
        format!(
            "filename {},content-type {}",
            base64::Engine::encode(engine, filename),
            base64::Engine::encode(engine, content_type)
        )
    }

    #[tokio::test]
    async fn http_release_flow_covers_upload_paste_claim_reports_admin_search_and_scopes() {
        let issuer = spawn_oidc_provider(serde_json::json!({
            "sub": "unused",
            "email": "unused@example.test",
            "groups": ["admins"]
        }))
        .await;
        let state = test_state(issuer).await;
        let base = spawn_http_app(state.clone()).await;
        let client = reqwest::Client::new();
        let (_user, user_token) = user_with_api_token(
            &state,
            "api-user@example.test",
            "api-user",
            Role::User,
            &[
                "files:read",
                "pastes:read",
                "items:claim",
                "tokens:read",
                "tokens:write",
            ],
        )
        .await;
        let (_admin, admin_token) = user_with_api_token(
            &state,
            "admin@example.test",
            "admin",
            Role::Admin,
            &["admin:reports", "admin:items", "admin:search"],
        )
        .await;

        let png = hex_fixture(include_str!("../tests/fixtures/sample.png.hex"));
        let upload = client
            .post(format!("{base}/api/v1/files"))
            .multipart(
                reqwest::multipart::Form::new().part(
                    "file",
                    reqwest::multipart::Part::bytes(png)
                        .file_name("sample.png")
                        .mime_str("image/png")
                        .unwrap(),
                ),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(upload.status(), StatusCode::OK);
        let upload: serde_json::Value = upload.json().await.unwrap();
        let file_id = upload["id"].as_str().unwrap().to_string();
        let file_delete_token = upload["delete_token"].as_str().unwrap().to_string();

        let paste = client
            .post(format!("{base}/api/v1/pastes"))
            .json(&serde_json::json!({
                "title": "Fixture paste",
                "syntax": "txt",
                "content": include_str!("../tests/fixtures/sample.txt")
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(paste.status(), StatusCode::OK);
        let paste: serde_json::Value = paste.json().await.unwrap();
        let paste_id = paste["id"].as_str().unwrap().to_string();
        let paste_delete_token = paste["delete_token"].as_str().unwrap().to_string();

        let paste_delete = client
            .delete(format!("{base}/api/v1/pastes/{paste_id}"))
            .header("x-delete-token", paste_delete_token)
            .send()
            .await
            .unwrap();
        assert_eq!(paste_delete.status(), StatusCode::OK);

        let claim = client
            .post(format!("{base}/api/v1/claim/file/{file_id}"))
            .bearer_auth(&user_token)
            .json(&serde_json::json!({ "delete_token": file_delete_token }))
            .send()
            .await
            .unwrap();
        assert_eq!(claim.status(), StatusCode::OK);

        let files = client
            .get(format!("{base}/api/v1/me/files?q=sample"))
            .bearer_auth(&user_token)
            .send()
            .await
            .unwrap();
        assert_eq!(files.status(), StatusCode::OK);
        let files: serde_json::Value = files.json().await.unwrap();
        assert_eq!(files["items"].as_array().unwrap().len(), 1);

        let report = client
            .post(format!("{base}/api/v1/reports"))
            .json(&serde_json::json!({
                "kind": "file",
                "id": file_id,
                "reason": "abuse",
                "details": "release-flow"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(report.status(), StatusCode::OK);

        let reports = client
            .get(format!("{base}/api/v1/admin/reports?state=open"))
            .bearer_auth(&admin_token)
            .send()
            .await
            .unwrap();
        assert_eq!(reports.status(), StatusCode::OK);
        let reports: serde_json::Value = reports.json().await.unwrap();
        let report_id = reports["items"][0]["id"].as_str().unwrap();

        let report_update = client
            .patch(format!("{base}/api/v1/admin/reports/{report_id}"))
            .bearer_auth(&admin_token)
            .json(&serde_json::json!({ "action": "resolve", "note": "handled" }))
            .send()
            .await
            .unwrap();
        assert_eq!(report_update.status(), StatusCode::OK);

        let search = client
            .get(format!("{base}/api/v1/admin/search?q=sample"))
            .bearer_auth(&admin_token)
            .send()
            .await
            .unwrap();
        assert_eq!(search.status(), StatusCode::OK);
        let search: serde_json::Value = search.json().await.unwrap();
        assert_eq!(search["files"].as_array().unwrap().len(), 1);

        let created_token = client
            .post(format!("{base}/api/v1/tokens"))
            .bearer_auth(&user_token)
            .json(&serde_json::json!({ "name": "limited", "scopes": ["files:read"] }))
            .send()
            .await
            .unwrap();
        assert_eq!(created_token.status(), StatusCode::OK);
        let created_token: serde_json::Value = created_token.json().await.unwrap();
        let limited_token = created_token["token"].as_str().unwrap();

        assert_eq!(
            client
                .get(format!("{base}/api/v1/me/files"))
                .bearer_auth(limited_token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            client
                .get(format!("{base}/api/v1/me/pastes"))
                .bearer_auth(limited_token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn tus_http_flow_covers_offsets_completion_ownership_and_policy() {
        let issuer = spawn_oidc_provider(serde_json::json!({
            "sub": "unused-tus",
            "email": "unused-tus@example.test",
            "groups": ["admins"]
        }))
        .await;
        let state = test_state(issuer).await;
        let base = spawn_http_app(state.clone()).await;
        let client = reqwest::Client::new();
        let payload = hex_fixture(include_str!("../tests/fixtures/sample.gif.hex"));

        let create = client
            .post(format!("{base}/tus"))
            .header("Tus-Resumable", "1.0.0")
            .header("Upload-Length", payload.len().to_string())
            .header("Upload-Metadata", tus_metadata("sample.gif", "image/gif"))
            .send()
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::CREATED);
        let location = create.headers()["location"].to_str().unwrap().to_string();

        let head = client
            .head(format!("{base}{location}"))
            .send()
            .await
            .unwrap();
        assert_eq!(head.status(), StatusCode::NO_CONTENT);
        assert_eq!(head.headers()["upload-offset"].to_str().unwrap(), "0");

        let mismatch = client
            .patch(format!("{base}{location}"))
            .header("Tus-Resumable", "1.0.0")
            .header("Upload-Offset", "1")
            .body(payload.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(mismatch.status(), StatusCode::BAD_REQUEST);

        let complete = client
            .patch(format!("{base}{location}"))
            .header("Tus-Resumable", "1.0.0")
            .header("Upload-Offset", "0")
            .body(payload)
            .send()
            .await
            .unwrap();
        assert_eq!(complete.status(), StatusCode::NO_CONTENT);
        assert!(complete.headers().contains_key("location"));

        let (owner, _) = user_with_api_token(
            &state,
            "tus-owner@example.test",
            "tus-owner",
            Role::User,
            &["files:read"],
        )
        .await;
        let session_token = util::secret_token();
        state
            .db
            .create_session(
                &owner.id,
                &util::hash_token(&session_token),
                util::now_ts() + 60,
            )
            .await
            .unwrap();
        let cookie = format!("midden_session={session_token}");
        let owned = client
            .post(format!("{base}/tus"))
            .header("cookie", &cookie)
            .header("Tus-Resumable", "1.0.0")
            .header("Upload-Length", "4")
            .header(
                "Upload-Metadata",
                tus_metadata("owned.bin", "application/octet-stream"),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(owned.status(), StatusCode::CREATED);
        let owned_location = owned.headers()["location"].to_str().unwrap();
        assert_eq!(
            client
                .head(format!("{base}{owned_location}"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            client
                .head(format!("{base}{owned_location}"))
                .header("cookie", &cookie)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );

        let mut policy = state.settings().await.unwrap().policy;
        policy.use_api = ActionRule::Disabled;
        state.db.set_json_setting("policy", &policy).await.unwrap();
        let denied = client
            .post(format!("{base}/tus"))
            .header("Tus-Resumable", "1.0.0")
            .header("Upload-Length", "1")
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn public_browse_only_lists_items_marked_public() {
        let issuer = spawn_oidc_provider(serde_json::json!({
            "sub": "unused-browse",
            "email": "unused-browse@example.test",
            "groups": ["admins"]
        }))
        .await;
        let state = test_state(issuer).await;
        let mut features = state.settings().await.unwrap().features;
        features.public_browse = true;
        state
            .db
            .set_json_setting("features", &features)
            .await
            .unwrap();
        let base = spawn_http_app(state).await;
        let client = reqwest::Client::new();

        for (name, visibility) in [("listed.txt", "public"), ("hidden.txt", "unlisted")] {
            let upload = client
                .post(format!("{base}/api/v1/files"))
                .multipart(
                    reqwest::multipart::Form::new()
                        .text("visibility", visibility.to_string())
                        .part(
                            "file",
                            reqwest::multipart::Part::bytes(format!("{name} body").into_bytes())
                                .file_name(name.to_string())
                                .mime_str("text/plain")
                                .unwrap(),
                        ),
                )
                .send()
                .await
                .unwrap();
            assert_eq!(upload.status(), StatusCode::OK);
        }

        let browse = client
            .get(format!("{base}/browse"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(browse.contains("listed.txt"));
        assert!(!browse.contains("hidden.txt"));
    }

    #[tokio::test]
    async fn signed_internal_urls_and_cache_headers_are_served_when_enabled() {
        let issuer = spawn_oidc_provider(serde_json::json!({
            "sub": "unused-signed",
            "email": "unused-signed@example.test",
            "groups": ["admins"]
        }))
        .await;
        let state = test_state(issuer).await;
        let mut delivery = state.settings().await.unwrap().delivery;
        delivery.public_cache_seconds = 42;
        delivery.static_cache_seconds = 84;
        delivery.signed_internal_urls = true;
        delivery.internal_url_secret = Some("test-secret".to_string());
        delivery.internal_url_ttl_seconds = 60;
        state
            .db
            .set_json_setting("delivery", &delivery)
            .await
            .unwrap();
        let base = spawn_http_app(state).await;
        let client = reqwest::Client::new();

        let upload = client
            .post(format!("{base}/api/v1/files"))
            .multipart(
                reqwest::multipart::Form::new().part(
                    "file",
                    reqwest::multipart::Part::bytes(b"cache me".to_vec())
                        .file_name("cache.txt")
                        .mime_str("text/plain")
                        .unwrap(),
                ),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(upload.status(), StatusCode::OK);
        let upload: serde_json::Value = upload.json().await.unwrap();
        let file_id = upload["id"].as_str().unwrap();
        let internal_url = upload["internal_url"].as_str().unwrap();
        let internal = url::Url::parse(internal_url).unwrap();
        let internal_path = match internal.query() {
            Some(query) => format!("{}?{query}", internal.path()),
            None => internal.path().to_string(),
        };

        let raw = client
            .get(format!("{base}/files/{file_id}/raw"))
            .send()
            .await
            .unwrap();
        assert_eq!(raw.status(), StatusCode::OK);
        assert_eq!(
            raw.headers()["cache-control"].to_str().unwrap(),
            "public, max-age=42"
        );

        let internal = client
            .get(format!("{base}{internal_path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(internal.status(), StatusCode::OK);
        assert_eq!(internal.bytes().await.unwrap().as_ref(), b"cache me");

        let static_asset = client
            .get(format!("{base}/static/midden.css"))
            .send()
            .await
            .unwrap();
        assert_eq!(static_asset.status(), StatusCode::OK);
        assert_eq!(
            static_asset.headers()["cache-control"].to_str().unwrap(),
            "public, max-age=84"
        );
    }

    #[tokio::test]
    async fn oidc_callback_provisions_with_allowed_claims_and_role_mapping() {
        let issuer = spawn_oidc_provider(serde_json::json!({
            "sub": "subject-1",
            "email": "oidc@example.test",
            "preferred_username": "oidc-user",
            "groups": ["admins"]
        }))
        .await;
        let state = test_state(issuer.clone()).await;
        let response = state
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .uri("/auth/oidc/callback?code=abc&state=state-1")
                    .header(
                        header::COOKIE,
                        "midden_oidc_state=state-1; midden_oidc_purpose=login",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let user = state.db.user_by_email("oidc@example.test").await.unwrap();
        assert_eq!(user.role, Role::Admin);
        assert!(
            state
                .db
                .user_by_oidc_identity(&issuer, "subject-1")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn oidc_callback_requires_explicit_link_for_existing_local_user() {
        let issuer = spawn_oidc_provider(serde_json::json!({
            "sub": "subject-2",
            "email": "local@example.test",
            "preferred_username": "local",
            "groups": ["admins"]
        }))
        .await;
        let state = test_state(issuer.clone()).await;
        state
            .db
            .create_user(
                "local@example.test",
                "local",
                Some("password-hash"),
                Role::User,
            )
            .await
            .unwrap();
        let response = state
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .uri("/auth/oidc/callback?code=abc&state=state-2")
                    .header(
                        header::COOKIE,
                        "midden_oidc_state=state-2; midden_oidc_purpose=login",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            state
                .db
                .user_by_oidc_identity(&issuer, "subject-2")
                .await
                .unwrap()
                .is_none()
        );
    }
}
