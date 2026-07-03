use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub features: FeatureConfig,
    pub limits: LimitsConfig,
    pub branding: BrandingConfig,
    pub policy: PolicyConfig,
    pub security: SecurityConfig,
    pub delivery: DeliveryConfig,
    pub smtp: SmtpConfig,
    pub oidc: OidcConfig,
    pub scanning: ScanningConfig,
    pub processing: ProcessingConfig,
    pub discovery: DiscoveryConfig,
    pub jobs: JobsConfig,
    pub uploads: UploadsConfig,
    pub metrics: MetricsConfig,
    pub tokens: TokensConfig,
    pub moderation: ModerationConfig,
}

impl AppConfig {
    pub fn load(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let mut builder = config::Config::builder();

        if let Some(path) = path {
            builder = builder.add_source(config::File::from(path).required(true));
        } else {
            builder = builder.add_source(config::File::with_name("midden.toml").required(false));
        }

        builder = builder.add_source(
            config::Environment::with_prefix("MIDDEN")
                .separator("__")
                .try_parsing(true),
        );

        let config: Self = builder.build()?.try_deserialize()?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.delivery.isolated_file_origin && self.delivery.public_file_base_url.is_none() {
            anyhow::bail!("isolated file origin requires a public file base URL");
        }
        if self.delivery.signed_internal_urls && self.delivery.internal_url_secret.is_none() {
            anyhow::bail!("signed internal URLs require a secret");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: String,
    pub public_base_url: String,
    pub template_dir: Option<PathBuf>,
    pub static_dir: Option<PathBuf>,
    pub behind_proxy: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".to_string(),
            public_base_url: "http://127.0.0.1:8080".to_string(),
            template_dir: None,
            static_dir: None,
            behind_proxy: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://midden.db?mode=rwc".to_string(),
            max_connections: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub backend: StorageBackend,
    pub local: LocalStorageConfig,
    pub s3: S3StorageConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Local,
            local: LocalStorageConfig::default(),
            s3: S3StorageConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    Local,
    S3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalStorageConfig {
    pub path: PathBuf,
}

impl Default for LocalStorageConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("data/blobs"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct S3StorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub prefix: Option<String>,
    pub allow_http: bool,
    pub virtual_hosted_style: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatureConfig {
    pub files: bool,
    pub pastes: bool,
    pub accounts: bool,
    pub api: bool,
    pub reports: bool,
    pub upload_by_url: bool,
    pub preview_pages: bool,
    pub public_browse: bool,
    pub oidc_login: bool,
    pub local_login: bool,
    pub paste_content_search: bool,
    pub paste_editing: bool,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            files: true,
            pastes: true,
            accounts: true,
            api: true,
            reports: true,
            upload_by_url: false,
            preview_pages: false,
            public_browse: false,
            oidc_login: false,
            local_login: true,
            paste_content_search: false,
            paste_editing: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LimitsConfig {
    pub max_upload_bytes: i64,
    pub max_paste_bytes: i64,
    pub anonymous_daily_bytes: Option<i64>,
    pub default_file_expiry: Option<String>,
    pub default_paste_expiry: Option<String>,
    pub expiry: ExpiryGuardrailsConfig,
    pub anonymous_quota: QuotaConfig,
    pub role_quotas: BTreeMap<String, QuotaConfig>,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_upload_bytes: 2 * 1024 * 1024 * 1024,
            max_paste_bytes: 1024 * 1024,
            anonymous_daily_bytes: None,
            default_file_expiry: None,
            default_paste_expiry: None,
            expiry: ExpiryGuardrailsConfig::default(),
            anonymous_quota: QuotaConfig::default(),
            role_quotas: BTreeMap::new(),
        }
    }
}

impl<'de> Deserialize<'de> for LimitsConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Default)]
        #[serde(default, deny_unknown_fields)]
        struct RawLimitsConfig {
            max_upload_bytes: Option<i64>,
            max_paste_bytes: Option<i64>,
            anonymous_daily_bytes: Option<i64>,
            default_file_expiry: Option<String>,
            default_paste_expiry: Option<String>,
            expiry: ExpiryGuardrailsConfig,
            anonymous_quota: QuotaConfig,
            role_quotas: BTreeMap<String, QuotaConfig>,
        }

        let raw = RawLimitsConfig::deserialize(deserializer)?;
        let default = LimitsConfig::default();

        Ok(Self {
            max_upload_bytes: raw.max_upload_bytes.unwrap_or(default.max_upload_bytes),
            max_paste_bytes: raw.max_paste_bytes.unwrap_or(default.max_paste_bytes),
            anonymous_daily_bytes: raw.anonymous_daily_bytes,
            default_file_expiry: raw.default_file_expiry,
            default_paste_expiry: raw.default_paste_expiry,
            expiry: raw.expiry,
            anonymous_quota: raw.anonymous_quota,
            role_quotas: raw.role_quotas,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExpiryGuardrailsConfig {
    pub allow_never: bool,
    pub anonymous_max_file_expiry: Option<String>,
    pub user_max_file_expiry: Option<String>,
    pub anonymous_max_paste_expiry: Option<String>,
    pub user_max_paste_expiry: Option<String>,
    pub allowed_presets: Vec<String>,
}

impl Default for ExpiryGuardrailsConfig {
    fn default() -> Self {
        Self {
            allow_never: true,
            anonymous_max_file_expiry: None,
            user_max_file_expiry: None,
            anonymous_max_paste_expiry: None,
            user_max_paste_expiry: None,
            allowed_presets: vec![
                "1h".to_string(),
                "12h".to_string(),
                "1d".to_string(),
                "7d".to_string(),
                "30d".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QuotaConfig {
    pub storage_bytes: Option<i64>,
    pub daily_upload_bytes: Option<i64>,
    pub monthly_upload_bytes: Option<i64>,
    pub item_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrandingConfig {
    pub instance_name: String,
    pub tagline: String,
    pub logo_url: Option<String>,
    pub favicon_url: Option<String>,
    pub accent_color: String,
    pub custom_css: String,
    pub dark_mode: String,
    pub footer_links: Vec<NavLink>,
    pub homepage_notices: Vec<String>,
    pub homepage_blocks: Vec<HomepageBlock>,
    pub abuse_email: Option<String>,
    pub contact_url: Option<String>,
    pub opengraph_description: String,
    pub opengraph_files: bool,
    pub opengraph_pastes: bool,
    pub takedown_page_text: String,
}

impl Default for BrandingConfig {
    fn default() -> Self {
        Self {
            instance_name: "Midden".to_string(),
            tagline: "Upload a file and get a link.".to_string(),
            logo_url: None,
            favicon_url: None,
            accent_color: "oklch(0.44 0.12 235)".to_string(),
            custom_css: String::new(),
            dark_mode: "auto".to_string(),
            footer_links: vec![
                NavLink::new("FAQ", "/faq"),
                NavLink::new("API", "/api/docs"),
                NavLink::new("Contact", "/contact"),
            ],
            homepage_notices: Vec::new(),
            homepage_blocks: Vec::new(),
            abuse_email: None,
            contact_url: None,
            opengraph_description: "A self-hosted file and paste sharing service.".to_string(),
            opengraph_files: true,
            opengraph_pastes: true,
            takedown_page_text: "This item is unavailable.".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomepageBlock {
    pub title: String,
    pub body: String,
    pub href: Option<String>,
    pub link_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavLink {
    pub label: String,
    pub href: String,
}

impl NavLink {
    pub fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: href.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyConfig {
    pub signup: SignupMode,
    pub upload_file: ActionRule,
    pub create_paste: ActionRule,
    pub use_api: ActionRule,
    pub view_item: ActionRule,
    pub delete_own_item: ActionRule,
    pub delete_policy: DeletePolicy,
    pub claim_anonymous_item: ActionRule,
    pub create_account: ActionRule,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            signup: SignupMode::Disabled,
            upload_file: ActionRule::Anonymous,
            create_paste: ActionRule::Anonymous,
            use_api: ActionRule::Anonymous,
            view_item: ActionRule::Anonymous,
            delete_own_item: ActionRule::Authenticated,
            delete_policy: DeletePolicy::DeleteTokens,
            claim_anonymous_item: ActionRule::Authenticated,
            create_account: ActionRule::Disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignupMode {
    Disabled,
    Open,
    InviteOnly,
    AdminCreated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionRule {
    Disabled,
    Anonymous,
    Authenticated,
    Moderator,
    Admin,
    Owner,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeletePolicy {
    Disabled,
    DeleteTokens,
    NoAnonymousDelete,
    ClaimLater,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub session_cookie_name: String,
    pub session_ttl_seconds: i64,
    pub secure_cookies: bool,
    pub content_disposition: ContentDispositionMode,
    pub reject_mime_mismatch: bool,
    pub rate_limit_backend: RateLimitBackend,
    pub content_policy: ContentPolicyConfig,
    pub url_upload: UrlUploadSecurityConfig,
    pub rate_limits: BTreeMap<String, RateLimitConfig>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            session_cookie_name: "midden_session".to_string(),
            session_ttl_seconds: 60 * 60 * 24 * 30,
            secure_cookies: false,
            content_disposition: ContentDispositionMode::Inline,
            reject_mime_mismatch: false,
            rate_limit_backend: RateLimitBackend::Memory,
            content_policy: ContentPolicyConfig::default(),
            url_upload: UrlUploadSecurityConfig::default(),
            rate_limits: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitBackend {
    Memory,
    Database,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentPolicyConfig {
    pub allowed_mime_types: Vec<String>,
    pub forced_attachment_mime_types: Vec<String>,
    pub risky_mime_mode: RiskyMimeMode,
    pub max_filename_bytes: usize,
}

impl Default for ContentPolicyConfig {
    fn default() -> Self {
        Self {
            allowed_mime_types: Vec::new(),
            forced_attachment_mime_types: vec![
                "image/svg+xml".to_string(),
                "text/html".to_string(),
                "application/javascript".to_string(),
                "text/javascript".to_string(),
            ],
            risky_mime_mode: RiskyMimeMode::Attachment,
            max_filename_bytes: 180,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeliveryConfig {
    pub public_cache_seconds: u64,
    pub static_cache_seconds: u64,
    pub public_file_base_url: Option<String>,
    pub isolated_file_origin: bool,
    pub signed_internal_urls: bool,
    pub internal_url_secret: Option<String>,
    pub internal_url_ttl_seconds: i64,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            public_cache_seconds: 3600,
            static_cache_seconds: 31_536_000,
            public_file_base_url: None,
            isolated_file_origin: false,
            signed_internal_urls: false,
            internal_url_secret: None,
            internal_url_ttl_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentDispositionMode {
    Inline,
    Attachment,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskyMimeMode {
    Attachment,
    InlineOnIsolatedOrigin,
    Plaintext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UrlUploadSecurityConfig {
    pub block_private_ips: bool,
    pub max_redirects: usize,
    pub connect_timeout_seconds: u64,
    pub request_timeout_seconds: u64,
    pub max_response_bytes: Option<i64>,
    pub allowed_ports: Vec<u16>,
    pub blocked_ports: Vec<u16>,
    pub user_agent: Option<String>,
    pub allowed_hosts: Vec<String>,
    pub blocked_hosts: Vec<String>,
}

impl Default for UrlUploadSecurityConfig {
    fn default() -> Self {
        Self {
            block_private_ips: true,
            max_redirects: 3,
            connect_timeout_seconds: 10,
            request_timeout_seconds: 60,
            max_response_bytes: None,
            allowed_ports: Vec::new(),
            blocked_ports: Vec::new(),
            user_agent: Some("Midden URL upload".to_string()),
            allowed_hosts: Vec::new(),
            blocked_hosts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub requests: u32,
    pub window_seconds: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SmtpConfig {
    pub enabled: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct OidcConfig {
    pub enabled: bool,
    pub issuer_url: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub redirect_url: Option<String>,
    pub allowed_domains: Vec<String>,
    pub allowed_groups: Vec<String>,
    pub role_claim: Option<String>,
    pub groups_claim: Option<String>,
    pub role_mappings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanningConfig {
    pub enabled: bool,
    pub adapters: Vec<ScannerAdapterConfig>,
    pub blocked_hashes: Vec<String>,
    pub blocked_mime_types: Vec<String>,
    pub default_on_error: ScanDecision,
}

impl Default for ScanningConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            adapters: Vec::new(),
            blocked_hashes: Vec::new(),
            blocked_mime_types: Vec::new(),
            default_on_error: ScanDecision::Allow,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProcessingConfig {
    pub metadata_extraction: bool,
    pub metadata_stripping: bool,
    pub thumbnails: bool,
    pub thumbnail_max_dimension: u32,
    pub thumbnail_jpeg_quality: u8,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            metadata_extraction: false,
            metadata_stripping: false,
            thumbnails: false,
            thumbnail_max_dimension: 320,
            thumbnail_jpeg_quality: 82,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryConfig {
    pub robots_index: bool,
    pub page_size: u32,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            robots_index: false,
            page_size: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JobsConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub metadata_limit: u32,
    pub scanner_retry_limit: u32,
    pub storage_verify_interval_seconds: u64,
}

impl Default for JobsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 300,
            metadata_limit: 25,
            scanner_retry_limit: 10,
            storage_verify_interval_seconds: 3600,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UploadsConfig {
    pub temp_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub access: MetricsAccessMode,
    pub bearer_token: Option<String>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            access: MetricsAccessMode::Admin,
            bearer_token: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricsAccessMode {
    #[default]
    Public,
    Admin,
    Token,
    Loopback,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TokensConfig {
    pub default_ttl_seconds: Option<i64>,
    pub max_ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModerationConfig {
    pub notify_webhook_url: Option<String>,
    pub notify_webhook_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScannerAdapterConfig {
    ClamAv { socket: String },
    Command { program: String, args: Vec<String> },
    Webhook { url: String, secret: Option<String> },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanDecision {
    Allow,
    Quarantine,
    Reject,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSettings {
    pub features: FeatureConfig,
    pub limits: LimitsConfig,
    pub branding: BrandingConfig,
    pub policy: PolicyConfig,
    pub security: SecurityConfig,
    pub delivery: DeliveryConfig,
    pub scanning: ScanningConfig,
    pub processing: ProcessingConfig,
    pub discovery: DiscoveryConfig,
    pub jobs: JobsConfig,
    pub uploads: UploadsConfig,
    pub metrics: MetricsConfig,
    pub tokens: TokensConfig,
    pub moderation: ModerationConfig,
}

impl RuntimeSettings {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            features: config.features.clone(),
            limits: config.limits.clone(),
            branding: config.branding.clone(),
            policy: config.policy.clone(),
            security: config.security.clone(),
            delivery: config.delivery.clone(),
            scanning: config.scanning.clone(),
            processing: config.processing.clone(),
            discovery: config.discovery.clone(),
            jobs: config.jobs.clone(),
            uploads: config.uploads.clone(),
            metrics: config.metrics.clone(),
            tokens: config.tokens.clone(),
            moderation: config.moderation.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_config_path_must_exist() {
        let missing = std::env::temp_dir().join(format!(
            "midden-missing-config-{}.toml",
            uuid::Uuid::new_v4()
        ));
        assert!(AppConfig::load(Some(missing)).is_err());
    }

    #[test]
    fn unknown_limit_fields_are_rejected() {
        let source = r#"
            [limits]
            obsolete_upload_limit_bytes = 2147483648
        "#;
        let result = config::Config::builder()
            .add_source(config::File::from_str(source, config::FileFormat::Toml))
            .build()
            .unwrap()
            .try_deserialize::<AppConfig>();
        assert!(result.is_err());
    }

    #[test]
    fn metrics_are_not_publicly_enabled_by_default() {
        let metrics = MetricsConfig::default();
        assert!(!metrics.enabled);
        assert_ne!(metrics.access, MetricsAccessMode::Public);
    }

    #[test]
    fn invalid_delivery_modes_are_rejected_by_config_validation() {
        let mut config = AppConfig::default();
        config.delivery.isolated_file_origin = true;
        config.delivery.public_file_base_url = None;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.delivery.signed_internal_urls = true;
        config.delivery.internal_url_secret = None;
        assert!(config.validate().is_err());
    }
}
