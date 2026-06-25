use super::*;

pub(super) async fn index(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    let page = serde_json::json!({
        "max_upload": util::human_bytes(settings.limits.max_upload_bytes),
        "delete_policy": format!("{:?}", settings.policy.delete_policy),
    });
    render(&state, "index.html", &settings, user.as_ref(), page)
}

pub(super) async fn upload_form_file(
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
    let page = serde_json::json!({
        "url": result.url,
        "raw_url": result.raw_url,
        "delete_token": result.delete_token,
        "file": result.file,
    });
    Ok(render(&state, "upload_result.html", &settings, user.as_ref(), page)?.into_response())
}

pub(super) async fn url_upload_form(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
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
pub(super) struct UrlUploadForm {
    url: String,
    expires: Option<String>,
    visibility: Option<String>,
    csrf_token: Option<String>,
}

pub(super) async fn url_upload(
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
    let page = serde_json::json!({
        "url": result.url,
        "raw_url": result.raw_url,
        "delete_token": result.delete_token,
        "file": result.file,
    });
    Ok(render(&state, "upload_result.html", &settings, user.as_ref(), page)?.into_response())
}

pub(super) async fn file_slug(
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
    authorize_item_view(
        &settings,
        user.as_ref(),
        file.owner_user_id.as_deref(),
        &file.visibility,
    )?;
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

pub(super) async fn raw_file(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    let file = state
        .db
        .active_file_by_public_id(&id)
        .await?
        .ok_or(AppError::NotFound)?;
    authorize_item_view(
        &settings,
        user.as_ref(),
        file.owner_user_id.as_deref(),
        &file.visibility,
    )?;
    serve_file(&state, file).await
}

#[derive(Debug, Deserialize)]
pub(super) struct InternalFileQuery {
    expires: i64,
    signature: String,
}

pub(super) async fn internal_raw_file(
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
    use futures_util::StreamExt;
    let stream = state
        .storage
        .get_blob_stream(&file.blob_hash)
        .await?
        .map(|res| res.map_err(axum::Error::new));
    let body = axum::body::Body::from_stream(stream);
    let content_type = file
        .content_type
        .as_deref()
        .unwrap_or("application/octet-stream")
        .parse::<HeaderValue>()
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from(file.size_bytes.max(0) as u64),
    );
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    if let Some(filename) = &file.original_filename {
        let forced_attachment = file.content_type.as_deref().is_some_and(|content_type| {
            settings
                .security
                .content_policy
                .forced_attachment_mime_types
                .iter()
                .any(|forced| forced.eq_ignore_ascii_case(content_type))
        });
        let disposition_kind = if forced_attachment {
            "attachment"
        } else {
            match settings.security.content_disposition {
                ContentDispositionMode::Inline => "inline",
                ContentDispositionMode::Attachment => "attachment",
            }
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
