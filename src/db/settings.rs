use super::*;
use serde::Serialize;
use std::collections::BTreeMap;

impl Database {
    pub async fn runtime_settings(&self, config: &AppConfig) -> anyhow::Result<RuntimeSettings> {
        let mut settings = RuntimeSettings::from_config(config);
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

        if let Some(value) = map
            .get("features")
            .filter(|_| !env_overrides_group("features"))
        {
            settings.features = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("limits").filter(|_| !env_overrides_group("limits")) {
            settings.limits = serde_json::from_str(value)?;
        }
        if let Some(value) = map
            .get("branding")
            .filter(|_| !env_overrides_group("branding"))
        {
            settings.branding = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("policy").filter(|_| !env_overrides_group("policy")) {
            settings.policy = serde_json::from_str(value)?;
        }
        if let Some(value) = map
            .get("security")
            .filter(|_| !env_overrides_group("security"))
        {
            settings.security = serde_json::from_str(value)?;
        }
        if let Some(value) = map
            .get("delivery")
            .filter(|_| !env_overrides_group("delivery"))
        {
            settings.delivery = serde_json::from_str(value)?;
        }
        if let Some(value) = map
            .get("scanning")
            .filter(|_| !env_overrides_group("scanning"))
        {
            settings.scanning = serde_json::from_str(value)?;
        }
        if let Some(value) = map
            .get("processing")
            .filter(|_| !env_overrides_group("processing"))
        {
            settings.processing = serde_json::from_str(value)?;
        }
        if let Some(value) = map
            .get("discovery")
            .filter(|_| !env_overrides_group("discovery"))
        {
            settings.discovery = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("jobs").filter(|_| !env_overrides_group("jobs")) {
            settings.jobs = serde_json::from_str(value)?;
        }
        if let Some(value) = map
            .get("uploads")
            .filter(|_| !env_overrides_group("uploads"))
        {
            settings.uploads = serde_json::from_str(value)?;
        }
        if let Some(value) = map
            .get("metrics")
            .filter(|_| !env_overrides_group("metrics"))
        {
            settings.metrics = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("tokens").filter(|_| !env_overrides_group("tokens")) {
            settings.tokens = serde_json::from_str(value)?;
        }
        if let Some(value) = map
            .get("moderation")
            .filter(|_| !env_overrides_group("moderation"))
        {
            settings.moderation = serde_json::from_str(value)?;
        }
        Ok(settings)
    }

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
}

fn env_overrides_group(group: &str) -> bool {
    std::env::vars_os().any(|(key, _)| env_key_overrides_group(group, &key.to_string_lossy()))
}

fn env_key_overrides_group(group: &str, key: &str) -> bool {
    let prefix = format!("MIDDEN__{}__", group.to_ascii_uppercase());
    key.starts_with(&prefix)
}

#[cfg(test)]
mod tests {
    #[test]
    fn env_override_detection_is_scoped_to_exact_runtime_setting_groups() {
        assert!(super::env_key_overrides_group(
            "features",
            "MIDDEN__FEATURES__API"
        ));
        assert!(super::env_key_overrides_group(
            "security",
            "MIDDEN__SECURITY__SECURE_COOKIES"
        ));
        assert!(!super::env_key_overrides_group(
            "features",
            "MIDDEN__FEATURE_FLAGS__API"
        ));
        assert!(!super::env_key_overrides_group(
            "features",
            "OTHER__FEATURES__API"
        ));
    }
}
