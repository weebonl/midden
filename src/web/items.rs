use super::*;
use crate::{commands, domain::ItemKind};

pub(super) fn render_unavailable_item(
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

pub(super) async fn report_form(
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
pub(super) struct ReportForm {
    reason: String,
    details: Option<String>,
    csrf_token: Option<String>,
}

pub(super) async fn create_report(
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
    commands::create_report(
        &state,
        &settings,
        ItemKind::parse(&kind)?,
        &id,
        user.as_ref().map(|user| user.id.as_str()),
        &form.reason,
        form.details.as_deref().unwrap_or(""),
    )
    .await?;
    Ok(Redirect::to("/"))
}

pub(super) async fn delete_form(
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
pub(super) struct DeleteForm {
    token: Option<String>,
    csrf_token: Option<String>,
}

pub(super) async fn delete_item(
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
            commands::delete_file(
                &state,
                &file,
                user.as_ref().map(|user| user.id.as_str()),
                "web delete",
            )
            .await?;
        }
        "paste" => {
            let paste = state
                .db
                .paste_by_public_id(&id)
                .await
                .map_err(|_| AppError::NotFound)?;
            authorize_paste_delete(&settings, user.as_ref(), &paste, form.token.as_deref())?;
            commands::delete_paste(
                &state,
                &paste,
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

pub(super) async fn claim_form(
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
pub(super) struct ClaimForm {
    token: String,
    csrf_token: Option<String>,
}

pub(super) async fn claim_item(
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
    commands::claim_item(&state, ItemKind::parse(&kind)?, &id, &user.id, &form.token).await?;
    Ok(Redirect::to("/account"))
}

pub(super) fn anonymous_delete_token(
    settings: &RuntimeSettings,
    user: Option<&User>,
) -> Option<String> {
    if user.is_some() {
        return None;
    }
    match settings.policy.delete_policy {
        DeletePolicy::DeleteTokens | DeletePolicy::ClaimLater => Some(util::secret_token()),
        DeletePolicy::Disabled | DeletePolicy::NoAnonymousDelete => None,
    }
}

pub(super) fn authorize_file_delete(
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

pub(super) fn authorize_paste_delete(
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
