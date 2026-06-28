use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use sqlx::{
    Any, AnyPool, AssertSqlSafe, ConnectOptions, Row,
    any::{AnyArguments, AnyConnectOptions, AnyPoolOptions},
};

use crate::{
    config::{AppConfig, RuntimeSettings},
    util,
};

mod auth;
mod items;
mod models;
mod moderation;
mod schema;
mod search;
mod settings;

pub use models::*;
use schema::SCHEMA;

#[derive(Debug, Clone)]
pub struct Database {
    pool: AnyPool,
    kind: DatabaseKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatabaseKind {
    Postgres,
    Sqlite,
}

impl Database {
    pub async fn connect(config: &AppConfig) -> anyhow::Result<Self> {
        sqlx::any::install_default_drivers();
        let url = &config.database.url;
        let kind = if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            DatabaseKind::Postgres
        } else {
            DatabaseKind::Sqlite
        };
        let options = url
            .parse::<AnyConnectOptions>()?
            .disable_statement_logging();
        let pool = AnyPoolOptions::new()
            .max_connections(config.database.max_connections)
            .connect_with(options)
            .await?;
        Ok(Self { pool, kind })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        for statement in SCHEMA.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement).execute(&self.pool).await?;
            }
        }
        self.run_additive_migrations().await?;
        Ok(())
    }

    async fn run_additive_migrations(&self) -> anyhow::Result<()> {
        self.add_column_if_missing("users", "is_disabled INTEGER NOT NULL DEFAULT 0")
            .await?;
        let added_email_verified_at = self
            .add_column_if_missing("users", "email_verified_at INTEGER")
            .await?;
        self.add_column_if_missing("users", "two_factor_enabled INTEGER NOT NULL DEFAULT 0")
            .await?;
        self.add_column_if_missing("api_tokens", "expires_at INTEGER")
            .await?;
        self.add_column_if_missing("api_tokens", "last_used_at INTEGER")
            .await?;
        if added_email_verified_at {
            self.query(
                "UPDATE users SET email_verified_at = created_at WHERE email_verified_at IS NULL",
            )
            .execute(&self.pool)
            .await?;
        }
        self.add_column_if_missing("invite_tokens", "revoked_at INTEGER")
            .await?;
        self.add_column_if_missing("files", "expires_at INTEGER")
            .await?;
        self.add_column_if_missing("files", "image_width INTEGER")
            .await?;
        self.add_column_if_missing("files", "image_height INTEGER")
            .await?;
        self.add_column_if_missing("files", "visibility TEXT NOT NULL DEFAULT 'unlisted'")
            .await?;
        self.add_column_if_missing("files", "metadata_json TEXT")
            .await?;
        self.add_column_if_missing("files", "thumbnail_hash TEXT")
            .await?;
        self.add_column_if_missing("pastes", "expires_at INTEGER")
            .await?;
        self.add_column_if_missing("pastes", "visibility TEXT NOT NULL DEFAULT 'unlisted'")
            .await?;

        Ok(())
    }

    async fn add_column_if_missing(&self, table: &str, column_def: &str) -> anyhow::Result<bool> {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column_def}");
        match sqlx::query(AssertSqlSafe(sql)).execute(&self.pool).await {
            Ok(_) => Ok(true),
            Err(err) if is_duplicate_column_error(&err) => Ok(false),
            Err(err) => Err(err.into()),
        }
    }

    fn query(&self, sql: &'static str) -> sqlx::query::Query<'static, Any, AnyArguments> {
        sqlx::query(self.rebind_sql(sql))
    }

    fn rebind_sql(&self, sql: &'static str) -> &'static str {
        if self.kind != DatabaseKind::Postgres {
            return sql;
        }

        static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut cache = cache.lock().expect("SQL rebind cache poisoned");
        if let Some(rebound) = cache.get(sql) {
            return rebound;
        }

        let mut index = 1;
        let mut rebound = String::with_capacity(sql.len() + 8);
        for ch in sql.chars() {
            if ch == '?' {
                rebound.push('$');
                rebound.push_str(&index.to_string());
                index += 1;
            } else {
                rebound.push(ch);
            }
        }
        let rebound = Box::leak(rebound.into_boxed_str());
        cache.insert(sql, rebound);
        rebound
    }

    pub async fn health(&self) -> bool {
        self.query("SELECT 1").execute(&self.pool).await.is_ok()
    }

    pub async fn audit(
        &self,
        actor_user_id: Option<&str>,
        action: &str,
        target: &str,
        detail: &str,
    ) -> anyhow::Result<()> {
        self.query(
            "INSERT INTO audit_events (id, actor_user_id, action, target, detail, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(actor_user_id)
        .bind(action)
        .bind(target)
        .bind(detail)
        .bind(util::now_ts())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn check_rate_limit(
        &self,
        action: &str,
        identity: &str,
        config: Option<&crate::config::RateLimitConfig>,
    ) -> anyhow::Result<bool> {
        let Some(config) = config.filter(|config| config.enabled) else {
            return Ok(true);
        };
        if config.requests == 0 || config.window_seconds == 0 {
            return Ok(false);
        }
        let now = util::now_ts();
        let key = format!("{action}:{identity}");
        let row = self
            .query("SELECT window_start, count FROM rate_limit_buckets WHERE key = ?")
            .bind(&key)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            self.query(
                "INSERT INTO rate_limit_buckets (key, window_start, count) VALUES (?, ?, 1)",
            )
            .bind(&key)
            .bind(now)
            .execute(&self.pool)
            .await?;
            return Ok(true);
        };
        let window_start: i64 = row.try_get("window_start")?;
        let count: i64 = row.try_get("count")?;
        if now.saturating_sub(window_start) >= config.window_seconds as i64 {
            self.query("UPDATE rate_limit_buckets SET window_start = ?, count = 1 WHERE key = ?")
                .bind(now)
                .bind(&key)
                .execute(&self.pool)
                .await?;
            return Ok(true);
        }
        if count >= i64::from(config.requests) {
            return Ok(false);
        }
        self.query("UPDATE rate_limit_buckets SET count = count + 1 WHERE key = ?")
            .bind(&key)
            .execute(&self.pool)
            .await?;
        Ok(true)
    }
}

fn is_duplicate_column_error(err: &sqlx::Error) -> bool {
    let message = err.to_string().to_lowercase();
    message.contains("duplicate column")
        || message.contains("already exists")
        || message.contains("column exists")
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Database {
        let mut config = AppConfig::default();
        config.database.url = "sqlite::memory:".to_string();
        config.database.max_connections = 1;
        let db = Database::connect(&config).await.unwrap();
        db.migrate().await.unwrap();
        db
    }

    #[tokio::test]
    async fn migrates_and_reads_runtime_settings() {
        let db = test_db().await;
        let config = AppConfig::default();
        let mut settings = db.runtime_settings(&config).await.unwrap();
        assert!(settings.features.files);
        settings.features.pastes = false;
        db.set_json_setting("features", &settings.features)
            .await
            .unwrap();
        let settings = db.runtime_settings(&config).await.unwrap();
        assert!(!settings.features.pastes);
    }

    #[tokio::test]
    async fn config_runtime_settings_include_operator_controls() {
        let db = test_db().await;
        let config = AppConfig::default();
        let settings = db.runtime_settings(&config).await.unwrap();

        assert_eq!(
            settings.security.rate_limit_backend,
            crate::config::RateLimitBackend::Memory
        );
        assert_eq!(
            settings.metrics.access,
            crate::config::MetricsAccessMode::Public
        );
        assert!(settings.metrics.enabled);
        assert!(
            settings
                .limits
                .expiry
                .allowed_presets
                .contains(&"7d".to_string())
        );
        assert!(settings.security.url_upload.request_timeout_seconds > 0);
        assert!(
            settings
                .security
                .content_policy
                .forced_attachment_mime_types
                .contains(&"text/html".to_string())
        );
        assert!(settings.tokens.default_ttl_seconds.is_none());
        assert!(settings.processing.thumbnail_max_dimension > 0);
        assert!(settings.moderation.notify_webhook_url.is_none());

        let mut metrics = settings.metrics.clone();
        metrics.access = crate::config::MetricsAccessMode::Admin;
        db.set_json_setting("metrics", &metrics).await.unwrap();
        let settings = db.runtime_settings(&config).await.unwrap();
        assert_eq!(
            settings.metrics.access,
            crate::config::MetricsAccessMode::Admin
        );
    }

    #[tokio::test]
    async fn owner_upsert_sets_owner_role() {
        let db = test_db().await;
        let user = db
            .upsert_owner("root@example.test", "root", Some("hash"))
            .await
            .unwrap();
        assert_eq!(user.role, Role::Owner);
    }

    #[tokio::test]
    async fn creating_file_item_increments_blob_ref_once() {
        let db = test_db().await;
        db.create_blob_if_missing("abc123", 4, Some("text/plain"))
            .await
            .unwrap();
        db.create_file_item(NewFileItem {
            id: "file-1",
            public_id: "pub123",
            blob_hash: "abc123",
            original_filename: Some("a.txt"),
            extension: Some("txt"),
            content_type: Some("text/plain"),
            size_bytes: 4,
            image_width: None,
            image_height: None,
            owner_user_id: None,
            delete_token_hash: None,
            expires_at: None,
            visibility: "unlisted",
            metadata_json: None,
            thumbnail_hash: None,
            state: "active",
        })
        .await
        .unwrap();
        assert_eq!(db.blob_ref_count("abc123").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn additive_migration_preserves_old_user_rows() {
        let mut config = AppConfig::default();
        config.database.url = "sqlite::memory:".to_string();
        config.database.max_connections = 1;
        let db = Database::connect(&config).await.unwrap();
        db.query(
            "CREATE TABLE users (
                id TEXT PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT,
                role TEXT NOT NULL,
                created_at INTEGER NOT NULL
             )",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        db.query(
            "INSERT INTO users (id, email, username, password_hash, role, created_at)
             VALUES ('old-user', 'old@example.test', 'old', 'hash', 'admin', 1234)",
        )
        .execute(&db.pool)
        .await
        .unwrap();

        db.migrate().await.unwrap();

        let user = db.user_by_email("old@example.test").await.unwrap();
        assert_eq!(user.id, "old-user");
        assert_eq!(user.role, Role::Admin);
        assert_eq!(user.email_verified_at, Some(1234));
        assert!(!user.two_factor_enabled);
    }

    #[tokio::test]
    async fn item_state_regressions_cover_moderation_delete_and_expiry() {
        let db = test_db().await;
        db.create_blob_if_missing("statehash", 4, Some("text/plain"))
            .await
            .unwrap();
        let file = db
            .create_file_item(NewFileItem {
                id: "state-file",
                public_id: "statepub",
                blob_hash: "statehash",
                original_filename: Some("state.txt"),
                extension: Some("txt"),
                content_type: Some("text/plain"),
                size_bytes: 4,
                image_width: None,
                image_height: None,
                owner_user_id: None,
                delete_token_hash: Some("delete-hash"),
                expires_at: Some(util::now_ts() - 1),
                visibility: "unlisted",
                metadata_json: None,
                thumbnail_hash: None,
                state: "active",
            })
            .await
            .unwrap();

        assert_eq!(db.expired_files().await.unwrap().len(), 1);
        db.expire_file(&file.id).await.unwrap();
        assert!(
            db.active_file_by_public_id("statepub")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(db.file_by_id(&file.id).await.unwrap().state, "expired");

        for state in ["quarantined", "takedown", "legal_hold"] {
            db.update_file_state_by_public_id("statepub", state, None, "regression")
                .await
                .unwrap();
            assert!(
                db.active_file_by_public_id("statepub")
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(db.file_by_id(&file.id).await.unwrap().state, state);
        }

        db.update_file_state_by_public_id("statepub", "active", None, "regression")
            .await
            .unwrap();
        let deleted = db.delete_file(&file.id, None, "regression").await.unwrap();
        assert_eq!(deleted.public_id, "statepub");
        assert!(
            db.active_file_by_public_id("statepub")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(db.file_by_id(&file.id).await.unwrap().state, "deleted");
    }

    #[tokio::test]
    async fn postgres_migration_smoke_when_configured() {
        let Ok(url) = std::env::var("MIDDEN_TEST_POSTGRES_URL") else {
            eprintln!("skipping Postgres migration smoke; MIDDEN_TEST_POSTGRES_URL is not set");
            return;
        };
        let mut config = AppConfig::default();
        config.database.url = url;
        let db = Database::connect(&config).await.unwrap();
        db.migrate().await.unwrap();
        let suffix = util::public_id();
        let email = format!("postgres-smoke-{suffix}@example.test");
        let user = db
            .create_user(
                &email,
                &format!("postgres-smoke-{suffix}"),
                Some("hash"),
                Role::User,
            )
            .await
            .unwrap();
        assert_eq!(user.email, email);
    }
}
