use super::admin::{AdminReportsQuery, AdminSearchQuery};
use super::*;
use crate::{
    commands,
    domain::{ItemKind, ItemModerationPlan, ItemState, ItemVisibility, ReportAction},
};

pub(super) async fn api_docs(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = current_user(&state, &jar).await?;
    render(
        &state,
        "docs.html",
        &settings,
        user.as_ref(),
        serde_json::json!({}),
    )
}

pub(super) async fn api_openapi(
    State(state): State<AppState>,
) -> AppResult<axum::Json<serde_json::Value>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    Ok(axum::Json(super::openapi::document()))
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiListQuery {
    q: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiItemsResponse<T> {
    items: Vec<T>,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiErrorResponse {
    error: ApiErrorDetail,
}

#[derive(Debug, Serialize)]
struct ApiErrorDetail {
    status: u16,
    code: String,
    message: String,
}

impl ApiErrorResponse {
    pub(super) fn new(status: StatusCode) -> Self {
        let message = status.canonical_reason().unwrap_or("error").to_string();
        Self {
            error: ApiErrorDetail {
                status: status.as_u16(),
                code: message.to_ascii_lowercase().replace(' ', "_"),
                message,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct ApiFileItem {
    id: String,
    url: String,
    raw_url: String,
    internal_url: Option<String>,
    thumbnail_url: Option<String>,
    filename: Option<String>,
    content_type: Option<String>,
    size_bytes: i64,
    image_width: Option<i64>,
    image_height: Option<i64>,
    visibility: String,
    metadata: Option<serde_json::Value>,
    expires_at: Option<i64>,
    state: String,
    created_at: i64,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiPasteItem {
    id: String,
    url: String,
    raw_url: String,
    title: Option<String>,
    syntax: Option<String>,
    size_bytes: usize,
    visibility: String,
    expires_at: Option<i64>,
    state: String,
    created_at: i64,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiDeletedResponse {
    deleted: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiClaimedResponse {
    claimed: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiRevokedResponse {
    revoked: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiUpdatedResponse {
    updated: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiPasteCreatedResponse {
    id: String,
    url: String,
    raw_url: String,
    delete_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiTokenCreatedResponse {
    token: String,
    expires_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiSearchResponse {
    files: Vec<ApiFileItem>,
    pastes: Vec<ApiPasteItem>,
}

pub(super) async fn api_list_my_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ApiListQuery>,
) -> AppResult<axum::Json<ApiItemsResponse<ApiFileItem>>> {
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
    Ok(axum::Json(ApiItemsResponse { items }))
}

pub(super) async fn api_list_my_pastes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ApiListQuery>,
) -> AppResult<axum::Json<ApiItemsResponse<ApiPasteItem>>> {
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
    Ok(axum::Json(ApiItemsResponse { items }))
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
) -> AppResult<axum::Json<ApiDeletedResponse>> {
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
    commands::delete_file(
        &state,
        &file,
        user.as_ref().map(|user| user.id.as_str()),
        "api delete",
    )
    .await?;
    Ok(axum::Json(ApiDeletedResponse { deleted: true }))
}

#[derive(Debug, Serialize)]
pub(super) struct ApiUploadResponse {
    id: String,
    url: String,
    raw_url: String,
    internal_url: Option<String>,
    delete_token: Option<String>,
}

fn api_file_item(state: &AppState, settings: &RuntimeSettings, file: &FileItem) -> ApiFileItem {
    let raw_url = raw_file_url(state, settings, file);
    let metadata = file
        .metadata_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
    ApiFileItem {
        id: file.public_id.clone(),
        url: file_url(state, settings, file),
        raw_url,
        internal_url: signed_internal_raw_url(state, settings, file),
        thumbnail_url: file
            .thumbnail_hash
            .as_ref()
            .map(|_| thumbnail_file_url(state, settings, file)),
        filename: file.original_filename.clone(),
        content_type: file.content_type.clone(),
        size_bytes: file.size_bytes,
        image_width: file.image_width,
        image_height: file.image_height,
        visibility: file.visibility.clone(),
        metadata,
        expires_at: file.expires_at,
        state: file.state.clone(),
        created_at: file.created_at,
    }
}

fn api_paste_item(state: &AppState, paste: &Paste) -> ApiPasteItem {
    let base = state.config.server.public_base_url.trim_end_matches('/');
    ApiPasteItem {
        id: paste.public_id.clone(),
        url: format!("{base}/p/{}", paste.public_id),
        raw_url: format!("{base}/p/{}/raw", paste.public_id),
        title: paste.title.clone(),
        syntax: paste.syntax.clone(),
        size_bytes: paste.content.len(),
        visibility: paste.visibility.clone(),
        expires_at: paste.expires_at,
        state: paste.state.clone(),
        created_at: paste.created_at,
    }
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
) -> AppResult<axum::Json<ApiPasteCreatedResponse>> {
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
    let expires_at = parse_expiry_or_default_checked(
        &settings,
        user.as_ref(),
        "paste",
        input.expires.as_deref(),
        settings.limits.default_paste_expiry.as_deref(),
    )?;
    let visibility = requested_visibility(&settings, input.visibility.as_deref())?;
    let created = commands::create_paste(
        &state,
        &settings,
        user.as_ref(),
        commands::CreatePasteInput {
            title: input.title.as_deref(),
            syntax: input.syntax.as_deref(),
            content: &input.content,
            expires_at,
            visibility,
        },
    )
    .await?;
    Ok(axum::Json(ApiPasteCreatedResponse {
        id: created.paste.public_id,
        url: created.url,
        raw_url: created.raw_url,
        delete_token: created.delete_token,
    }))
}

pub(super) async fn api_delete_paste(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<axum::Json<ApiDeletedResponse>> {
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
    commands::delete_paste(
        &state,
        &paste,
        user.as_ref().map(|user| user.id.as_str()),
        "api delete",
    )
    .await?;
    Ok(axum::Json(ApiDeletedResponse { deleted: true }))
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
) -> AppResult<axum::Json<commands::ReportCreated>> {
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
    let result = commands::create_report(
        &state,
        &settings,
        ItemKind::parse(&input.kind)?,
        &input.id,
        user.as_ref().map(|user| user.id.as_str()),
        &input.reason,
        input.details.as_deref().unwrap_or(""),
    )
    .await?;
    Ok(axum::Json(result))
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
) -> AppResult<axum::Json<ApiClaimedResponse>> {
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
    commands::claim_item(
        &state,
        ItemKind::parse(&kind)?,
        &id,
        &user.id,
        &input.delete_token,
    )
    .await?;
    Ok(axum::Json(ApiClaimedResponse { claimed: true }))
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
) -> AppResult<axum::Json<ApiItemsResponse<crate::db::ApiTokenSummary>>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "tokens:read")
        .await?
        .ok_or(AppError::Unauthorized)?;
    let tokens = state.db.list_api_tokens(&user.id).await?;
    Ok(axum::Json(ApiItemsResponse { items: tokens }))
}

pub(super) async fn api_create_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(input): axum::Json<CreateTokenRequest>,
) -> AppResult<axum::Json<ApiTokenCreatedResponse>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let actor = api_authenticated_user(&state, &headers, "tokens:write")
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !requested_scopes_allowed(&actor.scopes, &input.scopes) {
        return Err(AppError::Forbidden);
    }
    let user = actor.user;
    enforce_rate_limit(&state, &settings, "api_create_token", &headers, Some(&user)).await?;
    let created = commands::create_token(
        &state,
        &settings,
        &user,
        &input.name,
        &input.scopes,
        input.expires_in_seconds,
    )
    .await?;
    Ok(axum::Json(ApiTokenCreatedResponse {
        token: created.token,
        expires_at: created.expires_at,
    }))
}

fn requested_scopes_allowed(caller_scopes: &[String], requested_scopes: &[String]) -> bool {
    if caller_scopes.iter().any(|scope| scope == "*") {
        return true;
    }
    requested_scopes.iter().all(|requested| {
        requested != "*"
            && caller_scopes
                .iter()
                .any(|caller| caller.as_str() == requested.as_str())
    })
}

pub(super) async fn api_revoke_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<axum::Json<ApiRevokedResponse>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_user(&state, &headers, "tokens:write")
        .await?
        .ok_or(AppError::Unauthorized)?;
    commands::revoke_token(&state, &user, &id).await?;
    Ok(axum::Json(ApiRevokedResponse { revoked: true }))
}

pub(super) async fn api_admin_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminReportsQuery>,
) -> AppResult<axum::Json<ApiItemsResponse<crate::db::Report>>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    if !settings.features.reports {
        return Err(AppError::NotFound);
    }
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
    Ok(axum::Json(ApiItemsResponse { items: reports }))
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiReportActionRequest {
    action: String,
    note: Option<String>,
}

pub(super) async fn api_admin_update_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(input): axum::Json<ApiReportActionRequest>,
) -> AppResult<axum::Json<ApiUpdatedResponse>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    if !settings.features.reports {
        return Err(AppError::NotFound);
    }
    let user = api_role_user(&state, &headers, "admin:reports", Role::Moderator).await?;
    commands::moderate_reports(
        &state,
        std::slice::from_ref(&id),
        ReportAction::parse(&input.action)?,
        Some(&user.id),
        input.note.as_deref(),
    )
    .await?;
    Ok(axum::Json(ApiUpdatedResponse { updated: true }))
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
) -> AppResult<axum::Json<ApiUpdatedResponse>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = api_role_user(&state, &headers, "admin:items", Role::Moderator).await?;
    let item_kind = ItemKind::parse(&kind)?;
    let mut plan = ItemModerationPlan::new(item_kind, id.clone());
    if let Some(item_state) = input.state.as_deref() {
        plan.state = Some(ItemState::parse(item_state)?);
    }
    if input.visibility.is_some() {
        let visibility = requested_visibility(&settings, input.visibility.as_deref())?;
        plan.visibility = Some(ItemVisibility::parse(&settings, visibility)?);
    }
    plan.note = input.note;
    plan.block_hash = input.block_hash.unwrap_or(false);
    commands::moderate_item(&state, &settings, Some(&user.id), plan, "admin API").await?;
    Ok(axum::Json(ApiUpdatedResponse { updated: true }))
}

pub(super) async fn api_admin_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminSearchQuery>,
) -> AppResult<axum::Json<ApiSearchResponse>> {
    let settings = state.settings().await?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let _user = api_role_user(&state, &headers, "admin:search", Role::Moderator).await?;
    let q = query.q.unwrap_or_default();
    if q.trim().is_empty() {
        return Ok(axum::Json(ApiSearchResponse {
            files: Vec::new(),
            pastes: Vec::new(),
        }));
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
    Ok(axum::Json(ApiSearchResponse { files, pastes }))
}
