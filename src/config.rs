use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

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
}

impl AppConfig {
    pub fn load(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let mut builder = config::Config::builder();

        if let Some(path) = path {
            builder = builder.add_source(config::File::from(path).required(false));
        } else {
            builder = builder.add_source(config::File::with_name("midden.toml").required(false));
        }

        builder = builder.add_source(
            config::Environment::with_prefix("MIDDEN")
                .separator("__")
                .try_parsing(true),
        );

        Ok(builder.build()?.try_deserialize()?)
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
            paste_content_search: false,
            paste_editing: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    pub max_upload_bytes: i64,
    pub max_paste_bytes: i64,
    pub max_tus_upload_bytes: i64,
    pub anonymous_daily_bytes: Option<i64>,
    pub default_file_expiry: Option<String>,
    pub default_paste_expiry: Option<String>,
    pub anonymous_quota: QuotaConfig,
    pub role_quotas: BTreeMap<String, QuotaConfig>,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_upload_bytes: 200 * 1024 * 1024,
            max_paste_bytes: 1024 * 1024,
            max_tus_upload_bytes: 2 * 1024 * 1024 * 1024,
            anonymous_daily_bytes: None,
            default_file_expiry: None,
            default_paste_expiry: None,
            anonymous_quota: QuotaConfig::default(),
            role_quotas: BTreeMap::new(),
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
            url_upload: UrlUploadSecurityConfig::default(),
            rate_limits: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeliveryConfig {
    pub public_cache_seconds: u64,
    pub static_cache_seconds: u64,
    pub signed_internal_urls: bool,
    pub internal_url_secret: Option<String>,
    pub internal_url_ttl_seconds: i64,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            public_cache_seconds: 3600,
            static_cache_seconds: 31_536_000,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UrlUploadSecurityConfig {
    pub block_private_ips: bool,
    pub max_redirects: usize,
    pub allowed_hosts: Vec<String>,
    pub blocked_hosts: Vec<String>,
}

impl Default for UrlUploadSecurityConfig {
    fn default() -> Self {
        Self {
            block_private_ips: true,
            max_redirects: 3,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProcessingConfig {
    pub metadata_extraction: bool,
    pub metadata_stripping: bool,
    pub thumbnails: bool,
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
        }
    }
}
