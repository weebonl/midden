use super::*;

pub(super) async fn new_paste(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
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
pub(super) struct PasteForm {
    title: Option<String>,
    syntax: Option<String>,
    expires: Option<String>,
    visibility: Option<String>,
    content: String,
    csrf_token: Option<String>,
}

pub(super) async fn create_paste(
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

pub(super) async fn show_paste(
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

pub(super) async fn edit_paste_form(
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
pub(super) struct PasteEditForm {
    title: Option<String>,
    syntax: Option<String>,
    content: String,
    csrf_token: Option<String>,
}

pub(super) async fn update_paste(
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

pub(super) async fn raw_paste(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Response> {
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

fn can_edit_paste(settings: &RuntimeSettings, user: Option<&User>, paste: &Paste) -> bool {
    if !settings.features.paste_editing {
        return false;
    }
    let Some(user) = user else {
        return false;
    };
    user.role >= Role::Admin || paste.owner_user_id.as_deref() == Some(user.id.as_str())
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
