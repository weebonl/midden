use sqlx::{Any, Row, Transaction};

use super::{Database, DatabaseKind, FileItem, NewFileItem};
use crate::util;

/// A database-backed critical section for one content-addressed blob.
///
/// The transaction intentionally remains open while callers perform object-storage I/O. Any
/// producer that can create a live blob reference and zero-reference cleanup must use this guard,
/// so a committed live reference can never race with deletion of its storage object.
pub struct BlobMutation {
    db: Database,
    transaction: Option<Transaction<'static, Any>>,
    hash: String,
    lock_identity: String,
}

impl Database {
    pub async fn begin_blob_mutation(&self, hash: &str) -> anyhow::Result<BlobMutation> {
        let hash = util::canonical_blob_hash(hash)?;
        let lock_identity = hash.clone();
        let mut transaction = self.pool.begin().await?;
        match self.kind {
            DatabaseKind::Postgres => {
                self.query("SELECT pg_advisory_xact_lock(?)")
                    .bind(blob_advisory_lock_key(&lock_identity))
                    .execute(&mut *transaction)
                    .await?;
            }
            DatabaseKind::Sqlite => {
                // SQLite permits only one writer. This ephemeral row forces the deferred
                // transaction to acquire and retain that write lock until commit or rollback.
                self.query(
                    "INSERT INTO blob_mutation_locks (hash) VALUES (?)
                     ON CONFLICT(hash) DO UPDATE SET hash = excluded.hash",
                )
                .bind(&lock_identity)
                .execute(&mut *transaction)
                .await?;
            }
        }
        Ok(BlobMutation {
            db: self.clone(),
            transaction: Some(transaction),
            hash,
            lock_identity,
        })
    }
}

impl BlobMutation {
    pub async fn create_blob_if_missing(
        &mut self,
        size_bytes: i64,
        content_type: Option<&str>,
    ) -> anyhow::Result<()> {
        let query = self.db.query(
            "INSERT INTO blobs (hash, size_bytes, content_type, ref_count, created_at)
             VALUES (?, ?, ?, 0, ?)
             ON CONFLICT(hash) DO NOTHING",
        );
        let hash = self.hash.clone();
        let transaction = self.transaction_mut()?;
        query
            .bind(hash)
            .bind(size_bytes)
            .bind(content_type)
            .bind(util::now_ts())
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    pub async fn create_file_item(&mut self, new: NewFileItem<'_>) -> anyhow::Result<FileItem> {
        let new_blob_hash = util::canonical_blob_hash(new.blob_hash)?;
        if new_blob_hash != self.hash {
            anyhow::bail!(
                "blob mutation for {:?} cannot create a reference to {:?}",
                self.hash,
                new.blob_hash
            );
        }
        let insert = self.db.query(
            "INSERT INTO files (
                id, public_id, blob_hash, original_filename, extension, content_type,
                size_bytes, image_width, image_height, owner_user_id, delete_token_hash, expires_at,
                visibility, metadata_json, thumbnail_hash, state, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );
        let increment = self
            .db
            .query("UPDATE blobs SET ref_count = ref_count + 1 WHERE hash = ?");
        let select = self.db.query(select_file_items!("WHERE public_id = ?"));
        let hash = self.hash.clone();
        let transaction = self.transaction_mut()?;
        insert
            .bind(new.id)
            .bind(new.public_id)
            .bind(&hash)
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
            .execute(&mut **transaction)
            .await?;

        let referenced = increment.bind(&hash).execute(&mut **transaction).await?;
        if referenced.rows_affected() != 1 {
            anyhow::bail!("cannot create file item without an existing blob record");
        }

        let row = select
            .bind(new.public_id)
            .fetch_one(&mut **transaction)
            .await?;
        FileItem::from_row(&row)
    }

    pub async fn attach_thumbnail(
        &mut self,
        file_public_id: &str,
        metadata_json: Option<&str>,
    ) -> anyhow::Result<bool> {
        let update = self.db.query(
            "UPDATE files
             SET metadata_json = COALESCE(metadata_json, ?), thumbnail_hash = ?
             WHERE public_id = ? AND state = 'active' AND thumbnail_hash IS NULL",
        );
        let hash = self.hash.clone();
        let transaction = self.transaction_mut()?;
        let result = update
            .bind(metadata_json)
            .bind(hash)
            .bind(file_public_id)
            .execute(&mut **transaction)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn is_unreferenced(&mut self) -> anyhow::Result<bool> {
        let query = self.db.query(
            "SELECT CAST(CASE WHEN
                 NOT EXISTS (SELECT 1 FROM blobs WHERE hash = ? AND ref_count > 0)
                 AND NOT EXISTS (
                     SELECT 1 FROM files
                     WHERE blob_hash = ? AND state NOT IN ('deleted', 'expired')
                 )
                 AND NOT EXISTS (
                     SELECT 1 FROM files
                     WHERE thumbnail_hash = ? AND state NOT IN ('deleted', 'expired')
                 )
             THEN 1 ELSE 0 END AS BIGINT) AS unreferenced",
        );
        let hash = self.hash.clone();
        let transaction = self.transaction_mut()?;
        Ok(query
            .bind(&hash)
            .bind(&hash)
            .bind(&hash)
            .fetch_one(&mut **transaction)
            .await?
            .try_get::<i64, _>("unreferenced")?
            != 0)
    }

    pub async fn delete_if_unreferenced(&mut self) -> anyhow::Result<bool> {
        let query = self.db.query(
            "DELETE FROM blobs
             WHERE hash = ? AND ref_count = 0
               AND NOT EXISTS (
                   SELECT 1 FROM files
                   WHERE files.blob_hash = blobs.hash
                     AND files.state NOT IN ('deleted', 'expired')
               )
               AND NOT EXISTS (
                   SELECT 1 FROM files
                   WHERE files.thumbnail_hash = blobs.hash
                     AND files.state NOT IN ('deleted', 'expired')
               )",
        );
        let hash = self.hash.clone();
        let transaction = self.transaction_mut()?;
        let result = query.bind(hash).execute(&mut **transaction).await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn commit(mut self) -> anyhow::Result<()> {
        let mut transaction = self
            .transaction
            .take()
            .ok_or_else(|| anyhow::anyhow!("blob mutation transaction is no longer active"))?;
        if self.db.kind == DatabaseKind::Sqlite {
            self.db
                .query("DELETE FROM blob_mutation_locks WHERE hash = ?")
                .bind(&self.lock_identity)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    fn transaction_mut(&mut self) -> anyhow::Result<&mut Transaction<'static, Any>> {
        self.transaction
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("blob mutation transaction is no longer active"))
    }
}

fn blob_advisory_lock_key(hash: &str) -> i64 {
    let digest = util::sha256_hex(hash.as_bytes());
    let mut bytes = [0_u8; 8];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&digest[offset..offset + 2], 16)
            .expect("SHA-256 output is valid hexadecimal");
    }
    i64::from_be_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advisory_lock_key_is_stable_and_hash_specific() {
        assert_eq!(
            blob_advisory_lock_key("same-hash"),
            blob_advisory_lock_key("same-hash")
        );
        assert_ne!(
            blob_advisory_lock_key("first-hash"),
            blob_advisory_lock_key("second-hash")
        );
    }
}
