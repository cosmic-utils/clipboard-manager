DROP TABLE IF EXISTS data;

CREATE TABLE data (
    creation INTEGER PRIMARY KEY,
    mime TEXT NOT NULL,
    content BLOB NOT NULL
);


INSERT INTO data (creation, mime, content)
VALUES (1000, 'image/png', 'content');

WITH last_row AS (
	SELECT creation, mime, content
	FROM data
	ORDER BY creation DESC
	LIMIT 1
)
INSERT INTO data (creation, mime, content)
SELECT 1500, 'image/png', 'content'
WHERE NOT EXISTS (
	SELECT 1
	FROM last_row AS lr
	WHERE lr.content = 'content' AND (1500 - lr.creation) <= 1000
);
