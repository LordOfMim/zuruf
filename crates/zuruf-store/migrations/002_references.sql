BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS entity_links(child_kind TEXT NOT NULL, child_id TEXT NOT NULL, parent_kind TEXT NOT NULL, parent_id TEXT NOT NULL, FOREIGN KEY(child_kind,child_id) REFERENCES entities(kind,id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED, FOREIGN KEY(parent_kind,parent_id) REFERENCES entities(kind,id) DEFERRABLE INITIALLY DEFERRED);
CREATE UNIQUE INDEX IF NOT EXISTS draft_per_review ON entities(json_extract(body,'$.reviewId')) WHERE kind='batch' AND json_extract(body,'$.state')='draft';
CREATE UNIQUE INDEX IF NOT EXISTS task_per_batch ON entities(json_extract(body,'$.batchId')) WHERE kind='task';
INSERT OR IGNORE INTO migrations VALUES(2);
COMMIT;
