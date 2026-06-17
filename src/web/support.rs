use super::*;

pub(super) enum CacheScope {
    Public,
}

pub(super) fn insert_cache_control(response: &mut Response, seconds: u64, _scope: CacheScope) {
    let value = if seconds == 0 {
        HeaderValue::from_static("no-store")
    } else {
        HeaderValue::from_str(&format!("public, max-age={seconds}"))
            .unwrap_or_else(|_| HeaderValue::from_static("public, max-age=3600"))
    };
    response.headers_mut().insert(header::CACHE_CONTROL, value);
}

pub(super) fn signed_internal_raw_url(
    state: &AppState,
    settings: &RuntimeSettings,
    file: &FileItem,
) -> Option<String> {
    if !settings.delivery.signed_internal_urls {
        return None;
    }
    let secret = settings
        .delivery
        .internal_url_secret
        .as_deref()
        .filter(|secret| !secret.is_empty())?;
    let expires = util::now_ts() + settings.delivery.internal_url_ttl_seconds.max(1);
    let signature = sign_internal_file_url(secret, &file.public_id, expires);
    let base = state.config.server.public_base_url.trim_end_matches('/');
    Some(format!(
        "{base}/internal/files/{}/raw?expires={expires}&signature={signature}",
        file.public_id
    ))
}

pub(super) fn sign_internal_file_url(secret: &str, public_id: &str, expires: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update([0]);
    hasher.update(public_id.as_bytes());
    hasher.update([0]);
    hasher.update(expires.to_string().as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

pub(super) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut diff = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        diff |= (a ^ b) as usize;
    }
    diff == 0
}

pub(super) fn render<S: Serialize>(
    state: &AppState,
    name: &str,
    settings: &RuntimeSettings,
    current_user: Option<&User>,
    page: S,
) -> AppResult<Html<String>> {
    Ok(Html(state.templates.render(
        name,
        settings,
        current_user,
        page,
    )?))
}

pub(super) fn htmx_request(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Request")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

pub(super) async fn current_user(state: &AppState, jar: &CookieJar) -> AppResult<Option<User>> {
    let Some(cookie) = jar.get(&state.config.security.session_cookie_name) else {
        return Ok(None);
    };
    Ok(state
        .db
        .user_by_session_token(&util::hash_token(cookie.value()))
        .await?)
}

pub(super) fn validate_csrf(jar: &CookieJar, submitted: Option<&str>) -> AppResult<()> {
    let expected = jar
        .get(CSRF_COOKIE)
        .map(|cookie| cookie.value())
        .ok_or_else(|| AppError::BadRequest("missing CSRF cookie".to_string()))?;
    let submitted = submitted
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing CSRF token".to_string()))?;
    if submitted == expected {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct CsrfForm {
    pub(super) csrf_token: Option<String>,
}

pub(super) async fn enforce_rate_limit(
    state: &AppState,
    settings: &RuntimeSettings,
    action: &str,
    headers: &HeaderMap,
    user: Option<&User>,
) -> AppResult<()> {
    let identity = rate_limit_identity(state, headers, user);
    let result = state
        .rate_limiter
        .check(action, &identity, settings.security.rate_limits.get(action))
        .await;
    if matches!(result, Err(AppError::TooManyRequests)) {
        state.metrics.rate_limit_rejections.inc();
    }
    result
}

fn rate_limit_identity(state: &AppState, headers: &HeaderMap, user: Option<&User>) -> String {
    if let Some(user) = user {
        return format!("user:{}", user.id);
    }
    if state.config.server.behind_proxy
        && let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        return format!("ip:{forwarded}");
    }
    if let Some(real_ip) = headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
    {
        return format!("ip:{real_ip}");
    }
    "anonymous".to_string()
}

pub(super) async fn api_user(
    state: &AppState,
    headers: &HeaderMap,
    required_scope: &str,
) -> AppResult<Option<User>> {
    let settings = state.settings().await?;
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if let Some(token) = bearer {
        let user = state
            .db
            .user_by_api_token(&util::hash_token(token), required_scope)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if policy::can_use_api(&settings, Some(&user)) {
            return Ok(Some(user));
        }
        return Err(AppError::Forbidden);
    }
    if policy::can_use_api(&settings, None) {
        Ok(None)
    } else {
        Err(AppError::Unauthorized)
    }
}

pub(super) async fn api_role_user(
    state: &AppState,
    headers: &HeaderMap,
    required_scope: &str,
    minimum_role: Role,
) -> AppResult<User> {
    let user = api_user(state, headers, required_scope)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if user.role >= minimum_role {
        Ok(user)
    } else {
        Err(AppError::Forbidden)
    }
}

pub(super) fn session_cookie(
    state: &AppState,
    token: String,
    max_age_seconds: Option<i64>,
) -> Cookie<'static> {
    let mut cookie = Cookie::new(state.config.security.session_cookie_name.clone(), token);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(state.config.security.secure_cookies);
    if let Some(seconds) = max_age_seconds {
        cookie.set_max_age(time::Duration::seconds(seconds));
    }
    cookie
}

pub(super) fn transient_cookie(name: &'static str, value: String) -> Cookie<'static> {
    let mut cookie = Cookie::new(name, value);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_max_age(time::Duration::minutes(10));
    cookie
}

pub(super) fn parse_scopes(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn requested_visibility(
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

pub(super) fn parse_expiry_or_default(
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

pub(super) fn normalize_syntax(input: Option<&str>) -> Option<String> {
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
