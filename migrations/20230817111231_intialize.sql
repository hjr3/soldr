CREATE TABLE IF NOT EXISTS requests (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     method TEXT NOT NULL,
     uri TEXT NOT NULL,
     headers TEXT NOT NULL,
     body TEXT,
     state INT(1) DEFAULT 0,
     created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS origins (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     domain TEXT NOT NULL,
     origin_uri TEXT NOT NULL,
     timeout INTEGER NOT NULL,
     created_at INTEGER NOT NULL,
     updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS attempts (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     request_id INTEGER,
     response_status INTEGER NOT NULL,
     response_body BLOB NOT NULL,
     created_at INTEGER NOT NULL
);
