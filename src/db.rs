use rusqlite::{Connection, Result};

pub struct DatabaseService {
    db_path: String,
}

impl DatabaseService {
    pub fn new(db_path: &str) -> Self {
        Self {
            db_path: db_path.to_string(),
        }
    }

    pub fn initialize_db(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS brokers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                username TEXT,
                password TEXT,
                tls_enabled BOOLEAN NOT NULL DEFAULT 0,
                max_reconnect_attempts INTEGER DEFAULT -1,
                reconnect_interval_ms INTEGER DEFAULT 5000
            );

            CREATE TABLE IF NOT EXISTS topics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                topic TEXT NOT NULL UNIQUE,
                parent_topic TEXT,
                max_values INTEGER NOT NULL,
                query_frequency_ms INTEGER NOT NULL,
                FOREIGN KEY (parent_topic) REFERENCES topics(topic) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS subscriptions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                broker_id INTEGER NOT NULL,
                topic_id INTEGER NOT NULL,
                is_active BOOLEAN NOT NULL DEFAULT 1,
                FOREIGN KEY (broker_id) REFERENCES brokers(id) ON DELETE CASCADE,
                FOREIGN KEY (topic_id) REFERENCES topics(id) ON DELETE CASCADE,
                UNIQUE (broker_id, topic_id)
            );

            CREATE TABLE IF NOT EXISTS topic_values (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                topic_id INTEGER NOT NULL,
                value TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (topic_id) REFERENCES topics(id) ON DELETE CASCADE
            );
            "#,
        )?;
        Ok(())
    }
}
