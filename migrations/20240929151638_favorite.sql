CREATE TABLE IF NOT EXISTS FavoriteClipboardEntries (
    id INTEGER PRIMARY KEY,
    position INTEGER NOT NULL,
	FOREIGN KEY (id) REFERENCES ClipboardEntries(creation) ON DELETE CASCADE,
    -- UNIQUE (position),
    CHECK (position >= 0)
);