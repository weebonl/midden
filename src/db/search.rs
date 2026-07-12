use super::*;

impl Database {
    pub async fn recent_user_files(&self, user_id: &str) -> anyhow::Result<Vec<FileItem>> {
        let rows = self
            .query(select_file_items!(
                "WHERE owner_user_id = ? ORDER BY created_at DESC LIMIT 100"
            ))
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(FileItem::from_row).collect()
    }

    pub async fn search_user_files(
        &self,
        user_id: &str,
        query: &str,
    ) -> anyhow::Result<Vec<FileItem>> {
        let pattern = format!("%{}%", query.to_lowercase());
        let rows = self
            .query(select_file_items!(
                "WHERE owner_user_id = ?
               AND (
                    lower(public_id) LIKE ?
                 OR lower(COALESCE(original_filename, '')) LIKE ?
                 OR lower(COALESCE(content_type, '')) LIKE ?
               )
             ORDER BY created_at DESC LIMIT 100"
            ))
            .bind(user_id)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(FileItem::from_row).collect()
    }

    pub async fn recent_user_pastes(&self, user_id: &str) -> anyhow::Result<Vec<Paste>> {
        let rows = self
            .query(select_pastes!(
                "WHERE owner_user_id = ? ORDER BY created_at DESC LIMIT 100"
            ))
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(Paste::from_row).collect()
    }

    pub async fn search_user_pastes(
        &self,
        user_id: &str,
        query: &str,
        include_content: bool,
    ) -> anyhow::Result<Vec<Paste>> {
        let pattern = format!("%{}%", query.to_lowercase());
        let rows = if include_content {
            self.query(select_pastes!(
                "WHERE owner_user_id = ?
               AND (
                    lower(public_id) LIKE ?
                 OR lower(COALESCE(title, '')) LIKE ?
                 OR lower(COALESCE(syntax, '')) LIKE ?
                 OR lower(content) LIKE ?
               )
             ORDER BY created_at DESC LIMIT 100"
            ))
            .bind(user_id)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await?
        } else {
            self.query(select_pastes!(
                "WHERE owner_user_id = ?
               AND (
                    lower(public_id) LIKE ?
                 OR lower(COALESCE(title, '')) LIKE ?
                 OR lower(COALESCE(syntax, '')) LIKE ?
               )
             ORDER BY created_at DESC LIMIT 100"
            ))
            .bind(user_id)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await?
        };
        rows.iter().map(Paste::from_row).collect()
    }

    pub async fn admin_search_files(&self, query: &str) -> anyhow::Result<Vec<FileItem>> {
        let pattern = format!("%{}%", query.to_lowercase());
        let rows = self
            .query(select_file_items!(
                "WHERE lower(public_id) LIKE ?
                OR lower(COALESCE(original_filename, '')) LIKE ?
                OR lower(COALESCE(content_type, '')) LIKE ?
                OR lower(blob_hash) LIKE ?
             ORDER BY created_at DESC LIMIT 100"
            ))
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(FileItem::from_row).collect()
    }

    pub async fn admin_search_pastes(
        &self,
        query: &str,
        include_content: bool,
    ) -> anyhow::Result<Vec<Paste>> {
        let pattern = format!("%{}%", query.to_lowercase());
        let rows = if include_content {
            self.query(select_pastes!(
                "WHERE lower(public_id) LIKE ?
                    OR lower(COALESCE(title, '')) LIKE ?
                    OR lower(COALESCE(syntax, '')) LIKE ?
                    OR lower(content) LIKE ?
                 ORDER BY created_at DESC LIMIT 100"
            ))
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await?
        } else {
            self.query(select_pastes!(
                "WHERE lower(public_id) LIKE ?
                    OR lower(COALESCE(title, '')) LIKE ?
                    OR lower(COALESCE(syntax, '')) LIKE ?
                 ORDER BY created_at DESC LIMIT 100"
            ))
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await?
        };
        rows.iter().map(Paste::from_row).collect()
    }

    pub async fn public_files(
        &self,
        query: Option<&str>,
        before: Option<i64>,
        limit: i64,
    ) -> anyhow::Result<Vec<FileItem>> {
        let pattern = query.map(|query| format!("%{}%", query.to_lowercase()));
        let before = before.unwrap_or(i64::MAX);
        let rows = if let Some(pattern) = pattern.as_deref() {
            self.query(select_file_items!(
                "WHERE state = 'active'
                   AND visibility = 'public'
                   AND created_at < ?
                   AND (expires_at IS NULL OR expires_at > ?)
                   AND (
                        lower(public_id) LIKE ?
                     OR lower(COALESCE(original_filename, '')) LIKE ?
                     OR lower(COALESCE(content_type, '')) LIKE ?
                   )
                 ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(before)
            .bind(util::now_ts())
            .bind(pattern)
            .bind(pattern)
            .bind(pattern)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            self.query(select_file_items!(
                "WHERE state = 'active'
                   AND visibility = 'public'
                   AND created_at < ?
                   AND (expires_at IS NULL OR expires_at > ?)
                 ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(before)
            .bind(util::now_ts())
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        rows.iter().map(FileItem::from_row).collect()
    }

    pub async fn public_pastes(
        &self,
        query: Option<&str>,
        before: Option<i64>,
        limit: i64,
    ) -> anyhow::Result<Vec<Paste>> {
        let pattern = query.map(|query| format!("%{}%", query.to_lowercase()));
        let before = before.unwrap_or(i64::MAX);
        let rows = if let Some(pattern) = pattern.as_deref() {
            self.query(select_pastes!(
                "WHERE state = 'active'
                   AND visibility = 'public'
                   AND created_at < ?
                   AND (expires_at IS NULL OR expires_at > ?)
                   AND (
                        lower(public_id) LIKE ?
                     OR lower(COALESCE(title, '')) LIKE ?
                     OR lower(COALESCE(syntax, '')) LIKE ?
                   )
                 ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(before)
            .bind(util::now_ts())
            .bind(pattern)
            .bind(pattern)
            .bind(pattern)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            self.query(select_pastes!(
                "WHERE state = 'active'
                   AND visibility = 'public'
                   AND created_at < ?
                   AND (expires_at IS NULL OR expires_at > ?)
                 ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(before)
            .bind(util::now_ts())
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        rows.iter().map(Paste::from_row).collect()
    }
}
