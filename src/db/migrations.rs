use sqlx::{Any, AssertSqlSafe, Row, Transaction};

use super::{Database, DatabaseKind};
use crate::util;

use super::schema::{BLOB_MUTATION_LOCK_TABLE, MIGRATION_TABLE, SCHEMA};

#[derive(Debug, Clone, Copy)]
struct Migration {
    version: i64,
    name: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "baseline_schema",
    },
    Migration {
        version: 2,
        name: "identity_columns_and_email_backfill",
    },
    Migration {
        version: 3,
        name: "token_lifecycle_columns",
    },
    Migration {
        version: 4,
        name: "item_lifecycle_columns",
    },
    Migration {
        version: 5,
        name: "postgres_i64_columns_bigint",
    },
    Migration {
        version: 6,
        name: "blob_mutation_locks",
    },
];

const LAST_PRE_LEDGER_MIGRATION: i64 = 4;

pub(super) const POSTGRES_I64_COLUMNS: &[(&str, &str)] = &[
    ("schema_migrations", "version"),
    ("schema_migrations", "applied_at"),
    ("settings", "updated_at"),
    ("users", "email_verified_at"),
    ("users", "created_at"),
    ("sessions", "expires_at"),
    ("sessions", "created_at"),
    ("api_tokens", "expires_at"),
    ("api_tokens", "last_used_at"),
    ("api_tokens", "revoked_at"),
    ("api_tokens", "created_at"),
    ("password_reset_tokens", "expires_at"),
    ("password_reset_tokens", "used_at"),
    ("password_reset_tokens", "created_at"),
    ("oidc_identities", "created_at"),
    ("oidc_identities", "last_seen_at"),
    ("email_verification_tokens", "expires_at"),
    ("email_verification_tokens", "used_at"),
    ("email_verification_tokens", "created_at"),
    ("two_factor_challenges", "expires_at"),
    ("two_factor_challenges", "used_at"),
    ("two_factor_challenges", "created_at"),
    ("invite_tokens", "expires_at"),
    ("invite_tokens", "used_at"),
    ("invite_tokens", "revoked_at"),
    ("invite_tokens", "created_at"),
    ("blobs", "size_bytes"),
    ("blobs", "ref_count"),
    ("blobs", "created_at"),
    ("files", "size_bytes"),
    ("files", "image_width"),
    ("files", "image_height"),
    ("files", "expires_at"),
    ("files", "created_at"),
    ("pastes", "expires_at"),
    ("pastes", "created_at"),
    ("paste_revisions", "created_at"),
    ("reports", "created_at"),
    ("scanner_results", "created_at"),
    ("audit_events", "created_at"),
    ("rate_limit_buckets", "window_start"),
    ("rate_limit_buckets", "count"),
    ("moderation_notes", "created_at"),
];

impl Database {
    pub async fn migrate(&self) -> anyhow::Result<()> {
        let mut transaction = self.pool.begin().await?;
        self.lock_migration_sequence_before_ledger(&mut transaction)
            .await?;
        self.ensure_migration_table(&mut transaction).await?;
        self.lock_migration_table(&mut transaction).await?;
        self.validate_migration_ledger(&mut transaction).await?;
        self.adopt_completed_unversioned_migrations(&mut transaction)
            .await?;
        for migration in MIGRATIONS {
            self.apply_migration(&mut transaction, migration).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn validate_migration_ledger(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        let rows = self
            .query("SELECT version, name FROM schema_migrations ORDER BY version")
            .fetch_all(&mut **transaction)
            .await?;
        let latest = MIGRATIONS.last().map_or(0, |migration| migration.version);
        for row in rows {
            let version = row.try_get::<i64, _>("version")?;
            let name = row.try_get::<String, _>("name")?;
            if version > latest {
                anyhow::bail!(
                    "database schema version {version} is newer than this binary supports ({latest})"
                );
            }
            let Some(expected) = MIGRATIONS
                .iter()
                .find(|migration| migration.version == version)
            else {
                anyhow::bail!("database contains unknown migration version {version}");
            };
            if name != expected.name {
                anyhow::bail!(
                    "database migration {version} is named {name:?}, expected {:?}",
                    expected.name
                );
            }
        }
        Ok(())
    }

    async fn lock_migration_sequence_before_ledger(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        if self.kind == DatabaseKind::Postgres {
            // The table does not exist on a fresh database, so serialize its creation with
            // a stable transaction-level lock before taking the table lock used by existing
            // installations. Every migration decision remains protected until commit.
            sqlx::query("SELECT pg_advisory_xact_lock(1296647236, 17742)")
                .execute(&mut **transaction)
                .await?;
        }
        Ok(())
    }

    async fn ensure_migration_table(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        sqlx::query(MIGRATION_TABLE)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    async fn adopt_completed_unversioned_migrations(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        let row = self
            .query("SELECT COUNT(*) AS count FROM schema_migrations")
            .fetch_one(&mut **transaction)
            .await?;
        if row.try_get::<i64, _>("count")? != 0
            || !self
                .has_completed_unversioned_additive_schema(transaction)
                .await?
        {
            return Ok(());
        }

        // A structurally complete pre-ledger database may contain intentionally unverified
        // users. Adopt its additive migrations rather than treating an ambiguous NULL as
        // permission to verify an account. Structurally incomplete upgrades still run v2.
        for migration in MIGRATIONS.iter().filter(|migration| {
            migration.version > 1 && migration.version <= LAST_PRE_LEDGER_MIGRATION
        }) {
            self.record_migration(transaction, migration).await?;
        }
        Ok(())
    }

    async fn has_completed_unversioned_additive_schema(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<bool> {
        for (table, column) in [
            ("users", "is_disabled"),
            ("users", "email_verified_at"),
            ("users", "two_factor_enabled"),
            ("api_tokens", "expires_at"),
            ("api_tokens", "last_used_at"),
            ("invite_tokens", "revoked_at"),
            ("files", "expires_at"),
            ("files", "image_width"),
            ("files", "image_height"),
            ("files", "visibility"),
            ("files", "metadata_json"),
            ("files", "thumbnail_hash"),
            ("pastes", "expires_at"),
            ("pastes", "visibility"),
        ] {
            if !self.column_exists(transaction, table, column).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn apply_migration(
        &self,
        transaction: &mut Transaction<'_, Any>,
        migration: &Migration,
    ) -> anyhow::Result<()> {
        if self.migration_is_applied(transaction, migration).await? {
            return Ok(());
        }

        match migration.version {
            1 => self.create_baseline_schema(transaction).await?,
            2 => self.migrate_identity_columns(transaction).await?,
            3 => self.migrate_token_columns(transaction).await?,
            4 => self.migrate_item_columns(transaction).await?,
            5 => {
                self.migrate_postgres_i64_columns_to_bigint(transaction)
                    .await?
            }
            6 => self.create_blob_mutation_lock_table(transaction).await?,
            version => anyhow::bail!("unknown migration version {version}"),
        }
        self.record_migration(transaction, migration).await?;
        Ok(())
    }

    async fn lock_migration_table(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        match self.kind {
            DatabaseKind::Postgres => {
                sqlx::query("LOCK TABLE schema_migrations IN EXCLUSIVE MODE")
                    .execute(&mut **transaction)
                    .await?;
            }
            DatabaseKind::Sqlite => {
                sqlx::query(
                    "UPDATE schema_migrations SET applied_at = applied_at WHERE version = -1",
                )
                .execute(&mut **transaction)
                .await?;
            }
        }
        Ok(())
    }

    async fn migration_is_applied(
        &self,
        transaction: &mut Transaction<'_, Any>,
        migration: &Migration,
    ) -> anyhow::Result<bool> {
        let row = self
            .query("SELECT name FROM schema_migrations WHERE version = ?")
            .bind(migration.version)
            .fetch_optional(&mut **transaction)
            .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let name = row.try_get::<String, _>("name")?;
        if name != migration.name {
            anyhow::bail!(
                "database migration {} is named {name:?}, expected {:?}",
                migration.version,
                migration.name
            );
        }
        Ok(true)
    }

    async fn record_migration(
        &self,
        transaction: &mut Transaction<'_, Any>,
        migration: &Migration,
    ) -> anyhow::Result<()> {
        self.query(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?, ?, ?)
             ON CONFLICT(version) DO NOTHING",
        )
        .bind(migration.version)
        .bind(migration.name)
        .bind(util::now_ts())
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    async fn create_baseline_schema(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        for statement in SCHEMA
            .split(';')
            .map(str::trim)
            .filter(|sql| !sql.is_empty())
        {
            sqlx::query(statement).execute(&mut **transaction).await?;
        }
        Ok(())
    }

    async fn migrate_identity_columns(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        self.add_column_if_missing(
            transaction,
            "users",
            "is_disabled",
            "is_disabled INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        self.add_column_if_missing(
            transaction,
            "users",
            "email_verified_at",
            "email_verified_at INTEGER",
        )
        .await?;
        self.add_column_if_missing(
            transaction,
            "users",
            "two_factor_enabled",
            "two_factor_enabled INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        self.query(
            "UPDATE users SET email_verified_at = created_at WHERE email_verified_at IS NULL",
        )
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    async fn migrate_token_columns(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        for (table, column, definition) in [
            ("api_tokens", "expires_at", "expires_at INTEGER"),
            ("api_tokens", "last_used_at", "last_used_at INTEGER"),
            ("invite_tokens", "revoked_at", "revoked_at INTEGER"),
        ] {
            self.add_column_if_missing(transaction, table, column, definition)
                .await?;
        }
        Ok(())
    }

    async fn migrate_item_columns(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        for (table, column, definition) in [
            ("files", "expires_at", "expires_at INTEGER"),
            ("files", "image_width", "image_width INTEGER"),
            ("files", "image_height", "image_height INTEGER"),
            (
                "files",
                "visibility",
                "visibility TEXT NOT NULL DEFAULT 'unlisted'",
            ),
            ("files", "metadata_json", "metadata_json TEXT"),
            ("files", "thumbnail_hash", "thumbnail_hash TEXT"),
            ("pastes", "expires_at", "expires_at INTEGER"),
            (
                "pastes",
                "visibility",
                "visibility TEXT NOT NULL DEFAULT 'unlisted'",
            ),
        ] {
            self.add_column_if_missing(transaction, table, column, definition)
                .await?;
        }
        Ok(())
    }

    async fn migrate_postgres_i64_columns_to_bigint(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        if self.kind != DatabaseKind::Postgres {
            return Ok(());
        }

        for &(table, column) in POSTGRES_I64_COLUMNS {
            let data_type = self
                .query(
                    "SELECT data_type FROM information_schema.columns
                     WHERE table_schema = current_schema() AND table_name = ? AND column_name = ?",
                )
                .bind(table)
                .bind(column)
                .fetch_optional(&mut **transaction)
                .await?
                .map(|row| row.try_get::<String, _>("data_type"))
                .transpose()?;
            match data_type.as_deref() {
                Some("bigint") => continue,
                Some("integer") | Some("smallint") => {}
                Some(other) => anyhow::bail!(
                    "cannot convert {table}.{column} from unexpected PostgreSQL type {other:?}"
                ),
                None => anyhow::bail!("cannot convert missing PostgreSQL column {table}.{column}"),
            }

            let sql = format!(
                "ALTER TABLE {table} ALTER COLUMN {column} TYPE BIGINT USING {column}::BIGINT"
            );
            sqlx::query(AssertSqlSafe(sql))
                .execute(&mut **transaction)
                .await?;
        }
        Ok(())
    }

    async fn create_blob_mutation_lock_table(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> anyhow::Result<()> {
        sqlx::query(BLOB_MUTATION_LOCK_TABLE)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    async fn add_column_if_missing(
        &self,
        transaction: &mut Transaction<'_, Any>,
        table: &str,
        column: &str,
        definition: &str,
    ) -> anyhow::Result<()> {
        if self.column_exists(transaction, table, column).await? {
            return Ok(());
        }
        let sql = format!("ALTER TABLE {table} ADD COLUMN {definition}");
        sqlx::query(AssertSqlSafe(sql))
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    async fn column_exists(
        &self,
        transaction: &mut Transaction<'_, Any>,
        table: &str,
        column: &str,
    ) -> anyhow::Result<bool> {
        let row = match self.kind {
            DatabaseKind::Sqlite => {
                self.query("SELECT 1 AS present FROM pragma_table_info(?) WHERE name = ?")
                    .bind(table)
                    .bind(column)
                    .fetch_optional(&mut **transaction)
                    .await?
            }
            DatabaseKind::Postgres => {
                self.query(
                    "SELECT 1 AS present FROM information_schema.columns
                     WHERE table_schema = current_schema() AND table_name = ? AND column_name = ?",
                )
                .bind(table)
                .bind(column)
                .fetch_optional(&mut **transaction)
                .await?
            }
        };
        Ok(row.is_some())
    }
}
