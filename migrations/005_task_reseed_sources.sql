-- Destination downloader for reseed write target (create-task primary path)
ALTER TABLE tasks ADD COLUMN destination_downloader_id INTEGER REFERENCES downloaders(id);

-- Multi source downloaders for API/torrent_dir hash acquisition
CREATE TABLE task_source_downloaders (
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    downloader_id INTEGER NOT NULL REFERENCES downloaders(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, downloader_id)
);

-- Optional backfill: if old tasks only had pair, copy destination
UPDATE tasks
SET destination_downloader_id = (
    SELECT destination_id FROM downloader_pairs WHERE downloader_pairs.id = tasks.downloader_pair_id
)
WHERE destination_downloader_id IS NULL
  AND downloader_pair_id IS NOT NULL;
