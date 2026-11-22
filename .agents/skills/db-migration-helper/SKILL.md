---
name: db-migration-helper
description: >-
  Use this skill when modifying the SQLite database schema, adding new tables,
  columns, or indexes, or writing migrations in manga-tui. Ensures proper versioning in
  migration.rs, updates database.rs, syncs database_scheme.md, maintains compatibility
  with Google Drive Cloud Sync (library_sync.json), and writes unit tests.
---

# Database Migration Helper Skill

This skill guides you through safely modifying the SQLite schema, creating incremental migrations, and keeping cloud/offline synchronization aligned in `manga-tui`.

---

## Migration & Persistence Architecture

In `manga-tui`:
- Database operations and fresh table initialization live in `src/backend/database.rs`.
- Migration engine and incremental versions live in `src/backend/migration.rs`.
- Database documentation lives in `src/backend/database_scheme.md`.
- Cloud synchronization snapshot and bidirectional merge live in `src/backend/cloud.rs` (`LibrarySyncSnapshot`).
- Offline catalog metadata reads live in `src/backend/offline.rs` and `src/backend/database.rs` (`OfflineCatalogMetadata`).
- Migrations run automatically at startup via `update_database_with_migrations` called from `src/main.rs`.

---

## 1. Migration Checklist

Whenever you modify table structures or add indexes:

1. **Define the Migration in `src/backend/migration.rs`**:
   Write a new `migrate_version_X_Y_Z` function with the target version string. You can use `Query::AlterTable` for adding columns or `Query::Raw` for creating tables/indexes:
   ```rust
   /// Add column `new_col` to table `chapters` or create an index
   fn migrate_version_0_11_0(connection: &mut Connection, logger: &impl ILogger) -> rusqlite::Result<Option<MigrationTable>> {
       let queries = [
           Query::AlterTable {
               table_name: "chapters",
               command: AlterTableCommand::Add {
                   column: "new_col",
                   data_type: "TEXT NULL",
               },
           },
           Query::Raw("CREATE INDEX IF NOT EXISTS idx_chapters_new_col ON chapters(new_col)"),
       ];

       let migration = Migration::new(&queries)
           .with_name("Add column new_col and index to table chapters")
           .with_version("0.11.0")
           .up(connection)?;

       let migration_result = match migration {
           Some(available_migration) => {
               logger.inform("Updating database");
               let migration_result = available_migration.update(connection)?;
               logger.inform("Database updated");
               Some(migration_result)
           },
           None => None,
       };

       Ok(migration_result)
   }
   ```

2. **Register in `update_database_with_migrations` (`src/backend/migration.rs`)**:
   Append the new migration call to the vector in `update_database_with_migrations`:
   ```rust
   pub fn update_database_with_migrations(
       connection: &mut Connection,
       logger: &impl ILogger,
   ) -> rusqlite::Result<Vec<Option<MigrationTable>>> {
       let migrations: Vec<Option<MigrationTable>> = vec![
           migrate_version_0_4_0(connection, logger)?,
           migrate_version_0_6_0(connection, logger)?,
           migrate_version_0_10_1(connection, logger)?,
           migrate_version_0_11_0(connection, logger)?, // <- Added here
       ];

       Ok(migrations)
   }
   ```

3. **Update Fresh Schema Initialization in `src/backend/database.rs`**:
   Fresh installs run the complete `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS` statements in `Database::setup()`. Ensure new columns, tables, or indexes are present there too.

4. **Check Cloud Sync Compatibility (`src/backend/cloud.rs`)**:
   If the new column or table represents user library state (reading history, bookmarks, favorites, plan-to-read, dismissed chapter notifications, or search history):
   - Add the corresponding field (with `#[serde(default)]` for backwards compatibility with older cloud snapshots) to `SyncMangaRecord`, `SyncChapterRecord`, or `LibrarySyncSnapshot` in `src/backend/cloud.rs`.
   - Update export (`export_library_snapshot`) and import/merge (`merge_library_snapshot_into_db`) logic in `src/backend/cloud.rs`.

5. **Update Documentation in `src/backend/database_scheme.md`**:
   Keep table definitions, indexes, and field comments synchronized.

6. **Write Unit Tests in `src/backend/migration.rs`**:
   Add a test verifying:
   - Migration runs on a legacy in-memory database.
   - Column or index exists after migration (`PRAGMA table_info` / `sqlite_master`).
   - Running the migration a second time is an idempotent no-op (`Ok(None)`).

---

## 2. Verification & Sync

Run the migration, database, and cloud sync test suites:
```powershell
cargo test backend::migration -- --nocapture
cargo test backend::database -- --nocapture
cargo test backend::cloud -- --nocapture
```
Then format, install, and sync locally:
```powershell
cargo fmt --all
cargo install --path .
.\scripts\sync-local-install.ps1
```
