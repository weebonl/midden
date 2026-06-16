use super::*;

#[derive(Debug)]
pub(super) struct UploadedBytes {
    pub(super) bytes: Bytes,
    pub(super) filename: Option<String>,
    pub(super) content_type: Option<String>,
}

#[derive(Debug)]
pub(super) struct FetchedUrlUpload {
    pub(super) bytes: Bytes,
    pub(super) content_type: Option<String>,
}

#[derive(Debug)]
pub(super) struct PersistedUpload {
    pub(super) file: FileItem,
    pub(super) url: String,
    pub(super) raw_url: String,
    pub(super) internal_url: Option<String>,
    pub(super) delete_token: Option<String>,
}

pub(super) struct UploadFormData {
    pub(super) file: UploadedBytes,
    pub(super) expires: Option<String>,
    pub(super) visibility: Option<String>,
    pub(super) csrf_token: Option<String>,
}

pub(super) async fn fetch_url_upload(
    settings: &RuntimeSettings,
    mut url: url::Url,
) -> AppResult<FetchedUrlUpload> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    for redirect_count in 0..=settings.security.url_upload.max_redirects {
        validate_url_upload_target(settings, &url).await?;
        let response = client.get(url.clone()).send().await?;
        if response.status().is_redirection() {
            if redirect_count == settings.security.url_upload.max_redirects {
                return Err(AppError::BadRequest(
                    "URL upload exceeded redirect limit".to_string(),
                ));
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| AppError::BadRequest("redirect missing Location".to_string()))?;
            url = url
                .join(location)
                .map_err(|err| AppError::BadRequest(format!("invalid redirect URL: {err}")))?;
            continue;
        }
        let response = response.error_for_status()?;
        if let Some(length) = response.content_length()
            && length > settings.limits.max_upload_bytes as u64
        {
            return Err(AppError::PayloadTooLarge);
        }
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let bytes = response.bytes().await?;
        if bytes.len() as i64 > settings.limits.max_upload_bytes {
            return Err(AppError::PayloadTooLarge);
        }
        return Ok(FetchedUrlUpload {
            bytes,
            content_type,
        });
    }
    Err(AppError::BadRequest("URL upload failed".to_string()))
}

async fn validate_url_upload_target(settings: &RuntimeSettings, url: &url::Url) -> AppResult<()> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::BadRequest(
            "only http and https URLs are supported".to_string(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadRequest("URL host is required".to_string()))?
        .to_ascii_lowercase();
    if settings
        .security
        .url_upload
        .blocked_hosts
        .iter()
        .any(|pattern| host_matches(&host, pattern))
    {
        return Err(AppError::BadRequest("URL host is blocked".to_string()));
    }
    if !settings.security.url_upload.allowed_hosts.is_empty()
        && !settings
            .security
            .url_upload
            .allowed_hosts
            .iter()
            .any(|pattern| host_matches(&host, pattern))
    {
        return Err(AppError::BadRequest("URL host is not allowed".to_string()));
    }
    if settings.security.url_upload.block_private_ips {
        match url.host() {
            Some(url::Host::Ipv4(ip)) => reject_private_ip(IpAddr::V4(ip))?,
            Some(url::Host::Ipv6(ip)) => reject_private_ip(IpAddr::V6(ip))?,
            Some(url::Host::Domain(domain)) => {
                let port = url.port_or_known_default().unwrap_or(80);
                let mut addrs = tokio::net::lookup_host((domain, port)).await?;
                let mut saw_addr = false;
                for addr in addrs.by_ref() {
                    saw_addr = true;
                    reject_private_ip(addr.ip())?;
                }
                if !saw_addr {
                    return Err(AppError::BadRequest("URL host did not resolve".to_string()));
                }
            }
            None => return Err(AppError::BadRequest("URL host is required".to_string())),
        }
    }
    Ok(())
}

fn reject_private_ip(ip: IpAddr) -> AppResult<()> {
    if is_private_upload_ip(ip) {
        Err(AppError::BadRequest(
            "URL resolved to a private or local address".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn is_private_upload_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
        }
    }
}

fn host_matches(host: &str, pattern: &str) -> bool {
    let pattern = pattern.trim().trim_end_matches('.').to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    if pattern.is_empty() {
        return false;
    }
    host == pattern
        || (pattern.starts_with('.') && host.ends_with(&pattern))
        || host.ends_with(&format!(".{pattern}"))
}

fn resolved_content_type(
    settings: &RuntimeSettings,
    uploaded: &UploadedBytes,
) -> AppResult<String> {
    let declared = uploaded.content_type.as_deref().and_then(clean_mime);
    let extension_guess = uploaded.filename.as_deref().and_then(|name| {
        mime_guess::from_path(name)
            .first()
            .map(|mime| mime.essence_str().to_string())
    });
    let sniffed = processing::sniff_mime(&uploaded.bytes).map(ToOwned::to_owned);

    if settings.security.reject_mime_mismatch {
        if let (Some(sniffed), Some(declared)) = (sniffed.as_deref(), declared.as_deref()) {
            reject_mime_mismatch(sniffed, declared, "declared content type")?;
        }
        if let (Some(sniffed), Some(extension_guess)) =
            (sniffed.as_deref(), extension_guess.as_deref())
        {
            reject_mime_mismatch(sniffed, extension_guess, "filename extension")?;
        }
    }

    Ok(sniffed
        .or(declared)
        .or(extension_guess)
        .unwrap_or_else(|| "application/octet-stream".to_string()))
}

fn clean_mime(value: &str) -> Option<String> {
    value
        .parse::<mime::Mime>()
        .ok()
        .map(|mime| mime.essence_str().to_ascii_lowercase())
}

fn reject_mime_mismatch(sniffed: &str, candidate: &str, source: &str) -> AppResult<()> {
    if candidate == "application/octet-stream" || sniffed == "application/octet-stream" {
        return Ok(());
    }
    if sniffed != candidate {
        return Err(AppError::BadRequest(format!(
            "detected MIME {sniffed} does not match {source} {candidate}"
        )));
    }
    Ok(())
}

pub(super) async fn read_upload_form(
    mut multipart: Multipart,
    max_bytes: i64,
) -> AppResult<UploadFormData> {
    let mut file = None;
    let mut expires = None;
    let mut visibility = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::BadRequest(format!("invalid multipart body: {err}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "expires" {
            expires = Some(
                field
                    .text()
                    .await
                    .map_err(|err| AppError::BadRequest(format!("invalid expiry field: {err}")))?,
            );
        } else if name == "visibility" {
            visibility =
                Some(field.text().await.map_err(|err| {
                    AppError::BadRequest(format!("invalid visibility field: {err}"))
                })?);
        } else if name == CSRF_FIELD {
            let token = field
                .text()
                .await
                .map_err(|err| AppError::BadRequest(format!("invalid CSRF field: {err}")))?;
            return read_upload_form_after_csrf(
                multipart,
                max_bytes,
                file,
                expires,
                visibility,
                Some(token),
            )
            .await;
        } else if name == "file" {
            file = Some(read_upload_field_to_temp(field, max_bytes).await?);
        }
    }
    Ok(UploadFormData {
        file: file.ok_or_else(|| AppError::BadRequest("missing file field".to_string()))?,
        expires,
        visibility,
        csrf_token: None,
    })
}

async fn read_upload_form_after_csrf(
    mut multipart: Multipart,
    max_bytes: i64,
    mut file: Option<UploadedBytes>,
    mut expires: Option<String>,
    mut visibility: Option<String>,
    csrf_token: Option<String>,
) -> AppResult<UploadFormData> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::BadRequest(format!("invalid multipart body: {err}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "expires" {
            expires = Some(
                field
                    .text()
                    .await
                    .map_err(|err| AppError::BadRequest(format!("invalid expiry field: {err}")))?,
            );
        } else if name == "visibility" {
            visibility =
                Some(field.text().await.map_err(|err| {
                    AppError::BadRequest(format!("invalid visibility field: {err}"))
                })?);
        } else if name == "file" {
            file = Some(read_upload_field_to_temp(field, max_bytes).await?);
        }
    }
    Ok(UploadFormData {
        file: file.ok_or_else(|| AppError::BadRequest("missing file field".to_string()))?,
        expires,
        visibility,
        csrf_token,
    })
}

async fn read_upload_field_to_temp(
    mut field: axum::extract::multipart::Field<'_>,
    max_bytes: i64,
) -> AppResult<UploadedBytes> {
    let filename = field.file_name().map(ToOwned::to_owned);
    let content_type = field.content_type().map(ToString::to_string);
    let temp_path =
        std::env::temp_dir().join(format!("midden-upload-{}.part", uuid::Uuid::new_v4()));
    let mut temp = tokio::fs::File::create(&temp_path).await?;
    let mut size = 0_i64;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|err| AppError::BadRequest(format!("invalid upload field: {err}")))?
    {
        size += chunk.len() as i64;
        if size > max_bytes {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(AppError::PayloadTooLarge);
        }
        temp.write_all(&chunk).await?;
    }
    temp.flush().await?;
    drop(temp);
    if size == 0 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(AppError::BadRequest(
            "empty files are not accepted".to_string(),
        ));
    }
    let bytes = tokio::fs::read(&temp_path).await?;
    let _ = tokio::fs::remove_file(&temp_path).await;
    Ok(UploadedBytes {
        bytes: Bytes::from(bytes),
        filename,
        content_type,
    })
}

pub(super) async fn persist_file_upload(
    state: &AppState,
    settings: &RuntimeSettings,
    user: Option<&User>,
    mut uploaded: UploadedBytes,
    expires_at: Option<i64>,
    visibility: &str,
) -> AppResult<PersistedUpload> {
    let content_type = resolved_content_type(settings, &uploaded)?;
    if settings.processing.metadata_stripping {
        uploaded.bytes = processing::strip_file_metadata(&content_type, uploaded.bytes);
    }
    let size_bytes = uploaded.bytes.len() as i64;
    quota::enforce_file_upload_quota(&state.db, settings, user, size_bytes).await?;
    let hash = util::sha256_hex_bytes(&uploaded.bytes);
    if settings
        .scanning
        .blocked_hashes
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(&hash))
    {
        return Err(AppError::BadRequest("file hash is blocked".to_string()));
    }
    if settings
        .scanning
        .blocked_mime_types
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(&content_type))
    {
        return Err(AppError::BadRequest("file type is blocked".to_string()));
    }
    let image_dimensions = util::image_dimensions(&uploaded.bytes);
    let metadata_json = if settings.processing.metadata_extraction {
        Some(processing::file_metadata_json(
            &content_type,
            size_bytes,
            image_dimensions,
            settings.processing.metadata_stripping,
        )?)
    } else {
        None
    };
    let extension = util::normalize_extension(uploaded.filename.as_deref(), Some(&content_type));
    let public_id = util::public_id();
    let scan = scanner::scan_upload(
        &settings.scanning,
        ScanInput {
            bytes: &uploaded.bytes,
            filename: uploaded.filename.as_deref(),
            content_type: Some(&content_type),
            hash: &hash,
            public_id: &public_id,
        },
    )
    .await;
    state
        .metrics
        .record_scanner_outcome(&format!("{:?}", scan.decision).to_lowercase());
    if matches!(scan.decision, crate::config::ScanDecision::Reject) {
        return Err(AppError::BadRequest(
            "upload rejected by scanner".to_string(),
        ));
    }
    let file_state = if matches!(scan.decision, crate::config::ScanDecision::Quarantine) {
        "quarantined"
    } else {
        "active"
    };
    let delete_token = anonymous_delete_token(settings, user);
    let delete_token_hash = delete_token.as_deref().map(util::hash_token);
    state
        .db
        .create_blob_if_missing(&hash, uploaded.bytes.len() as i64, Some(&content_type))
        .await?;
    if !state.storage.exists(&hash).await? {
        state.storage.put_blob(&hash, uploaded.bytes).await?;
    }
    let file = state
        .db
        .create_file_item(NewFileItem {
            id: &uuid::Uuid::new_v4().to_string(),
            public_id: &public_id,
            blob_hash: &hash,
            original_filename: uploaded.filename.as_deref(),
            extension: extension.as_deref(),
            content_type: Some(&content_type),
            size_bytes,
            image_width: image_dimensions.map(|(width, _)| width),
            image_height: image_dimensions.map(|(_, height)| height),
            owner_user_id: user.map(|user| user.id.as_str()),
            delete_token_hash: delete_token_hash.as_deref(),
            expires_at,
            visibility,
            metadata_json: metadata_json.as_deref(),
            thumbnail_hash: None,
            state: file_state,
        })
        .await?;
    for report in &scan.reports {
        state
            .db
            .record_scan_result(
                "file",
                &file.public_id,
                &report.adapter,
                &format!("{:?}", report.decision).to_lowercase(),
                &report.detail,
            )
            .await?;
    }
    if file_state == "quarantined" {
        return Err(AppError::BadRequest(
            "upload quarantined by scanner".to_string(),
        ));
    }
    let slug = util::slug_with_extension(&file.public_id, file.extension.as_deref());
    let base = state.config.server.public_base_url.trim_end_matches('/');
    state.metrics.uploads.inc();
    state.metrics.upload_bytes.inc_by(size_bytes as u64);
    let internal_url = signed_internal_raw_url(state, settings, &file);
    Ok(PersistedUpload {
        raw_url: format!("{base}/files/{}/raw", file.public_id),
        url: format!("{base}/{slug}"),
        internal_url,
        delete_token,
        file,
    })
}
