use super::*;
use crate::domain::{
    AccountBulkAction, AccountBulkPlan, AccountBulkResult, ItemKind, ItemModerationOutcome,
    ItemModerationPlan, ItemState,
};

#[derive(Clone, Copy)]
enum FileReleaseTransition {
    Delete,
    Expire,
}

impl Database {
    pub async fn delete_paste(
        &self,
        paste_id: &str,
        actor_user_id: Option<&str>,
        reason: &str,
    ) -> anyhow::Result<Paste> {
        let mut transaction = self.pool.begin().await?;
        let row = self
            .query(select_pastes!("WHERE id = ?"))
            .bind(paste_id)
            .fetch_one(&mut *transaction)
            .await?;
        let paste = Paste::from_row(&row)?;
        let updated = self
            .query("UPDATE pastes SET state = 'deleted' WHERE id = ? AND state != 'deleted'")
            .bind(paste_id)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() > 0 {
            insert_item_audit(
                self,
                &mut transaction,
                actor_user_id,
                "paste.deleted",
                paste_id,
                reason,
            )
            .await?;
        }
        transaction.commit().await?;
        Ok(paste)
    }

    pub async fn apply_account_bulk(
        &self,
        plan: &AccountBulkPlan,
    ) -> anyhow::Result<Option<AccountBulkResult>> {
        let mut transaction = self.pool.begin().await?;
        let mut files = Vec::with_capacity(plan.file_ids.len());
        for public_id in &plan.file_ids {
            let row = self
                .query("SELECT id, blob_hash, owner_user_id FROM files WHERE public_id = ?")
                .bind(public_id)
                .fetch_optional(&mut *transaction)
                .await?;
            let Some(row) = row else {
                transaction.rollback().await?;
                return Ok(None);
            };
            let owner = row.try_get::<Option<String>, _>("owner_user_id")?;
            let owner_allowed = owner.as_deref() == Some(plan.owner_user_id.as_str())
                || (matches!(plan.action, AccountBulkAction::Delete)
                    && plan.allow_delete_any_owner);
            if !owner_allowed {
                transaction.rollback().await?;
                return Ok(None);
            }
            files.push((
                public_id.clone(),
                row.try_get::<String, _>("id")?,
                row.try_get::<String, _>("blob_hash")?,
            ));
        }

        let mut pastes = Vec::with_capacity(plan.paste_ids.len());
        for public_id in &plan.paste_ids {
            let row = self
                .query("SELECT id, owner_user_id FROM pastes WHERE public_id = ?")
                .bind(public_id)
                .fetch_optional(&mut *transaction)
                .await?;
            let Some(row) = row else {
                transaction.rollback().await?;
                return Ok(None);
            };
            let owner = row.try_get::<Option<String>, _>("owner_user_id")?;
            let owner_allowed = owner.as_deref() == Some(plan.owner_user_id.as_str())
                || (matches!(plan.action, AccountBulkAction::Delete)
                    && plan.allow_delete_any_owner);
            if !owner_allowed {
                transaction.rollback().await?;
                return Ok(None);
            }
            pastes.push((public_id.clone(), row.try_get::<String, _>("id")?));
        }

        let mut result = AccountBulkResult::default();
        match plan.action {
            AccountBulkAction::Delete => {
                for (_, internal_id, blob_hash) in &files {
                    let updated = self
                        .query(
                            "UPDATE files SET state = 'deleted'
                             WHERE id = ? AND state IN ('active', 'quarantined')",
                        )
                        .bind(internal_id)
                        .execute(&mut *transaction)
                        .await?;
                    if updated.rows_affected() > 0 {
                        self.query(
                            "UPDATE blobs SET ref_count = CASE WHEN ref_count > 0 THEN ref_count - 1 ELSE 0 END WHERE hash = ?",
                        )
                        .bind(blob_hash)
                        .execute(&mut *transaction)
                        .await?;
                        insert_item_audit(
                            self,
                            &mut transaction,
                            Some(&plan.owner_user_id),
                            "file.deleted",
                            internal_id,
                            "account bulk delete",
                        )
                        .await?;
                        let ref_count = self
                            .query("SELECT ref_count FROM blobs WHERE hash = ?")
                            .bind(blob_hash)
                            .fetch_optional(&mut *transaction)
                            .await?
                            .map(|row| row.try_get::<i64, _>("ref_count"))
                            .transpose()?;
                        if ref_count == Some(0) {
                            result.zero_ref_blob_hashes.push(blob_hash.clone());
                        }
                    }
                }
                for (_, internal_id) in &pastes {
                    let updated = self
                        .query(
                            "UPDATE pastes SET state = 'deleted' WHERE id = ? AND state = 'active'",
                        )
                        .bind(internal_id)
                        .execute(&mut *transaction)
                        .await?;
                    if updated.rows_affected() > 0 {
                        insert_item_audit(
                            self,
                            &mut transaction,
                            Some(&plan.owner_user_id),
                            "paste.deleted",
                            internal_id,
                            "account bulk delete",
                        )
                        .await?;
                    }
                }
            }
            AccountBulkAction::SetVisibility(visibility) => {
                for (public_id, ..) in &files {
                    self.query("UPDATE files SET visibility = ? WHERE public_id = ?")
                        .bind(visibility.as_str())
                        .bind(public_id)
                        .execute(&mut *transaction)
                        .await?;
                }
                for (public_id, _) in &pastes {
                    self.query("UPDATE pastes SET visibility = ? WHERE public_id = ?")
                        .bind(visibility.as_str())
                        .bind(public_id)
                        .execute(&mut *transaction)
                        .await?;
                }
            }
            AccountBulkAction::SetExpiry {
                file_expires_at,
                paste_expires_at,
            } => {
                for (public_id, ..) in &files {
                    self.query("UPDATE files SET expires_at = ? WHERE public_id = ?")
                        .bind(file_expires_at)
                        .bind(public_id)
                        .execute(&mut *transaction)
                        .await?;
                }
                for (public_id, _) in &pastes {
                    self.query("UPDATE pastes SET expires_at = ? WHERE public_id = ?")
                        .bind(paste_expires_at)
                        .bind(public_id)
                        .execute(&mut *transaction)
                        .await?;
                }
            }
        }

        insert_item_audit(
            self,
            &mut transaction,
            Some(&plan.owner_user_id),
            "account.bulk_items",
            &plan.owner_user_id,
            match plan.action {
                AccountBulkAction::Delete => "delete",
                AccountBulkAction::SetVisibility(_) => "set_visibility",
                AccountBulkAction::SetExpiry { .. } => "set_expiry",
            },
        )
        .await?;
        transaction.commit().await?;
        result.zero_ref_blob_hashes.sort();
        result.zero_ref_blob_hashes.dedup();
        Ok(Some(result))
    }

    pub async fn zero_ref_blob_hashes(&self) -> anyhow::Result<Vec<String>> {
        let rows = self
            .query(
                "SELECT hash FROM blobs
                 WHERE ref_count = 0
                   AND NOT EXISTS (
                       SELECT 1 FROM files
                       WHERE files.thumbnail_hash = blobs.hash
                         AND files.state NOT IN ('deleted', 'expired')
                   )
                 ORDER BY hash",
            )
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(|row| Ok(row.try_get("hash")?)).collect()
    }

    pub async fn delete_file_and_release_blob(
        &self,
        file_id: &str,
        actor_user_id: Option<&str>,
        reason: &str,
    ) -> anyhow::Result<Option<String>> {
        self.transition_file_and_release_blob(
            file_id,
            FileReleaseTransition::Delete,
            actor_user_id,
            reason,
        )
        .await
    }

    pub async fn expire_file_and_release_blob(
        &self,
        file_id: &str,
    ) -> anyhow::Result<Option<String>> {
        self.transition_file_and_release_blob(
            file_id,
            FileReleaseTransition::Expire,
            None,
            "scheduled expiry",
        )
        .await
    }

    async fn transition_file_and_release_blob(
        &self,
        file_id: &str,
        transition: FileReleaseTransition,
        actor_user_id: Option<&str>,
        reason: &str,
    ) -> anyhow::Result<Option<String>> {
        let mut transaction = self.pool.begin().await?;
        let row = self
            .query("SELECT blob_hash FROM files WHERE id = ?")
            .bind(file_id)
            .fetch_optional(&mut *transaction)
            .await?;
        let Some(row) = row else {
            transaction.rollback().await?;
            return Ok(None);
        };
        let blob_hash = row.try_get::<String, _>("blob_hash")?;
        let (update_sql, audit_action) = match transition {
            FileReleaseTransition::Delete => (
                "UPDATE files SET state = 'deleted'
                 WHERE id = ? AND state IN ('active', 'quarantined')",
                "file.deleted",
            ),
            FileReleaseTransition::Expire => (
                "UPDATE files SET state = 'expired'
                 WHERE id = ?
                   AND state IN ('active', 'quarantined')
                   AND expires_at IS NOT NULL
                   AND expires_at <= ?",
                "file.expired",
            ),
        };
        let mut update = self.query(update_sql).bind(file_id);
        if matches!(transition, FileReleaseTransition::Expire) {
            update = update.bind(util::now_ts());
        }
        let updated = update.execute(&mut *transaction).await?;
        if updated.rows_affected() == 0 {
            transaction.commit().await?;
            return Ok(None);
        }

        self.query(
            "UPDATE blobs
             SET ref_count = CASE WHEN ref_count > 0 THEN ref_count - 1 ELSE 0 END
             WHERE hash = ?",
        )
        .bind(&blob_hash)
        .execute(&mut *transaction)
        .await?;
        let ref_count = self
            .query("SELECT ref_count FROM blobs WHERE hash = ?")
            .bind(&blob_hash)
            .fetch_one(&mut *transaction)
            .await?
            .try_get::<i64, _>("ref_count")?;
        insert_item_audit(
            self,
            &mut transaction,
            actor_user_id,
            audit_action,
            file_id,
            reason,
        )
        .await?;
        transaction.commit().await?;
        Ok((ref_count == 0).then_some(blob_hash))
    }

    pub async fn apply_item_moderation(
        &self,
        plan: &ItemModerationPlan,
        actor_user_id: Option<&str>,
        detail: &str,
    ) -> anyhow::Result<ItemModerationOutcome> {
        let mut transaction = self.pool.begin().await?;
        let (blocked_hash, current_file_state) = match plan.kind {
            ItemKind::File => {
                let select = if self.kind == DatabaseKind::Postgres {
                    "SELECT blob_hash, state FROM files WHERE public_id = ? FOR UPDATE"
                } else {
                    "SELECT blob_hash, state FROM files WHERE public_id = ?"
                };
                match self
                    .query(select)
                    .bind(&plan.public_id)
                    .fetch_optional(&mut *transaction)
                    .await?
                {
                    Some(row) => (
                        Some(row.try_get::<String, _>("blob_hash")?),
                        Some(row.try_get::<String, _>("state")?),
                    ),
                    None => (None, None),
                }
            }
            ItemKind::Paste => {
                let exists = self
                    .query("SELECT id FROM pastes WHERE public_id = ?")
                    .bind(&plan.public_id)
                    .fetch_optional(&mut *transaction)
                    .await?
                    .is_some();
                (exists.then(String::new), None)
            }
        };
        if blocked_hash.is_none() {
            transaction.rollback().await?;
            return Ok(ItemModerationOutcome::NotFound);
        }

        let mut zero_ref_blob_hash = None;

        if let Some(item_state) = plan.state {
            if plan.kind == ItemKind::File {
                let current_state = current_file_state.as_deref().unwrap_or_default();
                let current_is_terminal = matches!(current_state, "deleted" | "expired");
                let target_is_terminal = item_state == ItemState::Deleted;
                if current_is_terminal && !target_is_terminal {
                    transaction.rollback().await?;
                    return Ok(ItemModerationOutcome::TerminalFileTransition);
                }
            }
            let (sql, audit_action) = match plan.kind {
                ItemKind::File => (
                    "UPDATE files SET state = ? WHERE public_id = ?",
                    "file.state_updated",
                ),
                ItemKind::Paste => (
                    "UPDATE pastes SET state = ? WHERE public_id = ?",
                    "paste.state_updated",
                ),
            };
            self.query(sql)
                .bind(item_state.as_str())
                .bind(&plan.public_id)
                .execute(&mut *transaction)
                .await?;
            insert_item_audit(
                self,
                &mut transaction,
                actor_user_id,
                audit_action,
                &plan.public_id,
                detail,
            )
            .await?;

            if plan.kind == ItemKind::File
                && item_state == ItemState::Deleted
                && !matches!(current_file_state.as_deref(), Some("deleted" | "expired"))
            {
                let blob_hash = blocked_hash.as_deref().unwrap_or_default();
                let released = self
                    .query(
                        "UPDATE blobs
                         SET ref_count = CASE WHEN ref_count > 0 THEN ref_count - 1 ELSE 0 END
                         WHERE hash = ?",
                    )
                    .bind(blob_hash)
                    .execute(&mut *transaction)
                    .await?;
                if released.rows_affected() != 1 {
                    anyhow::bail!("cannot delete a live file without its blob record");
                }
                let ref_count = self
                    .query("SELECT ref_count FROM blobs WHERE hash = ?")
                    .bind(blob_hash)
                    .fetch_one(&mut *transaction)
                    .await?
                    .try_get::<i64, _>("ref_count")?;
                if ref_count == 0 {
                    zero_ref_blob_hash = Some(blob_hash.to_string());
                }
            }
        }

        if let Some(visibility) = plan.visibility {
            let (sql, audit_action) = match plan.kind {
                ItemKind::File => (
                    "UPDATE files SET visibility = ? WHERE public_id = ?",
                    "file.visibility_updated",
                ),
                ItemKind::Paste => (
                    "UPDATE pastes SET visibility = ? WHERE public_id = ?",
                    "paste.visibility_updated",
                ),
            };
            self.query(sql)
                .bind(visibility.as_str())
                .bind(&plan.public_id)
                .execute(&mut *transaction)
                .await?;
            insert_item_audit(
                self,
                &mut transaction,
                actor_user_id,
                audit_action,
                &plan.public_id,
                detail,
            )
            .await?;
        }

        if let Some(note) = plan.note.as_deref() {
            self.query(
                "INSERT INTO moderation_notes (
                    id, item_kind, item_public_id, report_id, actor_user_id, note, created_at
                 ) VALUES (?, ?, ?, NULL, ?, ?, ?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(plan.kind.as_str())
            .bind(&plan.public_id)
            .bind(actor_user_id)
            .bind(note)
            .bind(util::now_ts())
            .execute(&mut *transaction)
            .await?;
        }

        if plan.block_hash {
            let blocked_hash = blocked_hash.as_deref().unwrap_or_default();
            let fallback = plan.scanning_fallback.clone().unwrap_or_default();
            self.query(
                "INSERT INTO settings (key, value, updated_at) VALUES ('scanning', ?, ?)
                 ON CONFLICT(key) DO NOTHING",
            )
            .bind(serde_json::to_string_pretty(&fallback)?)
            .bind(util::now_ts())
            .execute(&mut *transaction)
            .await?;
            let select_sql = if self.kind == DatabaseKind::Postgres {
                "SELECT value FROM settings WHERE key = 'scanning' FOR UPDATE"
            } else {
                "SELECT value FROM settings WHERE key = 'scanning'"
            };
            let persisted = self
                .query(select_sql)
                .fetch_optional(&mut *transaction)
                .await?
                .map(|row| row.try_get::<String, _>("value"))
                .transpose()?;
            let mut scanning = match persisted {
                Some(value) => serde_json::from_str::<crate::config::ScanningConfig>(&value)?,
                None => fallback,
            };
            if !scanning
                .blocked_hashes
                .iter()
                .any(|hash| hash.eq_ignore_ascii_case(blocked_hash))
            {
                scanning.blocked_hashes.push(blocked_hash.to_string());
                let scanning_json = serde_json::to_string_pretty(&scanning)?;
                self.query(
                    "INSERT INTO settings (key, value, updated_at)
                     VALUES ('scanning', ?, ?)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                )
                .bind(scanning_json)
                .bind(util::now_ts())
                .execute(&mut *transaction)
                .await?;
                insert_item_audit(
                    self,
                    &mut transaction,
                    actor_user_id,
                    "scanner.blocked_hash_added",
                    &plan.public_id,
                    blocked_hash,
                )
                .await?;
            }
        }

        transaction.commit().await?;
        Ok(ItemModerationOutcome::Applied { zero_ref_blob_hash })
    }
}

async fn insert_item_audit(
    db: &Database,
    transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    actor_user_id: Option<&str>,
    action: &str,
    target: &str,
    detail: &str,
) -> anyhow::Result<()> {
    db.query(
        "INSERT INTO audit_events (id, actor_user_id, action, target, detail, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(actor_user_id)
    .bind(action)
    .bind(target)
    .bind(detail)
    .bind(util::now_ts())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
