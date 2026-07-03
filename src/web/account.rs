use super::*;
use axum_extra::extract::Form as HtmlForm;
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
pub(super) struct AccountQuery {
    q: Option<String>,
}

pub(super) async fn account(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    ensure_accounts_enabled(&settings)?;
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let oidc_link_enabled = oidc::enabled(&state, &settings);
    let q = query.q.unwrap_or_default();
    let (files, pastes) = user_items_for_query(&state, &settings, &user, &q).await?;
    let page = if htmx_request(&headers) {
        serde_json::json!({
            "q": q,
            "files": files,
            "pastes": pastes,
        })
    } else {
        let tokens = state.db.list_api_tokens(&user.id).await?;
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
        })
    };
    render(
        &state,
        if htmx_request(&headers) {
            "account_items.html"
        } else {
            "account.html"
        },
        &settings,
        Some(&user),
        page,
    )
}

#[derive(Debug, Deserialize)]
pub(super) struct AccountTokenForm {
    name: String,
    scopes: String,
    expires_in_seconds: Option<String>,
    csrf_token: Option<String>,
}

pub(super) async fn account_create_token(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    axum::Form(form): axum::Form<AccountTokenForm>,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    ensure_accounts_enabled(&settings)?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
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
    let expires_at = account_token_expires_at(&settings, form.expires_in_seconds.as_deref())?;
    let token = format!("mdd_{}", util::secret_token());
    state
        .db
        .create_api_token_with_expiry(
            &user.id,
            &form.name,
            &util::hash_token(&token),
            &scopes,
            expires_at,
        )
        .await?;
    state
        .db
        .audit(Some(&user.id), "api_token.created", &user.id, &form.name)
        .await?;
    let tokens = state.db.list_api_tokens(&user.id).await?;
    if htmx_request(&headers) {
        return Ok(render(
            &state,
            "account_tokens.html",
            &settings,
            Some(&user),
            serde_json::json!({
                "tokens": tokens,
                "new_token": token,
            }),
        )?
        .into_response());
    }
    let files = state.db.recent_user_files(&user.id).await?;
    let pastes = state.db.recent_user_pastes(&user.id).await?;
    Ok(render(
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
    )?
    .into_response())
}

fn account_token_expires_at(
    settings: &RuntimeSettings,
    requested_ttl_seconds: Option<&str>,
) -> AppResult<Option<i64>> {
    let requested = requested_ttl_seconds
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|err| AppError::BadRequest(format!("invalid token TTL seconds: {err}")))
        })
        .transpose()?;
    let ttl = requested.or(settings.tokens.default_ttl_seconds);
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

pub(super) async fn account_revoke_token(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Form(form): axum::Form<CsrfForm>,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    ensure_accounts_enabled(&settings)?;
    if !settings.features.api {
        return Err(AppError::Forbidden);
    }
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    state.db.revoke_api_token(&user.id, &id).await?;
    state
        .db
        .audit(Some(&user.id), "api_token.revoked", &user.id, &id)
        .await?;
    if htmx_request(&headers) {
        let tokens = state.db.list_api_tokens(&user.id).await?;
        Ok(render(
            &state,
            "account_tokens.html",
            &settings,
            Some(&user),
            serde_json::json!({
                "tokens": tokens,
                "new_token": null,
            }),
        )?
        .into_response())
    } else {
        Ok(Redirect::to("/account").into_response())
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct AccountBulkItemsForm {
    bulk_action: String,
    file_ids: Option<Vec<String>>,
    paste_ids: Option<Vec<String>>,
    visibility: Option<String>,
    expires: Option<String>,
    csrf_token: Option<String>,
}

pub(super) async fn account_bulk_items(
    State(state): State<AppState>,
    jar: CookieJar,
    HtmlForm(form): HtmlForm<AccountBulkItemsForm>,
) -> AppResult<Redirect> {
    let settings = state.settings().await?;
    ensure_accounts_enabled(&settings)?;
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    let file_ids = form
        .file_ids
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let paste_ids = form
        .paste_ids
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeSet<_>>();
    match form.bulk_action.as_str() {
        "delete" => {
            for id in &file_ids {
                let file = state.db.file_by_public_id(id).await?;
                authorize_file_delete(&settings, Some(&user), &file, None)?;
                let deleted = state
                    .db
                    .delete_file(&file.id, Some(&user.id), "account bulk delete")
                    .await?;
                if deleted.state == "active" {
                    let remaining_refs = state.db.decrement_blob_ref(&deleted.blob_hash).await?;
                    if remaining_refs == 0 {
                        state.storage.delete_blob(&deleted.blob_hash).await?;
                    }
                }
            }
            for id in &paste_ids {
                let paste = state.db.paste_by_public_id_any(id).await?;
                authorize_paste_delete(&settings, Some(&user), &paste, None)?;
                state
                    .db
                    .delete_paste(&paste.id, Some(&user.id), "account bulk delete")
                    .await?;
            }
        }
        "set_visibility" => {
            let visibility = requested_visibility(&settings, form.visibility.as_deref())?;
            for id in &file_ids {
                let file = state.db.file_by_public_id(id).await?;
                if file.owner_user_id.as_deref() == Some(user.id.as_str()) {
                    state.db.set_file_visibility(id, visibility).await?;
                }
            }
            for id in &paste_ids {
                let paste = state.db.paste_by_public_id_any(id).await?;
                if paste.owner_user_id.as_deref() == Some(user.id.as_str()) {
                    state.db.set_paste_visibility(id, visibility).await?;
                }
            }
        }
        "set_expiry" => {
            let expires_at = parse_expiry_or_default_checked(
                &settings,
                Some(&user),
                "file",
                form.expires.as_deref(),
                None,
            )?;
            for id in &file_ids {
                state.db.set_file_expiry(id, &user.id, expires_at).await?;
            }
            let paste_expires_at = parse_expiry_or_default_checked(
                &settings,
                Some(&user),
                "paste",
                form.expires.as_deref(),
                None,
            )?;
            for id in &paste_ids {
                state
                    .db
                    .set_paste_expiry(id, &user.id, paste_expires_at)
                    .await?;
            }
        }
        _ => return Err(AppError::BadRequest("unknown bulk action".to_string())),
    }
    state
        .db
        .audit(
            Some(&user.id),
            "account.bulk_items",
            &user.id,
            &form.bulk_action,
        )
        .await?;
    Ok(Redirect::to("/account"))
}

pub(super) async fn account_send_email_verification(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<CsrfForm>,
) -> AppResult<Redirect> {
    let settings = state.settings().await?;
    ensure_accounts_enabled(&settings)?;
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
pub(super) struct AccountPasswordForm {
    current_password: String,
    new_password: String,
    csrf_token: Option<String>,
}

pub(super) async fn account_change_password(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<AccountPasswordForm>,
) -> AppResult<Redirect> {
    let settings = state.settings().await?;
    ensure_local_accounts_enabled(&settings)?;
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
pub(super) struct AccountTwoFactorForm {
    current_password: String,
    csrf_token: Option<String>,
}

pub(super) async fn account_enable_two_factor(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<AccountTwoFactorForm>,
) -> AppResult<Redirect> {
    let settings = state.settings().await?;
    ensure_local_accounts_enabled(&settings)?;
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

pub(super) async fn account_disable_two_factor(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<AccountTwoFactorForm>,
) -> AppResult<Redirect> {
    let settings = state.settings().await?;
    ensure_local_accounts_enabled(&settings)?;
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

pub(super) async fn account_deactivate(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(form): axum::Form<CsrfForm>,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    ensure_accounts_enabled(&settings)?;
    let user = current_user(&state, &jar)
        .await?
        .ok_or(AppError::Unauthorized)?;
    validate_csrf(&jar, form.csrf_token.as_deref())?;
    state.db.set_user_disabled(&user.id, true).await?;
    state
        .db
        .audit(Some(&user.id), "user.deactivated", &user.id, "account UI")
        .await?;
    let secure_cookies = settings.security.secure_cookies;
    let cookie = session_cookie(&state, String::new(), Some(0), secure_cookies);
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
