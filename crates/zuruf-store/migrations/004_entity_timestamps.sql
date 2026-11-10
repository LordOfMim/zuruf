BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS entity_timestamps (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(kind, id)
);
INSERT OR IGNORE INTO migrations VALUES(4);
COMMIT;
