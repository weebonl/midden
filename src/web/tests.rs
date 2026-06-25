use super::*;
use crate::jobs;
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
        .create_api_token_with_expiry(&user.id, "test", &util::hash_token(&token), &scopes, None)
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

fn csrf_cookie_from(headers: &reqwest::header::HeaderMap) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .find_map(|cookie| cookie.strip_prefix("midden_csrf=").map(ToOwned::to_owned))
        .unwrap()
}

async fn response_body(response: Response) -> String {
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

async fn admin_session_state() -> (AppState, String) {
    let issuer = spawn_oidc_provider(serde_json::json!({
        "sub": "unused-ui-admin",
        "email": "unused-ui-admin@example.test",
        "groups": ["admins"]
    }))
    .await;
    let state = test_state(issuer).await;
    let admin = state
        .db
        .create_user(
            "ui-admin@example.test",
            "ui-admin",
            Some("password-hash"),
            Role::Admin,
        )
        .await
        .unwrap();
    let session_token = util::secret_token();
    state
        .db
        .create_session(
            &admin.id,
            &util::hash_token(&session_token),
            util::now_ts() + 60,
        )
        .await
        .unwrap();
    (state, session_token)
}

#[tokio::test]
async fn admin_ui_exposes_wide_shell_and_settings_affordances() {
    let (state, session_token) = admin_session_state().await;
    let response = state
        .router()
        .oneshot(
            Request::builder()
                .uri("/admin")
                .header(header::COOKIE, format!("midden_session={session_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body(response).await;
    assert!(body.contains("class=\"page page--wide page--admin\""));
    assert!(body.contains("class=\"admin-nav\""));
    assert!(body.contains("data-admin-settings-form"));
    assert!(body.contains("class=\"settings-tabs\""));
    assert!(body.contains("class=\"settings-save-bar\""));
    assert!(body.contains("data-settings-section=\"features\""));
    assert!(body.contains("data-secret-input"));
    assert!(body.contains("data-accent-preview"));
    assert!(body.contains("x-cloak"));
}

#[tokio::test]
async fn static_assets_include_frontend_ux_helpers() {
    let issuer = spawn_oidc_provider(serde_json::json!({
        "sub": "unused-static-ui",
        "email": "unused-static-ui@example.test",
        "groups": ["admins"]
    }))
    .await;
    let state = test_state(issuer).await;
    let router = state.router();

    let css_response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/static/midden.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(css_response.status(), StatusCode::OK);
    let css = response_body(css_response).await;
    assert!(css.contains(".page--wide"));
    assert!(css.contains("[x-cloak]"));
    assert!(css.contains(".status-badge"));
    assert!(css.contains(".table-card"));
    assert!(css.contains(".htmx-request"));
    assert!(css.contains(".drop-zone:focus-within"));
    assert!(css.contains("input[type=\"checkbox\"]"));
    assert!(css.contains("inline-size: auto"));

    let js_response = router
        .oneshot(
            Request::builder()
                .uri("/static/midden.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(js_response.status(), StatusCode::OK);
    let js = response_body(js_response).await;
    assert!(js.contains("htmx:beforeRequest"));
    assert!(js.contains("data-copy-value"));
    assert!(js.contains("data-secret-toggle"));
    assert!(js.contains("midden:settings-section:"));
    assert!(js.contains("data-upload-cancel"));
}

#[tokio::test]
async fn public_upload_page_exposes_stateful_controls_and_hints() {
    let issuer = spawn_oidc_provider(serde_json::json!({
        "sub": "unused-public-ui",
        "email": "unused-public-ui@example.test",
        "groups": ["admins"]
    }))
    .await;
    let state = test_state(issuer).await;
    let response = state
        .router()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body(response).await;
    assert!(body.contains("data-upload-cancel"));
    assert!(body.contains("data-upload-resume"));
    assert!(body.contains("id=\"upload-status\""));
    assert!(body.contains("aria-describedby=\"upload-help\""));
    assert!(body.contains("No file selected"));
}

#[tokio::test]
async fn admin_settings_renders_without_user_role_quota() {
    let issuer = spawn_oidc_provider(serde_json::json!({
        "sub": "unused-admin-settings",
        "email": "unused-admin-settings@example.test",
        "groups": ["admins"]
    }))
    .await;
    let state = test_state(issuer).await;
    let admin = state
        .db
        .create_user(
            "admin-settings@example.test",
            "admin-settings",
            Some("password-hash"),
            Role::Admin,
        )
        .await
        .unwrap();
    let session_token = util::secret_token();
    state
        .db
        .create_session(
            &admin.id,
            &util::hash_token(&session_token),
            util::now_ts() + 60,
        )
        .await
        .unwrap();

    let response = state
        .router()
        .oneshot(
            Request::builder()
                .uri("/admin")
                .header(header::COOKIE, format!("midden_session={session_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("User quota"));
    assert!(body.contains("name=\"user_storage_bytes\" value=\"\""));
}

#[tokio::test]
async fn admin_settings_renders_operator_controls() {
    let (state, session_token) = admin_session_state().await;
    let response = state
        .router()
        .oneshot(
            Request::builder()
                .uri("/admin")
                .header(header::COOKIE, format!("midden_session={session_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body(response).await;
    assert!(body.contains("View access"));
    assert!(body.contains("name=\"expiry_allow_never\""));
    assert!(body.contains("name=\"upload_session_ttl_seconds\""));
    assert!(body.contains("name=\"metrics_access\""));
    assert!(body.contains("name=\"rate_limit_backend\""));
    assert!(body.contains("name=\"url_request_timeout_seconds\""));
    assert!(body.contains("name=\"forced_attachment_mime_types\""));
    assert!(body.contains("name=\"token_default_ttl_seconds\""));
    assert!(body.contains("name=\"thumbnail_max_dimension\""));
    assert!(body.contains("name=\"moderation_notify_webhook_url\""));
}

#[tokio::test]
async fn index_page_exposes_selected_file_status() {
    let issuer = spawn_oidc_provider(serde_json::json!({
        "sub": "unused-selected-file",
        "email": "unused-selected-file@example.test",
        "groups": ["admins"]
    }))
    .await;
    let state = test_state(issuer).await;
    let response = state
        .router()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("data-selected-file"));
    assert!(body.contains("No file selected"));
}

#[tokio::test]
async fn item_view_policy_and_retention_guardrails() {
    let issuer = spawn_oidc_provider(serde_json::json!({
        "sub": "unused-access",
        "email": "unused-access@example.test",
        "groups": ["admins"]
    }))
    .await;
    let state = test_state(issuer).await;
    let base = spawn_http_app(state.clone()).await;
    let client = reqwest::Client::new();

    let mut policy = state.settings().await.unwrap().policy;
    policy.view_item = ActionRule::Authenticated;
    state.db.set_json_setting("policy", &policy).await.unwrap();

    let upload = client
        .post(format!("{base}/api/v1/files"))
        .multipart(
            reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(b"authenticated viewers only".to_vec())
                    .file_name("private-ish.txt")
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

    assert_eq!(
        client
            .get(format!("{base}/files/{file_id}/raw"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    let viewer = state
        .db
        .create_user(
            "viewer@example.test",
            "viewer",
            Some("password-hash"),
            Role::User,
        )
        .await
        .unwrap();
    let session_token = util::secret_token();
    state
        .db
        .create_session(
            &viewer.id,
            &util::hash_token(&session_token),
            util::now_ts() + 60,
        )
        .await
        .unwrap();
    assert_eq!(
        client
            .get(format!("{base}/files/{file_id}/raw"))
            .header(header::COOKIE, format!("midden_session={session_token}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let mut limits = state.settings().await.unwrap().limits;
    limits.expiry.allow_never = false;
    limits.expiry.anonymous_max_file_expiry = Some("1h".to_string());
    state.db.set_json_setting("limits", &limits).await.unwrap();
    let too_long = client
        .post(format!("{base}/api/v1/files"))
        .multipart(
            reqwest::multipart::Form::new().text("expires", "2h").part(
                "file",
                reqwest::multipart::Part::bytes(b"too long".to_vec())
                    .file_name("too-long.txt")
                    .mime_str("text/plain")
                    .unwrap(),
            ),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(too_long.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn browser_upload_result_keeps_uploaded_file_as_links() {
    let issuer = spawn_oidc_provider(serde_json::json!({
        "sub": "unused-browser-upload",
        "email": "unused-browser-upload@example.test",
        "groups": ["admins"]
    }))
    .await;
    let state = test_state(issuer).await;
    let base = spawn_http_app(state).await;
    let client = reqwest::Client::new();

    let home = client.get(format!("{base}/")).send().await.unwrap();
    assert_eq!(home.status(), StatusCode::OK);
    let csrf = csrf_cookie_from(home.headers());
    let png = hex_fixture(include_str!("../../tests/fixtures/sample.png.hex"));
    let upload = client
        .post(format!("{base}/"))
        .header(header::COOKIE, format!("midden_csrf={csrf}"))
        .multipart(
            reqwest::multipart::Form::new()
                .text("csrf_token", csrf)
                .part(
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
    let body = upload.text().await.unwrap();
    assert!(body.contains("Open link"));
    assert!(body.contains("Raw file"));
    assert!(!body.contains("class=\"file-preview-media\""));
    assert!(!body.contains("alt=\"sample.png\""));
}

#[tokio::test]
async fn operational_controls_cover_upload_mime_metrics_and_rate_limits() {
    let issuer = spawn_oidc_provider(serde_json::json!({
        "sub": "unused-ops",
        "email": "unused-ops@example.test",
        "groups": ["admins"]
    }))
    .await;
    let state = test_state(issuer).await;
    let mut settings = state.settings().await.unwrap();
    settings.uploads.max_chunk_bytes = 4;
    settings.metrics.access = crate::config::MetricsAccessMode::Admin;
    settings
        .security
        .content_policy
        .forced_attachment_mime_types = vec!["text/plain".to_string()];
    settings.security.rate_limit_backend = crate::config::RateLimitBackend::Database;
    state
        .db
        .set_json_setting("uploads", &settings.uploads)
        .await
        .unwrap();
    state
        .db
        .set_json_setting("metrics", &settings.metrics)
        .await
        .unwrap();
    state
        .db
        .set_json_setting("security", &settings.security)
        .await
        .unwrap();

    let base = spawn_http_app(state.clone()).await;
    let client = reqwest::Client::new();

    let create = client
        .post(format!("{base}/tus"))
        .header("Tus-Resumable", "1.0.0")
        .header("Upload-Length", "8")
        .header("Upload-Metadata", tus_metadata("chunk.txt", "text/plain"))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let location = create.headers()["location"].to_str().unwrap();
    let too_large_chunk = client
        .patch(format!("{base}{location}"))
        .header("Tus-Resumable", "1.0.0")
        .header("Upload-Offset", "0")
        .body("12345")
        .send()
        .await
        .unwrap();
    assert_eq!(too_large_chunk.status(), StatusCode::PAYLOAD_TOO_LARGE);

    assert_eq!(
        client
            .get(format!("{base}/metrics"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let (admin_state, session_token) = admin_session_state().await;
    admin_state
        .db
        .set_json_setting("metrics", &settings.metrics)
        .await
        .unwrap();
    let admin_base = spawn_http_app(admin_state).await;
    assert_eq!(
        client
            .get(format!("{admin_base}/metrics"))
            .header(header::COOKIE, format!("midden_session={session_token}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let upload = client
        .post(format!("{base}/api/v1/files"))
        .multipart(
            reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(b"download me".to_vec())
                    .file_name("download.txt")
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
    let raw = client
        .get(format!("{base}/files/{file_id}/raw"))
        .send()
        .await
        .unwrap();
    assert_eq!(raw.status(), StatusCode::OK);
    assert!(
        raw.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment")
    );
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

    let png = hex_fixture(include_str!("../../tests/fixtures/sample.png.hex"));
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
            "content": include_str!("../../tests/fixtures/sample.txt")
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
async fn account_token_jobs_and_thumbnail_surfaces_are_available() {
    let issuer = spawn_oidc_provider(serde_json::json!({
        "sub": "unused-surfaces",
        "email": "unused-surfaces@example.test",
        "groups": ["admins"]
    }))
    .await;
    let state = test_state(issuer).await;
    let mut processing = state.settings().await.unwrap().processing;
    processing.thumbnails = true;
    processing.thumbnail_max_dimension = 12;
    state
        .db
        .set_json_setting("processing", &processing)
        .await
        .unwrap();

    let (admin, admin_token) = user_with_api_token(
        &state,
        "surface-admin@example.test",
        "surface-admin",
        Role::Admin,
        &["files:write", "files:read", "tokens:write", "tokens:read"],
    )
    .await;
    let session_token = util::secret_token();
    state
        .db
        .create_session(
            &admin.id,
            &util::hash_token(&session_token),
            util::now_ts() + 60,
        )
        .await
        .unwrap();
    let base = spawn_http_app(state.clone()).await;
    let client = reqwest::Client::new();

    let account = client
        .get(format!("{base}/account"))
        .header(header::COOKIE, format!("midden_session={session_token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(account.status(), StatusCode::OK);
    let account = account.text().await.unwrap();
    assert!(account.contains("name=\"bulk_action\""));
    assert!(account.contains("name=\"expires_in_seconds\""));

    let jobs = client
        .get(format!("{base}/admin/jobs"))
        .header(header::COOKIE, format!("midden_session={session_token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(jobs.status(), StatusCode::OK);
    let jobs = jobs.text().await.unwrap();
    assert!(jobs.contains("Background jobs"));
    assert!(jobs.contains("Run once"));

    let token = client
        .post(format!("{base}/api/v1/tokens"))
        .bearer_auth(&admin_token)
        .json(&serde_json::json!({
            "name": "short-lived",
            "scopes": ["files:read"],
            "expires_in_seconds": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(token.status(), StatusCode::OK);
    let token: serde_json::Value = token.json().await.unwrap();
    assert!(token["expires_at"].as_i64().is_some());

    let png = hex_fixture(include_str!("../../tests/fixtures/sample.png.hex"));
    let upload = client
        .post(format!("{base}/api/v1/files"))
        .bearer_auth(&admin_token)
        .multipart(
            reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(png)
                    .file_name("thumb.png")
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
    jobs::run_once(&state, &state.settings().await.unwrap())
        .await
        .unwrap();
    let file = state.db.file_by_public_id(&file_id).await.unwrap();
    assert!(file.thumbnail_hash.is_some());
    assert_ne!(
        file.thumbnail_hash.as_deref(),
        Some(file.blob_hash.as_str())
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
    let payload = hex_fixture(include_str!("../../tests/fixtures/sample.gif.hex"));

    let options = client
        .request(reqwest::Method::OPTIONS, format!("{base}/tus"))
        .send()
        .await
        .unwrap();
    assert_eq!(options.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        options.headers()["tus-max-size"].to_str().unwrap(),
        state
            .settings()
            .await
            .unwrap()
            .limits
            .max_upload_bytes
            .to_string()
    );

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
    assert!(complete.headers().contains_key("x-midden-raw-url"));
    assert!(complete.headers().contains_key("x-midden-delete-url"));
    assert!(complete.headers().contains_key("x-midden-delete-token"));

    let mut limits = state.settings().await.unwrap().limits;
    let original_max_upload_bytes = limits.max_upload_bytes;
    limits.max_upload_bytes = 1;
    state.db.set_json_setting("limits", &limits).await.unwrap();
    let too_large = client
        .post(format!("{base}/tus"))
        .header("Tus-Resumable", "1.0.0")
        .header("Upload-Length", "2")
        .send()
        .await
        .unwrap();
    assert_eq!(too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);
    limits.max_upload_bytes = original_max_upload_bytes;
    state.db.set_json_setting("limits", &limits).await.unwrap();

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
    let api_disabled = client
        .post(format!("{base}/tus"))
        .header("Tus-Resumable", "1.0.0")
        .header("Upload-Length", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(api_disabled.status(), StatusCode::CREATED);

    policy.upload_file = ActionRule::Disabled;
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
