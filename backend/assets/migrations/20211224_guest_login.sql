ALTER TABLE site ADD allow_guest INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS hidden (
    hidden_id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    least_permission INTEGER NOT NULL DEFAULT 0
);