CREATE TABLE IF NOT EXISTS ClipboardEntries (
    creation INTEGER PRIMARY KEY,
);

CREATE TABLE IF NOT EXISTS FavoriteClipboardEntries (
    id INTEGER PRIMARY KEY,
    position INTEGER NOT NULL,
	FOREIGN KEY (id) REFERENCES ClipboardEntries(creation) ON DELETE CASCADE,
    -- UNIQUE (position),
    CHECK (position >= 0)
);

CREATE TABLE IF NOT EXISTS ClipboardContents (
    id INTEGER PRIMARY KEY,
    mime TEXT NOT NULL,
    content BLOB NOT NULL,
	FOREIGN KEY (id) REFERENCES ClipboardEntries(creation) ON DELETE CASCADE,
);