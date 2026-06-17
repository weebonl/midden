use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct BrowseQuery {
    q: Option<String>,
    before: Option<i64>,
}

pub(super) async fn public_browse(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Query(query): Query<BrowseQuery>,
) -> AppResult<Html<String>> {
    let settings = state.settings().await?;
    if !settings.features.public_browse {
        return Err(AppError::NotFound);
    }
    let user = current_user(&state, &jar).await?;
    let limit = settings.discovery.page_size.clamp(1, 100) as i64;
    let q = query.q.as_deref().filter(|q| !q.trim().is_empty());
    let files = state.db.public_files(q, query.before, limit).await?;
    let pastes = state.db.public_pastes(q, query.before, limit).await?;
    let next_cursor = files
        .iter()
        .map(|file| file.created_at)
        .chain(pastes.iter().map(|paste| paste.created_at))
        .min();
    render(
        &state,
        if htmx_request(&headers) {
            "browse_results.html"
        } else {
            "browse.html"
        },
        &settings,
        user.as_ref(),
        serde_json::json!({
            "q": query.q.unwrap_or_default(),
            "files": files,
            "pastes": pastes,
            "next_cursor": next_cursor,
        }),
    )
}
