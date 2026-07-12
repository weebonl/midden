use super::*;
#[cfg(test)]
use serde::Serialize;
use std::collections::BTreeMap;

impl Database {
    pub async fn runtime_settings(&self, config: &AppConfig) -> anyhow::Result<RuntimeSettings> {
        let mut settings = self.persisted_runtime_settings(config).await?;
        apply_runtime_env_overrides(&mut settings, config)?;
        config.validate_runtime_settings(&settings)?;
        Ok(settings)
    }

    pub async fn persisted_runtime_settings(
        &self,
        config: &AppConfig,
    ) -> anyhow::Result<RuntimeSettings> {
        let mut settings = config
            .runtime_settings_base
            .as_deref()
            .cloned()
            .unwrap_or_else(|| RuntimeSettings::from_config(config));
        let rows = self
            .query("SELECT key, value FROM settings")
            .fetch_all(&self.pool)
            .await?;
        let map = rows
            .into_iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>("key")?,
                    row.try_get::<String, _>("value")?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, sqlx::Error>>()?;
        for (key, value) in map {
            settings.apply_group_json(&key, &value)?;
        }
        Ok(settings)
    }

    pub fn restore_environment_owned_fields(
        &self,
        settings: &mut RuntimeSettings,
        persisted: &RuntimeSettings,
        config: &AppConfig,
    ) -> anyhow::Result<()> {
        copy_runtime_env_paths(settings, persisted, config)
    }

    pub fn apply_runtime_environment(
        &self,
        settings: &mut RuntimeSettings,
        config: &AppConfig,
    ) -> anyhow::Result<()> {
        apply_runtime_env_overrides(settings, config)
    }

    #[cfg(test)]
    pub async fn set_json_setting<T: Serialize>(&self, key: &str, value: &T) -> anyhow::Result<()> {
        let encoded = serde_json::to_string_pretty(value)?;
        self.query(
            "INSERT INTO settings (key, value, updated_at)
             VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(encoded)
        .bind(util::now_ts())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn replace_runtime_settings(
        &self,
        settings: &RuntimeSettings,
        actor_user_id: Option<&str>,
        detail: &str,
    ) -> anyhow::Result<()> {
        let groups = settings.serialized_groups()?;
        let now = util::now_ts();
        let mut transaction = self.pool.begin().await?;
        for (key, value) in groups {
            self.query(
                "INSERT INTO settings (key, value, updated_at)
                 VALUES (?, ?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            )
            .bind(key)
            .bind(value)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }
        self.query(
            "INSERT INTO audit_events (id, actor_user_id, action, target, detail, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(actor_user_id)
        .bind("settings.updated")
        .bind("settings")
        .bind(detail)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}

fn apply_runtime_env_overrides(
    settings: &mut RuntimeSettings,
    config: &AppConfig,
) -> anyhow::Result<()> {
    copy_runtime_env_paths(settings, &RuntimeSettings::from_config(config), config)
}

fn copy_runtime_env_paths(
    settings: &mut RuntimeSettings,
    source: &RuntimeSettings,
    config: &AppConfig,
) -> anyhow::Result<()> {
    let source = serde_json::to_value(source)?;
    let mut target = serde_json::to_value(&*settings)?;
    for path in config.runtime_env_overrides.paths() {
        let value = json_path(&source, path).ok_or_else(|| {
            anyhow::anyhow!(
                "runtime environment override path {} was not present in loaded configuration",
                path.join(".")
            )
        })?;
        set_json_path(&mut target, path, value.clone())?;
    }
    *settings = serde_json::from_value(target)?;
    Ok(())
}

fn json_path<'a>(value: &'a serde_json::Value, path: &[String]) -> Option<&'a serde_json::Value> {
    path.iter()
        .try_fold(value, |current, segment| current.get(segment))
}

fn set_json_path(
    target: &mut serde_json::Value,
    path: &[String],
    value: serde_json::Value,
) -> anyhow::Result<()> {
    let Some((last, parents)) = path.split_last() else {
        anyhow::bail!("runtime environment override path must not be empty");
    };
    let mut current = target;
    for segment in parents {
        let object = current.as_object_mut().ok_or_else(|| {
            anyhow::anyhow!(
                "runtime environment override {} is not an object",
                path.join(".")
            )
        })?;
        current = object
            .entry(segment.clone())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }
    let object = current.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!(
            "runtime environment override {} is not an object",
            path.join(".")
        )
    })?;
    object.insert(last.clone(), value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, RuntimeEnvOverrides, RuntimeSettings};
    use std::collections::BTreeSet;

    async fn test_db() -> Database {
        let mut config = AppConfig::default();
        config.database.url = "sqlite::memory:".to_string();
        config.database.max_connections = 1;
        let db = Database::connect(&config).await.unwrap();
        db.migrate().await.unwrap();
        db
    }

    #[test]
    fn field_level_environment_overrides_preserve_other_persisted_fields() {
        let mut config = AppConfig::default();
        config.security.secure_cookies = true;
        config.runtime_env_overrides = RuntimeEnvOverrides::from_keys([
            "MIDDEN__SECURITY__SECURE_COOKIES",
            "MIDDEN__FEATURES__API",
            "MIDDEN__SERVER__BIND",
            "OTHER__FEATURES__FILES",
        ]);
        let mut settings = RuntimeSettings::from_config(&AppConfig::default());
        settings.security.secure_cookies = false;
        settings.security.reject_mime_mismatch = true;
        settings.features.api = false;
        settings.features.files = false;

        super::apply_runtime_env_overrides(&mut settings, &config).unwrap();

        assert!(settings.security.secure_cookies);
        assert!(settings.security.reject_mime_mismatch);
        assert!(settings.features.api);
        assert!(!settings.features.files);
    }

    #[tokio::test]
    async fn database_merge_applies_only_explicit_environment_fields_last() {
        let db = test_db().await;
        let mut config = AppConfig::default();
        config.security.secure_cookies = true;
        config.runtime_env_overrides =
            RuntimeEnvOverrides::from_keys(["MIDDEN__SECURITY__SECURE_COOKIES"]);
        let mut persisted = config.security.clone();
        persisted.secure_cookies = false;
        persisted.reject_mime_mismatch = true;
        db.set_json_setting("security", &persisted).await.unwrap();

        let settings = db.runtime_settings(&config).await.unwrap();

        assert!(settings.security.secure_cookies);
        assert!(settings.security.reject_mime_mismatch);
    }

    #[tokio::test]
    async fn admin_replacement_does_not_persist_environment_owned_values() {
        let db = test_db().await;
        let mut config = AppConfig::default();
        let mut file_baseline = RuntimeSettings::from_config(&config);
        file_baseline.limits.max_upload_bytes = 111;
        config.runtime_settings_base = Some(Box::new(file_baseline));
        config.limits.max_upload_bytes = 999;
        config.runtime_env_overrides =
            RuntimeEnvOverrides::from_keys(["MIDDEN__LIMITS__MAX_UPLOAD_BYTES"]);

        let mut persisted_limits = config.limits.clone();
        persisted_limits.max_upload_bytes = 222;
        db.set_json_setting("limits", &persisted_limits)
            .await
            .unwrap();
        let persisted = db.persisted_runtime_settings(&config).await.unwrap();
        let mut form_candidate = db.runtime_settings(&config).await.unwrap();
        assert_eq!(persisted.limits.max_upload_bytes, 222);
        assert_eq!(form_candidate.limits.max_upload_bytes, 999);

        form_candidate.branding.tagline = "unrelated admin edit".to_string();
        db.restore_environment_owned_fields(&mut form_candidate, &persisted, &config)
            .unwrap();
        db.replace_runtime_settings(&form_candidate, None, "admin regression")
            .await
            .unwrap();

        let stored_after_save = db.persisted_runtime_settings(&config).await.unwrap();
        assert_eq!(stored_after_save.limits.max_upload_bytes, 222);
        assert_eq!(stored_after_save.branding.tagline, "unrelated admin edit");
        assert_eq!(
            db.runtime_settings(&config)
                .await
                .unwrap()
                .limits
                .max_upload_bytes,
            999
        );
    }

    #[tokio::test]
    async fn persisted_runtime_settings_use_central_semantic_validation() {
        let db = test_db().await;
        let config = AppConfig::default();
        let mut jobs = config.jobs.clone();
        jobs.interval_seconds = 1;
        db.set_json_setting("jobs", &jobs).await.unwrap();

        let error = db.runtime_settings(&config).await.unwrap_err().to_string();

        assert!(error.contains("jobs.interval_seconds"), "{error}");
    }

    #[tokio::test]
    async fn replaces_every_runtime_group_and_audit_event_in_one_operation() {
        let db = test_db().await;
        let config = AppConfig::default();
        let mut settings = RuntimeSettings::from_config(&config);
        settings.features.api = false;
        settings.branding.instance_name = "Atomic settings".to_string();

        db.replace_runtime_settings(&settings, None, "test replacement")
            .await
            .unwrap();

        let rows = db
            .query("SELECT key FROM settings ORDER BY key")
            .fetch_all(&db.pool)
            .await
            .unwrap();
        let actual_keys = rows
            .iter()
            .map(|row| row.try_get::<String, _>("key").unwrap())
            .collect::<BTreeSet<_>>();
        let expected_keys = RuntimeSettings::GROUP_KEYS
            .iter()
            .map(|key| (*key).to_string())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual_keys, expected_keys);
        let row = db
            .query(
                "SELECT action, target, detail FROM audit_events WHERE action = 'settings.updated'",
            )
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(row.try_get::<String, _>("target").unwrap(), "settings");
        assert_eq!(
            row.try_get::<String, _>("detail").unwrap(),
            "test replacement"
        );
        let loaded = db.runtime_settings(&config).await.unwrap();
        assert!(!loaded.features.api);
        assert_eq!(loaded.branding.instance_name, "Atomic settings");
    }

    #[tokio::test]
    async fn runtime_settings_replacement_rolls_back_every_group_and_audit_on_failure() {
        let db = test_db().await;
        db.query(
            "CREATE TRIGGER fail_metrics_setting
             BEFORE INSERT ON settings
             WHEN NEW.key = 'metrics'
             BEGIN
               SELECT RAISE(ABORT, 'injected settings failure');
             END",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        let settings = RuntimeSettings::from_config(&AppConfig::default());

        assert!(
            db.replace_runtime_settings(&settings, None, "must roll back")
                .await
                .is_err()
        );

        let row = db
            .query("SELECT COUNT(*) AS count FROM settings")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(row.try_get::<i64, _>("count").unwrap(), 0);
        let row = db
            .query("SELECT COUNT(*) AS count FROM audit_events")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(row.try_get::<i64, _>("count").unwrap(), 0);
    }
}
