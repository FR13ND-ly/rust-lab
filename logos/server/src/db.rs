use common::{FileMetadata, StorageInfo};
use sqlx::{Pool, Postgres, Row};
use std::collections::HashMap;
use uuid::Uuid;

pub async fn init_db(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS storages (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name TEXT UNIQUE NOT NULL,
            created_at TIMESTAMPTZ DEFAULT NOW()
        );
        "#
    ).execute(pool).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS files (
            storage_id UUID NOT NULL REFERENCES storages(id),
            path TEXT NOT NULL,
            size BIGINT NOT NULL,
            modified BIGINT NOT NULL,
            version BIGINT NOT NULL,
            hash TEXT NOT NULL,
            is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
            last_modified_by TEXT,
            PRIMARY KEY (storage_id, path)
        );
        "#
    ).execute(pool).await?;

    sqlx::query("ALTER TABLE files ADD COLUMN IF NOT EXISTS last_modified_by TEXT")
        .execute(pool)
        .await?;
    
    Ok(())
}

pub async fn list_storages(pool: &Pool<Postgres>) -> Result<Vec<StorageInfo>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, name FROM storages ORDER BY name ASC")
        .fetch_all(pool)
        .await?;

    let mut storages = Vec::new();
    for r in rows {
        storages.push(StorageInfo {
            id: r.try_get::<Uuid, _>("id")?.to_string(),
            name: r.try_get("name")?,
        });
    }
    Ok(storages)
}

pub async fn create_storage(pool: &Pool<Postgres>, name: &str) -> Result<StorageInfo, sqlx::Error> {
    let row = sqlx::query("INSERT INTO storages (name) VALUES ($1) RETURNING id, name")
        .bind(name)
        .fetch_one(pool)
        .await?;

    Ok(StorageInfo {
        id: row.try_get::<Uuid, _>("id")?.to_string(),
        name: row.try_get("name")?,
    })
}

pub async fn delete_storage(pool: &Pool<Postgres>, storage_id: &str) -> Result<(), sqlx::Error> {
    let uuid = Uuid::parse_str(storage_id)
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

    let mut tx = pool.begin().await?;
    
    sqlx::query("DELETE FROM files WHERE storage_id = $1")
        .bind(uuid)
        .execute(&mut *tx)
        .await?;
        
    sqlx::query("DELETE FROM storages WHERE id = $1")
        .bind(uuid)
        .execute(&mut *tx)
        .await?;
        
    tx.commit().await?;
    Ok(())
}

pub async fn load_storage_files(pool: &Pool<Postgres>, storage_id: &str) -> Result<HashMap<String, FileMetadata>, sqlx::Error> {
    let uuid = Uuid::parse_str(storage_id)
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    
    let rows = sqlx::query("SELECT path, size, modified, version, hash, is_deleted, last_modified_by FROM files WHERE storage_id = $1")
        .bind(uuid)
        .fetch_all(pool)
        .await?;

    let mut map = HashMap::new();
    for row in rows {
        let meta = FileMetadata {
            path: row.try_get("path")?,
            size: row.try_get::<i64, _>("size")? as u64,
            modified: row.try_get::<i64, _>("modified")? as u64,
            version: row.try_get::<i64, _>("version")? as u64,
            hash: row.try_get("hash")?,
            is_deleted: row.try_get("is_deleted")?,
            last_modified_by: row.try_get("last_modified_by")?,
        };
        map.insert(meta.path.clone(), meta);
    }
    Ok(map)
}

pub async fn save_file(pool: &Pool<Postgres>, storage_id: &str, meta: &FileMetadata) -> Result<(), sqlx::Error> {
    let uuid = Uuid::parse_str(storage_id)
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO files (storage_id, path, size, modified, version, hash, is_deleted, last_modified_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (storage_id, path) DO UPDATE
        SET size = EXCLUDED.size,
            modified = EXCLUDED.modified,
            version = EXCLUDED.version,
            hash = EXCLUDED.hash,
            is_deleted = EXCLUDED.is_deleted,
            last_modified_by = EXCLUDED.last_modified_by
        "#
    )
    .bind(uuid)
    .bind(&meta.path)
    .bind(meta.size as i64)
    .bind(meta.modified as i64)
    .bind(meta.version as i64)
    .bind(&meta.hash)
    .bind(meta.is_deleted)
    .bind(&meta.last_modified_by)
    .execute(pool)
    .await?;

    Ok(())
}