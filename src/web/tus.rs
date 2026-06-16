use super::*;

pub(super) async fn tus_options(State(state): State<AppState>) -> AppResult<Response> {
    let settings = state.settings().await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert("Tus-Resumable", HeaderValue::from_static("1.0.0"));
    response
        .headers_mut()
        .insert("Tus-Version", HeaderValue::from_static("1.0.0"));
    response
        .headers_mut()
        .insert("Tus-Extension", HeaderValue::from_static("creation"));
    response.headers_mut().insert(
        "Tus-Max-Size",
        HeaderValue::from_str(&settings.limits.max_tus_upload_bytes.to_string()).unwrap(),
    );
    Ok(response)
}

pub(super) async fn tus_create(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    enforce_rate_limit(&state, &settings, "tus_create", &headers, user.as_ref()).await?;
    if !policy::can_use_api(&settings, user.as_ref())
        || !policy::can_upload_file(&settings, user.as_ref())
    {
        return Err(AppError::Forbidden);
    }
    let total = parse_i64_header(&headers, "Upload-Length")?;
    if total > settings.limits.max_tus_upload_bytes {
        return Err(AppError::PayloadTooLarge);
    }
    let upload_id = uuid::Uuid::new_v4().to_string();
    let metadata = parse_tus_metadata(&headers);
    let filename = metadata.get("filename").map(String::as_str);
    let content_type = metadata.get("content_type").map(String::as_str);
    let expires_at = parse_expiry_or_default(
        metadata.get("expires").map(String::as_str),
        settings.limits.default_file_expiry.as_deref(),
    )
    .map_err(|err| AppError::BadRequest(format!("invalid expiry: {err}")))?;
    let visibility =
        requested_visibility(&settings, metadata.get("visibility").map(String::as_str))?;
    state
        .db
        .start_upload_session(NewUploadSession {
            upload_id: &upload_id,
            filename,
            content_type,
            total_bytes: total,
            owner_user_id: user.as_ref().map(|u| u.id.as_str()),
            expires_at,
            visibility,
        })
        .await?;
    let mut response = StatusCode::CREATED.into_response();
    response
        .headers_mut()
        .insert("Tus-Resumable", HeaderValue::from_static("1.0.0"));
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&format!("/tus/{upload_id}")).unwrap(),
    );
    Ok(response)
}

pub(super) async fn tus_head(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    if !policy::can_use_api(&settings, user.as_ref())
        || !policy::can_upload_file(&settings, user.as_ref())
    {
        return Err(AppError::Forbidden);
    }
    let session = state
        .db
        .upload_session(&id)
        .await
        .map_err(|_| AppError::NotFound)?;
    authorize_tus_session_user(user.as_ref(), session.owner_user_id.as_deref())?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert("Tus-Resumable", HeaderValue::from_static("1.0.0"));
    response.headers_mut().insert(
        "Upload-Offset",
        HeaderValue::from_str(&session.received_bytes.to_string()).unwrap(),
    );
    response.headers_mut().insert(
        "Upload-Length",
        HeaderValue::from_str(&session.total_bytes.to_string()).unwrap(),
    );
    Ok(response)
}

pub(super) async fn tus_patch(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: axum::body::Body,
) -> AppResult<Response> {
    let settings = state.settings().await?;
    let user = current_user(&state, &jar).await?;
    enforce_rate_limit(&state, &settings, "tus_patch", &headers, user.as_ref()).await?;
    if !policy::can_use_api(&settings, user.as_ref())
        || !policy::can_upload_file(&settings, user.as_ref())
    {
        return Err(AppError::Forbidden);
    }
    let mut session = state
        .db
        .upload_session(&id)
        .await
        .map_err(|_| AppError::NotFound)?;
    authorize_tus_session_user(user.as_ref(), session.owner_user_id.as_deref())?;
    let session_owner = upload_session_owner(&state, &session).await?;
    if session.state != "open" {
        return Err(AppError::BadRequest("upload is not open".to_string()));
    }
    let expected_offset = parse_i64_header(&headers, "Upload-Offset")?;
    if expected_offset != session.received_bytes {
        return Err(AppError::BadRequest("upload offset mismatch".to_string()));
    }
    let chunk = body
        .collect()
        .await
        .map_err(|err| AppError::BadRequest(format!("invalid body: {err}")))?
        .to_bytes();
    let next_offset = session.received_bytes + chunk.len() as i64;
    if next_offset > session.total_bytes {
        return Err(AppError::PayloadTooLarge);
    }
    let temp_path = PathBuf::from(&session.temp_path);
    if let Some(parent) = temp_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&temp_path)
        .await?;
    file.write_all(&chunk).await?;
    file.flush().await?;
    state
        .db
        .update_upload_session_offset(&id, next_offset)
        .await?;
    session.received_bytes = next_offset;

    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert("Tus-Resumable", HeaderValue::from_static("1.0.0"));
    response.headers_mut().insert(
        "Upload-Offset",
        HeaderValue::from_str(&next_offset.to_string()).unwrap(),
    );

    if next_offset == session.total_bytes {
        let bytes = tokio::fs::read(&temp_path).await?;
        let uploaded = UploadedBytes {
            bytes: Bytes::from(bytes),
            filename: session.filename.clone(),
            content_type: session.content_type.clone(),
        };
        let completed = persist_file_upload(
            &state,
            &settings,
            session_owner.as_ref(),
            uploaded,
            session.expires_at,
            &session.visibility,
        )
        .await?;
        state.db.complete_upload_session(&id).await?;
        let _ = tokio::fs::remove_file(&temp_path).await;
        response.headers_mut().insert(
            header::LOCATION,
            HeaderValue::from_str(&completed.url).unwrap_or_else(|_| HeaderValue::from_static("/")),
        );
    }
    Ok(response)
}

fn authorize_tus_session_user(
    current_user: Option<&User>,
    owner_user_id: Option<&str>,
) -> AppResult<()> {
    let Some(owner_user_id) = owner_user_id else {
        return Ok(());
    };
    let Some(current_user) = current_user else {
        return Err(AppError::Forbidden);
    };
    if current_user.id == owner_user_id || current_user.role >= Role::Admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn upload_session_owner(
    state: &AppState,
    session: &crate::db::UploadSession,
) -> AppResult<Option<User>> {
    let Some(owner_user_id) = session.owner_user_id.as_deref() else {
        return Ok(None);
    };
    Ok(Some(state.db.user_by_id(owner_user_id).await?))
}
