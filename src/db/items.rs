use super::*;

impl Database {
    pub async fn blob_hashes(&self) -> anyhow::Result<Vec<String>> {
        let rows = self
            .query(
                "SELECT hash FROM blobs
                 WHERE ref_count > 0
                    OR EXISTS (
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

    #[cfg(test)]
    pub async fn blob_ref_count(&self, hash: &str) -> anyhow::Result<i64> {
        let row = self
            .query("SELECT ref_count FROM blobs WHERE hash = ?")
            .bind(hash)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get("ref_count")?)
    }

    #[cfg(test)]
    pub async fn create_blob_if_missing(
        &self,
        hash: &str,
        size_bytes: i64,
        content_type: Option<&str>,
    ) -> anyhow::Result<()> {
        self.query(
            "INSERT INTO blobs (hash, size_bytes, content_type, ref_count, created_at)
             VALUES (?, ?, ?, 0, ?)
             ON CONFLICT(hash) DO NOTHING",
        )
        .bind(hash)
        .bind(size_bytes)
        .bind(content_type)
        .bind(util::now_ts())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[cfg(test)]
    pub async fn create_file_item(&self, new: NewFileItem<'_>) -> anyhow::Result<FileItem> {
        let mut transaction = self.pool.begin().await?;
        self.query(
            "INSERT INTO files (
                id, public_id, blob_hash, original_filename, extension, content_type,
                size_bytes, image_width, image_height, owner_user_id, delete_token_hash, expires_at,
                visibility, metadata_json, thumbnail_hash, state, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new.id)
        .bind(new.public_id)
        .bind(new.blob_hash)
        .bind(new.original_filename)
        .bind(new.extension)
        .bind(new.content_type)
        .bind(new.size_bytes)
        .bind(new.image_width)
        .bind(new.image_height)
        .bind(new.owner_user_id)
        .bind(new.delete_token_hash)
        .bind(new.expires_at)
        .bind(new.visibility)
        .bind(new.metadata_json)
        .bind(new.thumbnail_hash)
        .bind(new.state)
        .bind(util::now_ts())
        .execute(&mut *transaction)
        .await?;
        let referenced = self
            .query("UPDATE blobs SET ref_count = ref_count + 1 WHERE hash = ?")
            .bind(new.blob_hash)
            .execute(&mut *transaction)
            .await?;
        if referenced.rows_affected() != 1 {
            transaction.rollback().await?;
            anyhow::bail!("cannot create file item without an existing blob record");
        }
        let row = self
            .query(select_file_items!("WHERE public_id = ?"))
            .bind(new.public_id)
            .fetch_one(&mut *transaction)
            .await?;
        let file = FileItem::from_row(&row)?;
        transaction.commit().await?;
        Ok(file)
    }

    pub async fn file_by_public_id(&self, public_id: &str) -> anyhow::Result<FileItem> {
        let row = self
            .query(select_file_items!("WHERE public_id = ?"))
            .bind(public_id)
            .fetch_one(&self.pool)
            .await?;
        FileItem::from_row(&row)
    }

    pub async fn active_file_by_public_id(
        &self,
        public_id: &str,
    ) -> anyhow::Result<Option<FileItem>> {
        let row = self
            .query(select_file_items!(
                "WHERE public_id = ? AND state = 'active' AND (expires_at IS NULL OR expires_at > ?)"
            ))
        .bind(public_id)
        .bind(util::now_ts())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| FileItem::from_row(&row)).transpose()
    }

    pub async fn claim_file_by_public_id(
        &self,
        public_id: &str,
        user_id: &str,
        token_hash: &str,
    ) -> anyhow::Result<bool> {
        let result = self
            .query(
                "UPDATE files SET owner_user_id = ?
                 WHERE public_id = ?
                   AND owner_user_id IS NULL
                   AND delete_token_hash = ?
                   AND state = 'active'",
            )
            .bind(user_id)
            .bind(public_id)
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() > 0 {
            self.audit(Some(user_id), "file.claimed", public_id, "delete token")
                .await?;
        }
        Ok(result.rows_affected() > 0)
    }

    pub async fn claim_paste_by_public_id(
        &self,
        public_id: &str,
        user_id: &str,
        token_hash: &str,
    ) -> anyhow::Result<bool> {
        let result = self
            .query(
                "UPDATE pastes SET owner_user_id = ?
                 WHERE public_id = ?
                   AND owner_user_id IS NULL
                   AND delete_token_hash = ?
                   AND state = 'active'",
            )
            .bind(user_id)
            .bind(public_id)
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() > 0 {
            self.audit(Some(user_id), "paste.claimed", public_id, "delete token")
                .await?;
        }
        Ok(result.rows_affected() > 0)
    }

    pub async fn create_paste(&self, new: NewPaste<'_>) -> anyhow::Result<Paste> {
        self.query(
            "INSERT INTO pastes (
                id, public_id, title, content, syntax, owner_user_id, delete_token_hash,
                expires_at, visibility, state, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?)",
        )
        .bind(new.id)
        .bind(new.public_id)
        .bind(new.title)
        .bind(new.content)
        .bind(new.syntax)
        .bind(new.owner_user_id)
        .bind(new.delete_token_hash)
        .bind(new.expires_at)
        .bind(new.visibility)
        .bind(util::now_ts())
        .execute(&self.pool)
        .await?;
        self.paste_by_public_id(new.public_id).await
    }

    pub async fn paste_by_public_id(&self, public_id: &str) -> anyhow::Result<Paste> {
        let row = self
            .query(select_pastes!(
                "WHERE public_id = ? AND state = 'active' AND (expires_at IS NULL OR expires_at > ?)"
            ))
        .bind(public_id)
        .bind(util::now_ts())
        .fetch_one(&self.pool)
        .await?;
        Paste::from_row(&row)
    }

    pub async fn paste_by_public_id_any(&self, public_id: &str) -> anyhow::Result<Paste> {
        let row = self
            .query(select_pastes!("WHERE public_id = ?"))
            .bind(public_id)
            .fetch_one(&self.pool)
            .await?;
        Paste::from_row(&row)
    }

    pub async fn update_paste(
        &self,
        paste_id: &str,
        title: Option<&str>,
        content: &str,
        syntax: Option<&str>,
        actor_user_id: Option<&str>,
    ) -> anyhow::Result<Paste> {
        let current = self.paste_by_id(paste_id).await?;
        self.query(
            "INSERT INTO paste_revisions (
                id, paste_id, title, content, syntax, created_by_user_id, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(paste_id)
        .bind(current.title.as_deref())
        .bind(&current.content)
        .bind(current.syntax.as_deref())
        .bind(actor_user_id)
        .bind(util::now_ts())
        .execute(&self.pool)
        .await?;
        self.query("UPDATE pastes SET title = ?, content = ?, syntax = ? WHERE id = ?")
            .bind(title)
            .bind(content)
            .bind(syntax)
            .bind(paste_id)
            .execute(&self.pool)
            .await?;
        self.audit(
            actor_user_id,
            "paste.updated",
            &current.public_id,
            "paste edit",
        )
        .await?;
        self.paste_by_id(paste_id).await
    }

    pub async fn paste_revision_count(&self, paste_id: &str) -> anyhow::Result<i64> {
        let row = self
            .query("SELECT COUNT(*) AS count FROM paste_revisions WHERE paste_id = ?")
            .bind(paste_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get("count")?)
    }

    pub async fn expired_files(&self) -> anyhow::Result<Vec<FileItem>> {
        let rows = self
            .query(
                select_file_items!(
                    "WHERE state IN ('active', 'quarantined') AND expires_at IS NOT NULL AND expires_at <= ?"
                ),
            )
            .bind(util::now_ts())
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(FileItem::from_row).collect()
    }

    pub async fn expire_due_pastes(&self) -> anyhow::Result<u64> {
        let result = self
            .query(
                "UPDATE pastes SET state = 'expired'
             WHERE state = 'active' AND expires_at IS NOT NULL AND expires_at <= ?",
            )
            .bind(util::now_ts())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn expired_paste_count(&self) -> anyhow::Result<i64> {
        let row = self
            .query(
                "SELECT COUNT(*) AS count
             FROM pastes WHERE state = 'active' AND expires_at IS NOT NULL AND expires_at <= ?",
            )
            .bind(util::now_ts())
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get("count")?)
    }

    pub async fn paste_by_id(&self, id: &str) -> anyhow::Result<Paste> {
        let row = self
            .query(select_pastes!("WHERE id = ?"))
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Paste::from_row(&row)
    }
}

impl Database {
    #[cfg(test)]
    pub async fn install_file_insert_failure_for_test(&self) -> anyhow::Result<()> {
        self.query(
            "CREATE TRIGGER fail_file_publication
             BEFORE INSERT ON files
             BEGIN
               SELECT RAISE(ABORT, 'injected file publication failure');
             END",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_file_metadata(
        &self,
        public_id: &str,
        metadata_json: Option<&str>,
    ) -> anyhow::Result<()> {
        self.query(
            "UPDATE files SET metadata_json = COALESCE(metadata_json, ?) WHERE public_id = ?",
        )
        .bind(metadata_json)
        .bind(public_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn files_needing_processing(
        &self,
        metadata: bool,
        thumbnails: bool,
        limit: i64,
    ) -> anyhow::Result<Vec<FileItem>> {
        let rows = match (metadata, thumbnails) {
            (true, true) => {
                self.query(
                    select_file_items!(
                        "WHERE state = 'active'
                       AND (expires_at IS NULL OR expires_at > ?)
                       AND (
                            metadata_json IS NULL
                         OR (thumbnail_hash IS NULL AND content_type IN ('image/jpeg', 'image/png', 'image/gif'))
                       )
                     ORDER BY created_at ASC LIMIT ?"
                    ),
                )
                .bind(util::now_ts())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (true, false) => {
                self.query(
                    select_file_items!(
                        "WHERE state = 'active'
                       AND (expires_at IS NULL OR expires_at > ?)
                       AND metadata_json IS NULL
                     ORDER BY created_at ASC LIMIT ?"
                    ),
                )
                .bind(util::now_ts())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (false, true) => {
                self.query(
                    select_file_items!(
                        "WHERE state = 'active'
                       AND (expires_at IS NULL OR expires_at > ?)
                       AND thumbnail_hash IS NULL
                       AND content_type IN ('image/jpeg', 'image/png', 'image/gif')
                     ORDER BY created_at ASC LIMIT ?"
                    ),
                )
                .bind(util::now_ts())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (false, false) => Vec::new(),
        };
        rows.iter().map(FileItem::from_row).collect()
    }

    pub async fn scanner_retry_file_candidates(&self, limit: i64) -> anyhow::Result<Vec<FileItem>> {
        let rows = self
            .query(select_file_items!(
                "WHERE state IN ('active', 'quarantined')
                   AND EXISTS (
                     SELECT 1 FROM scanner_results failed
                      WHERE failed.item_kind = 'file'
                        AND failed.item_public_id = files.public_id
                        AND (
                             lower(failed.detail) LIKE '%failed%'
                          OR lower(failed.detail) LIKE '%returned http%'
                          OR lower(failed.detail) LIKE '%invalid webhook%'
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM scanner_results resolved
                             WHERE resolved.item_kind = failed.item_kind
                               AND resolved.item_public_id = failed.item_public_id
                               AND resolved.adapter = failed.adapter
                               AND resolved.created_at >= failed.created_at
                               AND lower(resolved.detail) NOT LIKE '%failed%'
                               AND lower(resolved.detail) NOT LIKE '%returned http%'
                               AND lower(resolved.detail) NOT LIKE '%invalid webhook%'
                        )
                   )
                 ORDER BY created_at ASC LIMIT ?"
            ))
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(FileItem::from_row).collect()
    }

    pub async fn file_usage_for_user(&self, user_id: Option<&str>) -> anyhow::Result<FileUsage> {
        let day_start = util::now_ts() - 60 * 60 * 24;
        let month_start = util::now_ts() - 60 * 60 * 24 * 30;
        let row = if let Some(user_id) = user_id {
            self.query(
                "SELECT
                    CAST(COALESCE(SUM(CASE WHEN state IN ('active', 'quarantined') THEN size_bytes ELSE 0 END), 0) AS BIGINT) AS storage_bytes,
                    CAST(COALESCE(SUM(CASE WHEN created_at >= ? THEN size_bytes ELSE 0 END), 0) AS BIGINT) AS daily_upload_bytes,
                    CAST(COALESCE(SUM(CASE WHEN created_at >= ? THEN size_bytes ELSE 0 END), 0) AS BIGINT) AS monthly_upload_bytes,
                    CAST(COALESCE(SUM(CASE WHEN state IN ('active', 'quarantined') THEN 1 ELSE 0 END), 0) AS BIGINT) AS item_count
                 FROM files WHERE owner_user_id = ?",
            )
            .bind(day_start)
            .bind(month_start)
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?
        } else {
            self.query(
                "SELECT
                    CAST(COALESCE(SUM(CASE WHEN state IN ('active', 'quarantined') THEN size_bytes ELSE 0 END), 0) AS BIGINT) AS storage_bytes,
                    CAST(COALESCE(SUM(CASE WHEN created_at >= ? THEN size_bytes ELSE 0 END), 0) AS BIGINT) AS daily_upload_bytes,
                    CAST(COALESCE(SUM(CASE WHEN created_at >= ? THEN size_bytes ELSE 0 END), 0) AS BIGINT) AS monthly_upload_bytes,
                    CAST(COALESCE(SUM(CASE WHEN state IN ('active', 'quarantined') THEN 1 ELSE 0 END), 0) AS BIGINT) AS item_count
                 FROM files WHERE owner_user_id IS NULL",
            )
            .bind(day_start)
            .bind(month_start)
            .fetch_one(&self.pool)
            .await?
        };
        Ok(FileUsage {
            storage_bytes: row.try_get("storage_bytes")?,
            daily_upload_bytes: row.try_get("daily_upload_bytes")?,
            monthly_upload_bytes: row.try_get("monthly_upload_bytes")?,
            item_count: row.try_get("item_count")?,
        })
    }
}
