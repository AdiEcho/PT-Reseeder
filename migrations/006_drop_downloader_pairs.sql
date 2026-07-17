-- 006: backfill from downloader_pairs, then drop pair column/table.
-- Do NOT rebuild tasks via DROP TABLE: child tables use ON DELETE CASCADE,
-- and sqlx wraps migrations in a transaction where PRAGMA foreign_keys=OFF is ignored.

-- 1) Destination backfill (idempotent with 005)
UPDATE tasks
SET destination_downloader_id = (
    SELECT destination_id
    FROM downloader_pairs
    WHERE downloader_pairs.id = tasks.downloader_pair_id
)
WHERE destination_downloader_id IS NULL
  AND downloader_pair_id IS NOT NULL;

-- 2) Source backfill only when task has zero source downloaders
INSERT INTO task_source_downloaders (task_id, downloader_id)
SELECT t.id, dp.source_id
FROM tasks t
JOIN downloader_pairs dp ON dp.id = t.downloader_pair_id
WHERE t.downloader_pair_id IS NOT NULL
  AND NOT EXISTS (
      SELECT 1 FROM task_source_downloaders tsd WHERE tsd.task_id = t.id
  )
  AND EXISTS (
      SELECT 1 FROM downloaders d WHERE d.id = dp.source_id
  );

-- 3) Drop legacy column (SQLite 3.35+)
ALTER TABLE tasks DROP COLUMN downloader_pair_id;

-- 4) Drop pair table last
DROP TABLE IF EXISTS downloader_pairs;
