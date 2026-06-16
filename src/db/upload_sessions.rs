use super::*;

impl Database {
    pub async fn start_upload_session(&self, session: NewUploadSession<'_>) -> anyhow::Result<()> {
        self.query(
            "INSERT INTO upload_sessions (
                id, filename, content_type, total_bytes, received_bytes, owner_user_id,
                temp_path, state, expires_at, visibility, created_at, updated_at
             ) VALUES (?, ?, ?, ?, 0, ?, ?, 'open', ?, ?, ?, ?)",
        )
        .bind(session.upload_id)
        .bind(session.filename)
        .bind(session.content_type)
        .bind(session.total_bytes)
        .bind(session.owner_user_id)
        .bind(format!("data/uploads/{}.part", session.upload_id))
        .bind(session.expires_at)
        .bind(session.visibility)
        .bind(util::now_ts())
        .bind(util::now_ts())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upload_session(&self, upload_id: &str) -> anyhow::Result<UploadSession> {
        let row = self
            .query(
                "SELECT id, filename, content_type, total_bytes, received_bytes, owner_user_id,
                    temp_path, state, expires_at, visibility, created_at, updated_at
             FROM upload_sessions WHERE id = ?",
            )
            .bind(upload_id)
            .fetch_one(&self.pool)
            .await?;
        UploadSession::from_row(&row)
    }

    pub async fn update_upload_session_offset(
        &self,
        upload_id: &str,
        offset: i64,
    ) -> anyhow::Result<()> {
        self.query("UPDATE upload_sessions SET received_bytes = ?, updated_at = ? WHERE id = ?")
            .bind(offset)
            .bind(util::now_ts())
            .bind(upload_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn complete_upload_session(&self, upload_id: &str) -> anyhow::Result<()> {
        self.query("UPDATE upload_sessions SET state = 'complete', updated_at = ? WHERE id = ?")
            .bind(util::now_ts())
            .bind(upload_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn expired_upload_sessions(&self) -> anyhow::Result<Vec<UploadSession>> {
        let rows = self
            .query(
                "SELECT id, filename, content_type, total_bytes, received_bytes, owner_user_id,
                    temp_path, state, expires_at, visibility, created_at, updated_at
             FROM upload_sessions
             WHERE state != 'complete'
               AND (
                    (expires_at IS NOT NULL AND expires_at <= ?)
                 OR updated_at <= ?
               )",
            )
            .bind(util::now_ts())
            .bind(util::now_ts() - 60 * 60 * 24)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(UploadSession::from_row).collect()
    }

    pub async fn delete_upload_session(&self, upload_id: &str) -> anyhow::Result<()> {
        self.query("DELETE FROM upload_sessions WHERE id = ?")
            .bind(upload_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn cleanup_expired_auth_state(&self) -> anyhow::Result<u64> {
        let now = util::now_ts();
        let mut deleted = 0;
        deleted += self
            .query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?
            .rows_affected();
        deleted += self
            .query("DELETE FROM password_reset_tokens WHERE expires_at <= ? OR used_at IS NOT NULL")
            .bind(now)
            .execute(&self.pool)
            .await?
            .rows_affected();
        deleted += self
            .query("DELETE FROM email_verification_tokens WHERE expires_at <= ? OR used_at IS NOT NULL")
            .bind(now)
            .execute(&self.pool)
            .await?
            .rows_affected();
        deleted += self
            .query("DELETE FROM two_factor_challenges WHERE expires_at <= ? OR used_at IS NOT NULL")
            .bind(now)
            .execute(&self.pool)
            .await?
            .rows_affected();
        deleted += self
            .query("DELETE FROM invite_tokens WHERE used_at IS NULL AND expires_at IS NOT NULL AND expires_at <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(deleted)
    }
}
