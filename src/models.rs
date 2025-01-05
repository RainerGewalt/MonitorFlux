#[derive(Debug)]
pub struct Broker {
    pub id: i64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls_enabled: bool,
}

#[derive(Debug)]
pub struct Topic {
    pub id: i64,
    pub topic: String,
    pub parent_topic: Option<String>,
    pub max_values: usize,
    pub query_frequency_ms: u64,
}

#[derive(Debug)]
pub struct Subscription {
    pub id: i64,
    pub broker_id: i64,
    pub topic_id: i64,
    pub is_active: bool,
}
