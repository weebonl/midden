use super::admin::{
    AdminReportActionForm, AdminReportsQuery, AdminSearchQuery, apply_report_action,
    update_item_state, update_item_visibility,
};
use super::*;

pub(super) async fn api_docs(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    render(
        &state,
        "docs.html",
        &settings,
        user.as_ref(),
        serde_json::json!({}),
    )
}

pub(super) async fn api_openapi() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Midden API",
            "version": env!("CARGO_PKG_VERSION")
        },
        "paths": {
            "/api/v1/files": {
                "post": {
                    "summary": "Upload a file",
                    "requestBody": { "content": { "multipart/form-data": {} } },
                    "responses": { "200": { "description": "Uploaded file" } }
                }
            },
            "/api/v1/files/{id}": {
                "delete": { "summary": "Delete a file", "responses": { "200": { "description": "Deleted" } } }
            },
            "/api/v1/pastes": {
                "post": { "summary": "Create a paste", "responses": { "200": { "description": "Created paste" } } }
            },
            "/api/v1/pastes/{id}": {
                "delete": { "summary": "Delete a paste", "responses": { "200": { "description": "Deleted" } } }
            },
            "/api/v1/me/files": {
                "get": { "summary": "List authenticated account files", "responses": { "200": { "description": "File list" } } }
            },
            "/api/v1/me/pastes": {
                "get": { "summary": "List authenticated account pastes", "responses": { "200": { "description": "Paste list" } } }
            },
            "/api/v1/claim/{kind}/{id}": {
                "post": { "summary": "Claim an anonymous file or paste with a delete token", "responses": { "200": { "description": "Claimed" } } }
            },
            "/api/v1/reports": {
                "post": { "summary": "Report a file or paste", "responses": { "200": { "description": "Report submitted" } } }
            },
            "/api/v1/tokens": {
                "get": { "summary": "List account API tokens", "responses": { "200": { "description": "Token list" } } },
                "post": { "summary": "Create an account API token", "responses": { "200": { "description": "Token created" } } }
            },
            "/api/v1/tokens/{id}": {
                "delete": { "summary": "Revoke an account API token", "responses": { "200": { "description": "Token revoked" } } }
            },
            "/api/v1/admin/reports": {
                "get": { "summary": "List moderation reports", "responses": { "200": { "description": "Report list" } } }
            },
            "/api/v1/admin/reports/{id}": {
                "patch": { "summary": "Update a moderation report", "responses": { "200": { "description": "Report updated" } } }
            },
            "/api/v1/admin/items/{kind}/{id}": {
                "patch": { "summary": "Update item moderation state, notes, or blocked hash", "responses": { "200": { "description": "Item updated" } } }
            },
            "/api/v1/admin/search": {
                "get": { "summary": "Search file and paste metadata as a moderator", "responses": { "200": { "description": "Search results" } } }
            }
        },
        "components": {
            "securitySchemes": {
                "bearer": { "type": "http", "scheme": "bearer" }
            }
        }
    }))
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiListQuery {
    q: Option<String>,
}

pub(super) async fn api_list_my_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ApiListQuery>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "files:read")
        .await?
        .ok_or(AppError::Unauthorized)?;
    enforce_rate_limit(&state, &settings, "api_list_files", &headers, Some(&user)).await?;
    let files = if let Some(q) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
        state.db.search_user_files(&user.id, q).await?
    } else {
        state.db.recent_user_files(&user.id).await?
    };
    let items = files
        .iter()
        .map(|file| api_file_item(&state, &settings, file))
        .collect::<Vec<_>>();
    Ok(axum::Json(serde_json::json!({ "items": items })))
}

pub(super) async fn api_list_my_pastes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ApiListQuery>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "pastes:read")
        .await?
        .ok_or(AppError::Unauthorized)?;
    enforce_rate_limit(&state, &settings, "api_list_pastes", &headers, Some(&user)).await?;
    let pastes = if let Some(q) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
        state
            .db
            .search_user_pastes(&user.id, q, settings.features.paste_content_search)
            .await?
    } else {
        state.db.recent_user_pastes(&user.id).await?
    };
    let items = pastes
        .iter()
        .map(|paste| api_paste_item(&state, paste))
        .collect::<Vec<_>>();
    Ok(axum::Json(serde_json::json!({ "items": items })))
}

pub(super) async fn api_upload_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> AppResult<axum::Json<ApiUploadResponse>> {
    let settings = state.settings().await?;
    let user = api_user(&state, &headers, "files:write").await?;
    enforce_rate_limit(
        &state,
        &settings,
        "api_upload_file",
        &headers,
        user.as_ref(),
    )
    .await?;
    if !settings.features.api || !policy::can_upload_file(&settings, user.as_ref()) {
        return Err(AppError::Forbidden);
    }
    let form = read_upload_form(&settings, multipart, settings.limits.max_upload_bytes).await?;
    let result = persist_file_upload(
        &state,
        &settings,
        user.as_ref(),
        form.file,
        parse_expiry_or_default_checked(
            &settings,
            user.as_ref(),
            "file",
            form.expires.as_deref(),
            settings.limits.default_file_expiry.as_deref(),
        )?,
        requested_visibility(&settings, form.visibility.as_deref())?,
    )
    .await?;
    Ok(axum::Json(ApiUploadResponse {
        url: result.url,
        raw_url: result.raw_url,
        internal_url: result.internal_url,
        delete_token: result.delete_token,
        id: result.file.public_id,
    }))
}

pub(super) async fn api_delete_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "files:delete").await?;
    enforce_rate_limit(
        &state,
        &settings,
        "api_delete_file",
        &headers,
        user.as_ref(),
    )
    .await?;
    let file = state
        .db
        .active_file_by_public_id(&id)
        .await?
        .ok_or(AppError::NotFound)?;
    let delete_token = headers.get("x-delete-token").and_then(|v| v.to_str().ok());
    authorize_file_delete(&settings, user.as_ref(), &file, delete_token)?;
    let deleted = state
        .db
        .delete_file(
            &file.id,
            user.as_ref().map(|user| user.id.as_str()),
            "api delete",
        )
        .await?;
    let remaining_refs = state.db.decrement_blob_ref(&deleted.blob_hash).await?;
    if remaining_refs == 0 {
        state.storage.delete_blob(&deleted.blob_hash).await?;
    }
    Ok(axum::Json(serde_json::json!({ "deleted": true })))
}

#[derive(Debug, Serialize)]
pub(super) struct ApiUploadResponse {
    id: String,
    url: String,
    raw_url: String,
    internal_url: Option<String>,
    delete_token: Option<String>,
}

fn api_file_item(
    state: &AppState,
    settings: &RuntimeSettings,
    file: &FileItem,
) -> serde_json::Value {
    let base = state.config.server.public_base_url.trim_end_matches('/');
    let slug = util::slug_with_extension(&file.public_id, file.extension.as_deref());
    let raw_url = format!("{base}/files/{}/raw", file.public_id);
    let metadata = file
        .metadata_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
    serde_json::json!({
        "id": file.public_id,
        "url": format!("{base}/{slug}"),
        "raw_url": raw_url.clone(),
        "internal_url": signed_internal_raw_url(state, settings, file),
        "thumbnail_url": file.thumbnail_hash.as_ref().map(|_| raw_url),
        "filename": file.original_filename,
        "content_type": file.content_type,
        "size_bytes": file.size_bytes,
        "image_width": file.image_width,
        "image_height": file.image_height,
        "visibility": file.visibility,
        "metadata": metadata,
        "expires_at": file.expires_at,
        "state": file.state,
        "created_at": file.created_at,
    })
}

fn api_paste_item(state: &AppState, paste: &Paste) -> serde_json::Value {
    let base = state.config.server.public_base_url.trim_end_matches('/');
    serde_json::json!({
        "id": paste.public_id,
        "url": format!("{base}/p/{}", paste.public_id),
        "raw_url": format!("{base}/p/{}/raw", paste.public_id),
        "title": paste.title,
        "syntax": paste.syntax,
        "size_bytes": paste.content.len(),
        "visibility": paste.visibility,
        "expires_at": paste.expires_at,
        "state": paste.state,
        "created_at": paste.created_at,
    })
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiPasteRequest {
    title: Option<String>,
    syntax: Option<String>,
    expires: Option<String>,
    visibility: Option<String>,
    content: String,
}

pub(super) async fn api_create_paste(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(input): axum::Json<ApiPasteRequest>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    let user = api_user(&state, &headers, "pastes:write").await?;
    enforce_rate_limit(
        &state,
        &settings,
        "api_create_paste",
        &headers,
        user.as_ref(),
    )
    .await?;
    if !settings.features.api || !policy::can_create_paste(&settings, user.as_ref()) {
        return Err(AppError::Forbidden);
    }
    if input.content.len() as i64 > settings.limits.max_paste_bytes {
        return Err(AppError::PayloadTooLarge);
    }
    let public_id = util::public_id();
    let delete_token = anonymous_delete_token(&settings, user.as_ref());
    let delete_hash = delete_token.as_deref().map(util::hash_token);
    let syntax = normalize_syntax(input.syntax.as_deref());
    state
        .db
        .create_paste(NewPaste {
            id: &uuid::Uuid::new_v4().to_string(),
            public_id: &public_id,
            title: input.title.as_deref(),
            content: &input.content,
            syntax: syntax.as_deref(),
            owner_user_id: user.as_ref().map(|u| u.id.as_str()),
            delete_token_hash: delete_hash.as_deref(),
            expires_at: parse_expiry_or_default_checked(
                &settings,
                user.as_ref(),
                "paste",
                input.expires.as_deref(),
                settings.limits.default_paste_expiry.as_deref(),
            )?,
            visibility: requested_visibility(&settings, input.visibility.as_deref())?,
        })
        .await?;
    state.metrics.pastes.inc();
    Ok(axum::Json(serde_json::json!({
        "id": public_id,
        "url": format!("{}/p/{public_id}", state.config.server.public_base_url.trim_end_matches('/')),
        "raw_url": format!("{}/p/{public_id}/raw", state.config.server.public_base_url.trim_end_matches('/')),
        "delete_token": delete_token,
    })))
}

pub(super) async fn api_delete_paste(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "pastes:delete").await?;
    enforce_rate_limit(
        &state,
        &settings,
        "api_delete_paste",
        &headers,
        user.as_ref(),
    )
    .await?;
    let paste = state
        .db
        .paste_by_public_id(&id)
        .await
        .map_err(|_| AppError::NotFound)?;
    let delete_token = headers.get("x-delete-token").and_then(|v| v.to_str().ok());
    authorize_paste_delete(&settings, user.as_ref(), &paste, delete_token)?;
    state
        .db
        .delete_paste(
            &paste.id,
            user.as_ref().map(|user| user.id.as_str()),
            "api delete",
        )
        .await?;
    Ok(axum::Json(serde_json::json!({ "deleted": true })))
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiReportRequest {
    kind: String,
    id: String,
    reason: String,
    details: Option<String>,
}

pub(super) async fn api_create_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(input): axum::Json<ApiReportRequest>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api || !settings.features.reports {
        return Err(AppError::Forbidden);
    }
    if input.kind != "file" && input.kind != "paste" {
        return Err(AppError::BadRequest(
            "kind must be file or paste".to_string(),
        ));
    }
    let user = api_user(&state, &headers, "reports:write").await?;
    enforce_rate_limit(
        &state,
        &settings,
        "api_create_report",
        &headers,
        user.as_ref(),
    )
    .await?;
    state
        .db
        .create_report(
            &input.kind,
            &input.id,
            user.as_ref().map(|user| user.id.as_str()),
            &input.reason,
            input.details.as_deref().unwrap_or(""),
        )
        .await?;
    state.metrics.reports.inc();

    if let Err(err) = trigger_moderation_webhook(
        &settings,
        &input.kind,
        &input.id,
        user.as_ref().map(|u| u.id.as_str()),
        &input.reason,
        input.details.as_deref().unwrap_or(""),
    )
    .await
    {
        tracing::error!(error = %err, "failed to trigger moderation webhook");
    }

    if let Some(abuse_email) = &settings.branding.abuse_email {
        state
            .mailer
            .send(
                abuse_email,
                "New Midden report",
                &format!(
                    "A report was submitted for {} {}.\n\nReason: {}\n\nDetails:\n{}",
                    input.kind,
                    input.id,
                    input.reason,
                    input.details.as_deref().unwrap_or("")
                ),
            )
            .await?;
    }
    Ok(axum::Json(serde_json::json!({ "reported": true })))
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiClaimRequest {
    delete_token: String,
}

pub(super) async fn api_claim_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((kind, id)): Path<(String, String)>,
    axum::Json(input): axum::Json<ApiClaimRequest>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "items:claim")
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !policy::allowed(settings.policy.claim_anonymous_item, Some(&user)) {
        return Err(AppError::Forbidden);
    }
    let token_hash = util::hash_token(input.delete_token.trim());
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
    Ok(axum::Json(serde_json::json!({ "claimed": true })))
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateTokenRequest {
    name: String,
    scopes: Vec<String>,
    expires_in_seconds: Option<i64>,
}

pub(super) async fn api_list_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "tokens:read")
        .await?
        .ok_or(AppError::Unauthorized)?;
    let tokens = state.db.list_api_tokens(&user.id).await?;
    Ok(axum::Json(serde_json::json!({ "items": tokens })))
}

pub(super) async fn api_create_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(input): axum::Json<CreateTokenRequest>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "tokens:write")
        .await?
        .ok_or(AppError::Unauthorized)?;
    enforce_rate_limit(&state, &settings, "api_create_token", &headers, Some(&user)).await?;
    let expires_at = api_token_expires_at(&settings, input.expires_in_seconds)?;
    let token = format!("mdd_{}", util::secret_token());
    state
        .db
        .create_api_token_with_expiry(
            &user.id,
            &input.name,
            &util::hash_token(&token),
            &input.scopes,
            expires_at,
        )
        .await?;
    state
        .db
        .audit(Some(&user.id), "api_token.created", &user.id, &input.name)
        .await?;
    Ok(axum::Json(
        serde_json::json!({ "token": token, "expires_at": expires_at }),
    ))
}

fn api_token_expires_at(
    settings: &RuntimeSettings,
    requested_ttl_seconds: Option<i64>,
) -> AppResult<Option<i64>> {
    let ttl = requested_ttl_seconds.or(settings.tokens.default_ttl_seconds);
    let Some(ttl) = ttl else {
        return Ok(None);
    };
    if ttl <= 0 {
        return Err(AppError::BadRequest(
            "token TTL must be positive".to_string(),
        ));
    }
    if let Some(max) = settings.tokens.max_ttl_seconds
        && ttl > max
    {
        return Err(AppError::BadRequest(
            "token TTL exceeds configured maximum".to_string(),
        ));
    }
    Ok(Some(util::now_ts().saturating_add(ttl)))
}

pub(super) async fn api_revoke_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "tokens:write")
        .await?
        .ok_or(AppError::Unauthorized)?;
    state.db.revoke_api_token(&user.id, &id).await?;
    state
        .db
        .audit(Some(&user.id), "api_token.revoked", &user.id, &id)
        .await?;
    Ok(axum::Json(serde_json::json!({ "revoked": true })))
}

pub(super) async fn api_admin_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminReportsQuery>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let _user = api_role_user(&state, &headers, "admin:reports", Role::Moderator).await?;
    let state_filter = query.state.as_deref().filter(|value| !value.is_empty());
    let kind_filter = query.kind.as_deref().filter(|value| !value.is_empty());
    let reason_filter = query
        .reason
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let created_after = query
        .days
        .filter(|days| *days > 0)
        .map(|days| util::now_ts() - days * 60 * 60 * 24);
    let reports = state
        .db
        .list_reports_filtered(state_filter, kind_filter, reason_filter, created_after)
        .await?;
    Ok(axum::Json(serde_json::json!({ "items": reports })))
}

pub(super) async fn api_admin_update_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(input): axum::Json<AdminReportActionForm>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let user = api_role_user(&state, &headers, "admin:reports", Role::Moderator).await?;
    let report = state.db.report_by_id(&id).await?;
    apply_report_action(
        &state,
        &report,
        &input.action,
        Some(&user),
        input.note.as_deref(),
    )
    .await?;
    Ok(axum::Json(serde_json::json!({ "updated": true })))
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiAdminItemUpdate {
    state: Option<String>,
    visibility: Option<String>,
    note: Option<String>,
    block_hash: Option<bool>,
}

pub(super) async fn api_admin_update_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((kind, id)): Path<(String, String)>,
    axum::Json(input): axum::Json<ApiAdminItemUpdate>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    let user = api_role_user(&state, &headers, "admin:items", Role::Moderator).await?;
    if let Some(item_state) = input.state.as_deref() {
        if !matches!(
            item_state,
            "active" | "quarantined" | "takedown" | "legal_hold" | "deleted"
        ) {
            return Err(AppError::BadRequest("invalid item state".to_string()));
        }
        update_item_state(&state, &kind, &id, item_state, Some(&user.id), "admin API").await?;
    }
    if input.visibility.is_some() {
        let visibility = requested_visibility(&settings, input.visibility.as_deref())?;
        update_item_visibility(&state, &kind, &id, visibility, Some(&user.id), "admin API").await?;
    }
    if let Some(note) = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty())
    {
        state
            .db
            .add_moderation_note(&kind, &id, None, Some(&user.id), note)
            .await?;
    }
    if input.block_hash.unwrap_or(false) {
        if kind != "file" {
            return Err(AppError::BadRequest(
                "blocked hashes can only be created from files".to_string(),
            ));
        }
        let file = state.db.file_by_public_id(&id).await?;
        let mut scanning = settings.scanning.clone();
        if !scanning
            .blocked_hashes
            .iter()
            .any(|hash| hash.eq_ignore_ascii_case(&file.blob_hash))
        {
            scanning.blocked_hashes.push(file.blob_hash.clone());
            state.db.set_json_setting("scanning", &scanning).await?;
        }
    }
    Ok(axum::Json(serde_json::json!({ "updated": true })))
}

pub(super) async fn api_admin_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminSearchQuery>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let _user = api_role_user(&state, &headers, "admin:search", Role::Moderator).await?;
    let settings = state.settings().await?;
    let q = query.q.unwrap_or_default();
    if q.trim().is_empty() {
        return Ok(axum::Json(serde_json::json!({ "files": [], "pastes": [] })));
    }
    let files = state
        .db
        .admin_search_files(&q)
        .await?
        .iter()
        .map(|file| api_file_item(&state, &settings, file))
        .collect::<Vec<_>>();
    let pastes = state
        .db
        .admin_search_pastes(&q, query.paste_content.unwrap_or(false))
        .await?
        .iter()
        .map(|paste| api_paste_item(&state, paste))
        .collect::<Vec<_>>();
    Ok(axum::Json(
        serde_json::json!({ "files": files, "pastes": pastes }),
    ))
}
