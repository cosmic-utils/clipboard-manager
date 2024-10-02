DROP TABLE IF EXISTS FavoriteClipboardEntries;

DROP TABLE IF EXISTS ClipboardEntries;

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

CREATE TABLE IF NOT EXISTS FavoriteClipboardEntries (
	id INTEGER PRIMARY KEY,
	position INTEGER NOT NULL,
	FOREIGN KEY (id) REFERENCES ClipboardEntries(creation) ON DELETE CASCADE,
	UNIQUE (position),
	CHECK (position >= 0)
);

INSERT INTO
	ClipboardEntries (creation, mime, content)
VALUES
	(1000, 'image/png', 'content1');

INSERT INTO
	ClipboardEntries (creation, mime, content)
VALUES
	(2000, 'image/png', 'content2');

INSERT INTO
	ClipboardEntries (creation, mime, content)
VALUES
	(3000, 'image/png', 'content3');

INSERT INTO
	ClipboardEntries (creation, mime, content)
VALUES
	(4000, 'image/png', 'content4');

INSERT INTO
	ClipboardEntries (creation, mime, content)
VALUES
	(5000, 'image/png', 'content5');

INSERT INTO
	FavoriteClipboardEntries (id, position)
VALUES
	(1000, 0);

INSERT INTO
	FavoriteClipboardEntries (id, position)
VALUES
	(2000, 1);

INSERT INTO
	FavoriteClipboardEntries (id, position)
VALUES
	(4000, 2);

UPDATE
	FavoriteClipboardEntries
SET
	position = position + 1
WHERE
	position >= 2;

INSERT INTO
	FavoriteClipboardEntries (id, position)
VALUES
	(3000, 2);