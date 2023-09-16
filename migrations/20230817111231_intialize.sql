CREATE TABLE IF NOT EXISTS origins (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     domain TEXT NOT NULL,
     origin_uri TEXT NOT NULL,
     timeout INTEGER NOT NULL,
     alert_threshold SMALLINT,
     alert_email TEXT,
     smtp_host TEXT,
     smtp_username TEXT,
     smtp_password TEXT,
     smtp_port SMALLINT,
     smtp_tls INT(1) NOT NULL DEFAULT 0,
     created_at INTEGER NOT NULL,
     updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS requests (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     method TEXT NOT NULL,
     uri TEXT NOT NULL,
     headers TEXT NOT NULL,
     body TEXT,
     state INT(1) NOT NULL DEFAULT 0,
     created_at INTEGER NOT NULL,
     retry_ms_at INTEGER     
);
CREATE INDEX request_state_retry ON requests(state, retry_ms_at);
CREATE INDEX request_state_created ON requests(state, created_at);

CREATE TABLE IF NOT EXISTS attempts (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     request_id INTEGER REFERENCES requests(id) ON DELETE CASCADE,
     response_status INTEGER NOT NULL,
     response_body BLOB NOT NULL,
     created_at INTEGER NOT NULL
);

CREATE INDEX attempt_req_id ON attempts(request_id);
