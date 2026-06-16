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

        if let Some(value) = map.get("features") {
            settings.features = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("limits") {
            settings.limits = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("branding") {
            settings.branding = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("policy") {
            settings.policy = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("security") {
            settings.security = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("delivery") {
            settings.delivery = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("scanning") {
            settings.scanning = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("processing") {
            settings.processing = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("discovery") {
            settings.discovery = serde_json::from_str(value)?;
        }
        if let Some(value) = map.get("jobs") {
            settings.jobs = serde_json::from_str(value)?;
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
