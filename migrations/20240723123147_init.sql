CREATE TABLE IF NOT EXISTS ClipboardEntries (
    creation INTEGER PRIMARY KEY,
    mime TEXT NOT NULL,
    content BLOB NOT NULL,
    metadataMime TEXT,
    metadata TEXT,
    CHECK (
        (
            metadataMime IS NULL
            AND metadata IS NULL
        )
        OR (
            metadataMime IS NOT NULL
            AND metadata IS NOT NULL
        )
    )
);