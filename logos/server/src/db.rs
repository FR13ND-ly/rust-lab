use common::FileMetadata;
use sqlx::{Pool, Postgres, Row};
use std::collections::HashMap;

pub async fn init_db(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS files (
            path TEXT PRIMARY KEY,
            size BIGINT NOT NULL,
            modified BIGINT NOT NULL,
            version BIGINT NOT NULL,
            hash TEXT NOT NULL,
            is_deleted BOOLEAN NOT NULL DEFAULT FALSE
        );
        "#
    )
    .execute(pool)
    .await?;
    
    Ok(())
}

pub async fn load_state(pool: &Pool<Postgres>) -> Result<HashMap<String, FileMetadata>, sqlx::Error> {
    let rows = sqlx::query("SELECT path, size, modified, version, hash, is_deleted FROM files")
        .fetch_all(pool)
        .await?;

    let mut map = HashMap::new();
    for row in rows {
        let meta = FileMetadata {
            path: row.get("path"),
            size: row.get::<i64, _>("size") as u64,
            modified: row.get::<i64, _>("modified") as u64,
            version: row.get::<i64, _>("version") as u64,
            hash: row.get("hash"),
            is_deleted: row.get("is_deleted"),
        };
        map.insert(meta.path.clone(), meta);
    }
    Ok(map)
}

pub async fn save_file(pool: &Pool<Postgres>, meta: &FileMetadata) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO files (path, size, modified, version, hash, is_deleted)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (path) DO UPDATE
        SET size = EXCLUDED.size,
            modified = EXCLUDED.modified,
            version = EXCLUDED.version,
            hash = EXCLUDED.hash,
            is_deleted = EXCLUDED.is_deleted
        "#
    )
    .bind(&meta.path)
    .bind(meta.size as i64)
    .bind(meta.modified as i64)
    .bind(meta.version as i64)
    .bind(&meta.hash)
    .bind(meta.is_deleted)
    .execute(pool)
    .await?;

    Ok(())
}