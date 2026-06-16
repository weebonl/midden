use crate::{
    app::{AppError, AppResult},
    config::{QuotaConfig, RuntimeSettings},
    db::{Database, FileUsage, User},
};

pub async fn enforce_file_upload_quota(
    db: &Database,
    settings: &RuntimeSettings,
    user: Option<&User>,
    incoming_bytes: i64,
) -> AppResult<()> {
    let (quota, usage) = match user {
        Some(user) => {
            let Some(quota) = settings.limits.role_quotas.get(user.role.as_str()) else {
                return Ok(());
            };
            (quota, db.file_usage_for_user(Some(&user.id)).await?)
        }
        None => {
            let mut quota = settings.limits.anonymous_quota.clone();
            if quota.daily_upload_bytes.is_none() {
                quota.daily_upload_bytes = settings.limits.anonymous_daily_bytes;
            }
            if quota.is_empty() {
                return Ok(());
            }
            let usage = db.file_usage_for_user(None).await?;
            return check_quota(&quota, usage, incoming_bytes);
        }
    };
    check_quota(quota, usage, incoming_bytes)
}

fn check_quota(quota: &QuotaConfig, usage: FileUsage, incoming_bytes: i64) -> AppResult<()> {
    if exceeds(quota.storage_bytes, usage.storage_bytes, incoming_bytes) {
        return Err(AppError::PayloadTooLarge);
    }
    if exceeds(
        quota.daily_upload_bytes,
        usage.daily_upload_bytes,
        incoming_bytes,
    ) {
        return Err(AppError::PayloadTooLarge);
    }
    if exceeds(
        quota.monthly_upload_bytes,
        usage.monthly_upload_bytes,
        incoming_bytes,
    ) {
        return Err(AppError::PayloadTooLarge);
    }
    if let Some(item_count) = quota.item_count
        && usage.item_count + 1 > item_count
    {
        return Err(AppError::PayloadTooLarge);
    }
    Ok(())
}

fn exceeds(limit: Option<i64>, current: i64, incoming: i64) -> bool {
    limit.is_some_and(|limit| current.saturating_add(incoming) > limit)
}

trait EmptyQuota {
    fn is_empty(&self) -> bool;
}

impl EmptyQuota for QuotaConfig {
    fn is_empty(&self) -> bool {
        self.storage_bytes.is_none()
            && self.daily_upload_bytes.is_none()
            && self.monthly_upload_bytes.is_none()
            && self.item_count.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_byte_limits() {
        let quota = QuotaConfig {
            storage_bytes: Some(10),
            daily_upload_bytes: None,
            monthly_upload_bytes: None,
            item_count: None,
        };
        let usage = FileUsage {
            storage_bytes: 8,
            daily_upload_bytes: 0,
            monthly_upload_bytes: 0,
            item_count: 0,
        };
        assert!(check_quota(&quota, usage, 3).is_err());
        assert!(check_quota(&quota, usage, 2).is_ok());
    }
}
