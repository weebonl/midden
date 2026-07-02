use std::sync::Arc;

use axum::{
    Router,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use prometheus_client::registry::Registry;
use thiserror::Error;

use crate::{
    config::{AppConfig, RuntimeSettings},
    db::Database,
    mail::Mailer,
    metrics::AppMetrics,
    storage::BlobStorage,
    templates::Templates,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: Database,
    pub storage: BlobStorage,
    pub templates: Templates,
    pub mailer: Mailer,
    pub metrics: AppMetrics,
    pub rate_limiter: crate::rate_limit::RateLimiter,
    pub upload_quota_lock: Arc<tokio::sync::Mutex<()>>,
    pub registry: Arc<Registry>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> anyhow::Result<Self> {
        config.validate()?;
        let db = Database::connect(&config).await?;
        let storage = BlobStorage::from_config(&config).await?;
        let templates = Templates::load(&config)?;
        let mailer = Mailer::new(config.smtp.clone());
        let metrics = AppMetrics::new();
        let mut registry = Registry::default();
        metrics.register(&mut registry);
        Ok(Self {
            config: Arc::new(config),
            db,
            storage,
            templates,
            mailer,
            metrics,
            rate_limiter: crate::rate_limit::RateLimiter::default(),
            upload_quota_lock: Arc::new(tokio::sync::Mutex::new(())),
            registry: Arc::new(registry),
        })
    }

    pub async fn settings(&self) -> anyhow::Result<RuntimeSettings> {
        self.db.runtime_settings(&self.config).await
    }

    pub fn router(self) -> Router {
        crate::web::router(self)
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("unauthorized")]
    Unauthorized,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("too many requests")]
    TooManyRequests,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    HttpClient(#[from] reqwest::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::Io(_) | Self::HttpClient(_) | Self::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let message = if matches!(self, AppError::Other(_)) {
            tracing::error!(error = %self, "request failed");
            "internal server error".to_string()
        } else {
            self.to_string()
        };

        if let Ok(response) = crate::web::REQUEST_CONTEXT.try_with(|ctx| {
            if ctx.is_htmx {
                let html = format!(
                    "<div class=\"notice error\" x-data=\"{{ show: true }}\" x-show=\"show\">\
                       <span>Error: {}</span>\
                       <button type=\"button\" class=\"link-button\" x-on:click=\"show = false\" style=\"float: right; margin-top: -2px; font-weight: bold; text-decoration: none;\">&times;</button>\
                     </div>",
                    html_escape::encode_text(&message)
                );
                (status, Html(html)).into_response()
            } else {
                match ctx.templates.render(
                    "error.html",
                    &ctx.settings,
                    ctx.current_user.as_ref(),
                    serde_json::json!({ "message": message }),
                ) {
                    Ok(rendered) => (status, Html(rendered)).into_response(),
                    Err(err) => {
                        tracing::error!(error = %err, "failed to render error template");
                        (status, Html(format!(
                            "<!doctype html><title>{status}</title><main><h1>{status}</h1><p>{}</p><p><a href=\"/\">Return home</a></p></main>",
                            html_escape::encode_text(&message)
                        ))).into_response()
                    }
                }
            }
        }) {
            return response;
        }

        (status, Html(format!(
            "<!doctype html><title>{status}</title><main><h1>{status}</h1><p>{}</p><p><a href=\"/\">Return home</a></p></main>",
            html_escape::encode_text(&message)
        )))
            .into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
