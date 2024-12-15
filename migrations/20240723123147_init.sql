CREATE TABLE IF NOT EXISTS ClipboardEntries (
    id INTEGER PRIMARY KEY,
    creation INTEGER
);

CREATE INDEX IF NOT EXISTS index_creation ON ClipboardEntries (creation);

CREATE TABLE IF NOT EXISTS ClipboardContents (
    id INTEGER PRIMARY KEY,
    mime TEXT NOT NULL,
    content BLOB NOT NULL,
	FOREIGN KEY (id) REFERENCES ClipboardEntries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS FavoriteClipboardEntries (
    id INTEGER PRIMARY KEY,
    position INTEGER NOT NULL,
	FOREIGN KEY (id) REFERENCES ClipboardEntries(id) ON DELETE CASCADE,
    -- UNIQUE (position),
    CHECK (position >= 0)
);
