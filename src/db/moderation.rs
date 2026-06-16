use super::*;

impl Database {
    pub async fn create_report(
        &self,
        item_kind: &str,
        item_public_id: &str,
        reporter_user_id: Option<&str>,
        reason: &str,
        details: &str,
    ) -> anyhow::Result<()> {
        self.query(
            "INSERT INTO reports (id, item_kind, item_public_id, reporter_user_id, reason, details, state, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 'open', ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(item_kind)
        .bind(item_public_id)
        .bind(reporter_user_id)
        .bind(reason)
        .bind(details)
        .bind(util::now_ts())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_reports_filtered(
        &self,
        state: Option<&str>,
        item_kind: Option<&str>,
        reason: Option<&str>,
        created_after: Option<i64>,
    ) -> anyhow::Result<Vec<Report>> {
        let rows = self.query(
            "SELECT id, item_kind, item_public_id, reporter_user_id, reason, details, state, created_at
             FROM reports ORDER BY created_at DESC LIMIT 500",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut reports = rows
            .iter()
            .map(Report::from_row)
            .collect::<anyhow::Result<Vec<_>>>()?;
        reports.retain(|report| {
            state.is_none_or(|state| report.state == state)
                && item_kind.is_none_or(|item_kind| report.item_kind == item_kind)
                && reason.is_none_or(|reason| {
                    report
                        .reason
                        .to_lowercase()
                        .contains(&reason.to_lowercase())
                })
                && created_after.is_none_or(|created_after| report.created_at >= created_after)
        });
        reports.truncate(100);
        Ok(reports)
    }

    pub async fn reports_for_item(
        &self,
        item_kind: &str,
        item_public_id: &str,
    ) -> anyhow::Result<Vec<Report>> {
        let rows = self
            .query(
                "SELECT id, item_kind, item_public_id, reporter_user_id, reason, details, state, created_at
             FROM reports
             WHERE item_kind = ? AND item_public_id = ?
             ORDER BY created_at DESC LIMIT 100",
            )
            .bind(item_kind)
            .bind(item_public_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(Report::from_row).collect()
    }

    pub async fn report_by_id(&self, report_id: &str) -> anyhow::Result<Report> {
        let row = self.query(
            "SELECT id, item_kind, item_public_id, reporter_user_id, reason, details, state, created_at
             FROM reports WHERE id = ?",
        )
        .bind(report_id)
        .fetch_one(&self.pool)
        .await?;
        Report::from_row(&row)
    }

    pub async fn update_report_state(
        &self,
        report_id: &str,
        state: &str,
        actor_user_id: Option<&str>,
        detail: &str,
    ) -> anyhow::Result<()> {
        self.query("UPDATE reports SET state = ? WHERE id = ?")
            .bind(state)
            .bind(report_id)
            .execute(&self.pool)
            .await?;
        self.audit(actor_user_id, "report.updated", report_id, detail)
            .await?;
        Ok(())
    }

    pub async fn update_file_state_by_public_id(
        &self,
        public_id: &str,
        state: &str,
        actor_user_id: Option<&str>,
        detail: &str,
    ) -> anyhow::Result<bool> {
        let result = self
            .query("UPDATE files SET state = ? WHERE public_id = ?")
            .bind(state)
            .bind(public_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() > 0 {
            self.audit(actor_user_id, "file.state_updated", public_id, detail)
                .await?;
        }
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_paste_state_by_public_id(
        &self,
        public_id: &str,
        state: &str,
        actor_user_id: Option<&str>,
        detail: &str,
    ) -> anyhow::Result<bool> {
        let result = self
            .query("UPDATE pastes SET state = ? WHERE public_id = ?")
            .bind(state)
            .bind(public_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() > 0 {
            self.audit(actor_user_id, "paste.state_updated", public_id, detail)
                .await?;
        }
        Ok(result.rows_affected() > 0)
    }
}

impl Database {
    pub async fn record_scan_result(
        &self,
        item_kind: &str,
        item_public_id: &str,
        adapter: &str,
        decision: &str,
        detail: &str,
    ) -> anyhow::Result<()> {
        self.query(
            "INSERT INTO scanner_results (
                id, item_kind, item_public_id, adapter, decision, detail, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(item_kind)
        .bind(item_public_id)
        .bind(adapter)
        .bind(decision)
        .bind(detail)
        .bind(util::now_ts())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn scan_results_for_item(
        &self,
        item_kind: &str,
        item_public_id: &str,
    ) -> anyhow::Result<Vec<ScannerResult>> {
        let rows = self
            .query(
                "SELECT id, item_kind, item_public_id, adapter, decision, detail, created_at
             FROM scanner_results
             WHERE item_kind = ? AND item_public_id = ?
             ORDER BY created_at DESC LIMIT 100",
            )
            .bind(item_kind)
            .bind(item_public_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(ScannerResult::from_row).collect()
    }

    pub async fn audit_events_for_target(&self, target: &str) -> anyhow::Result<Vec<AuditEvent>> {
        let rows = self
            .query(
                "SELECT id, actor_user_id, action, target, detail, created_at
             FROM audit_events
             WHERE target = ?
             ORDER BY created_at DESC LIMIT 100",
            )
            .bind(target)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(AuditEvent::from_row).collect()
    }

    pub async fn add_moderation_note(
        &self,
        item_kind: &str,
        item_public_id: &str,
        report_id: Option<&str>,
        actor_user_id: Option<&str>,
        note: &str,
    ) -> anyhow::Result<()> {
        self.query(
            "INSERT INTO moderation_notes (
                id, item_kind, item_public_id, report_id, actor_user_id, note, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(item_kind)
        .bind(item_public_id)
        .bind(report_id)
        .bind(actor_user_id)
        .bind(note)
        .bind(util::now_ts())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn moderation_notes_for_item(
        &self,
        item_kind: &str,
        item_public_id: &str,
    ) -> anyhow::Result<Vec<ModerationNote>> {
        let rows = self
            .query(
                "SELECT id, item_kind, item_public_id, report_id, actor_user_id, note, created_at
             FROM moderation_notes
             WHERE item_kind = ? AND item_public_id = ?
             ORDER BY created_at DESC LIMIT 100",
            )
            .bind(item_kind)
            .bind(item_public_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(ModerationNote::from_row).collect()
    }
}
