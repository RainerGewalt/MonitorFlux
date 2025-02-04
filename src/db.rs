use rusqlite::{params, Connection, OptionalExtension, Result};
use std::sync::Mutex;
use log::{error, info};

pub struct DatabaseService {
    conn: Mutex<Connection>,
}

impl DatabaseService {
    /// Creates a new `DatabaseService` and ensures the database connection is valid.
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Initializes the database schema.
    pub fn initialize_db(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Log the start of database initialization
        info!("Initializing database schema...");

        match conn.execute_batch(
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
        ) {
            Ok(_) => {
                info!("Database schema initialized successfully.");
                Ok(())
            }
            Err(e) => {
                error!("Failed to initialize database schema: {:?}", e);
                Err(e)
            }
        }
    }

    /// Adds or updates a topic in the database.
    pub fn add_or_update_topic(
        &self,
        topic: &str,
        parent_topic: Option<&str>,
        max_values: usize,
        query_frequency_ms: u64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT INTO topics (topic, parent_topic, max_values, query_frequency_ms)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(topic) DO UPDATE SET
                parent_topic = excluded.parent_topic,
                max_values = excluded.max_values,
                query_frequency_ms = excluded.query_frequency_ms
            "#,
            params![topic, parent_topic, max_values, query_frequency_ms],
        )?;
        Ok(())
    }

    /// Inserts a new value for a topic and trims old values based on `max_values`.
    pub fn insert_value(&self, topic: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare("SELECT id, max_values FROM topics WHERE topic = ?1")
            .map_err(|e| {
                error!("Failed to prepare SELECT query for topic '{}': {:?}", topic, e);
                e
            })?;
        let mut rows = stmt.query(params![topic])?;

        if let Some(row) = rows.next()? {
            let topic_id: i64 = row.get(0)?;
            let max_values: i64 = row.get(1)?;

            conn.execute(
                "INSERT INTO topic_values (topic_id, value) VALUES (?1, ?2)",
                params![topic_id, value],
            ).map_err(|e| {
                error!("Failed to insert value for topic '{}': {:?}", topic, e);
                e
            })?;

            conn.execute(
                "DELETE FROM topic_values
             WHERE id NOT IN (
                 SELECT id
                 FROM topic_values
                 WHERE topic_id = ?1
                 ORDER BY timestamp DESC
                 LIMIT ?2
             ) AND topic_id = ?1",
                params![topic_id, max_values],
            ).map_err(|e| {
                error!("Failed to delete old values for topic '{}': {:?}", topic, e);
                e
            })?;
        } else {
            error!("Topic '{}' not found in database.", topic);
        }
        Ok(())
    }


    /// Retrieves the last `n` values for a topic, including their timestamps.
    pub fn get_last_values(&self, topic: &str, limit: usize) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT value, timestamp FROM topic_values
         INNER JOIN topics ON topics.id = topic_values.topic_id
         WHERE topics.topic = ?1
         ORDER BY topic_values.timestamp DESC
         LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![topic, limit], |row| {
            Ok((row.get(0)?, row.get(1)?)) // Return both value and timestamp
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    pub fn get_last_value(&self, topic: &str) -> Result<Option<(String, String)>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT value, timestamp
         FROM topic_values
         WHERE topic_id = (SELECT id FROM topics WHERE topic = ?1)
         ORDER BY timestamp DESC
         LIMIT 1",
        )?;
        let mut rows = stmt.query(params![topic])?;

        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            let timestamp: String = row.get(1)?;
            Ok(Some((value, timestamp)))
        } else {
            Ok(None)
        }
    }
    /// Aktualisiert den Broker für alle Topics
    pub fn update_broker_for_topics(&self, old_broker_name: &str, new_broker_name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            UPDATE topics
            SET broker_id = (SELECT id FROM brokers WHERE name = ?2)
            WHERE broker_id = (SELECT id FROM brokers WHERE name = ?1)
            "#,
            params![old_broker_name, new_broker_name],
        )?;
        Ok(())
    }

    /// Überprüft, ob ein Topic existiert und ob es noch zum aktuellen Broker gehört
    pub fn validate_topic(&self, topic: &str, broker_name: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT 1
            FROM topics
            WHERE topic = ?1 AND broker_id = (SELECT id FROM brokers WHERE name = ?2)
            "#,
        )?;
        let exists: Option<i32> = stmt.query_row(params![topic, broker_name], |row| row.get(0)).optional()?;
        Ok(exists.is_some())
    }

    /// Überprüft, ob ein Broker existiert, und fügt ihn hinzu, falls nicht vorhanden.
    pub fn validate_or_add_broker(
        &self,
        broker_name: &str,
        broker_host: &str,
        broker_port: u16,
        username: Option<&str>,
        password: Option<&str>,
        tls_enabled: bool,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT OR IGNORE INTO brokers (name, host, port, username, password, tls_enabled)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                broker_name,
                broker_host,
                broker_port,
                username,
                password,
                tls_enabled
            ],
        )?;
        Ok(())
    }
}
