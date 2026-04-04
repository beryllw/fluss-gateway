use fluss::client::FlussConnection;
use moka::future::Cache;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::config::{AuthConfig, PoolConfig};

/// Key used to identify a unique Fluss connection per credential pair.
pub type CredentialKey = (String, [u8; 32]); // (username, SHA-256(password))

/// Connection pool backed by moka cache. Creates one FlussConnection per
/// unique credential and caches it with idle-timeout eviction.
pub struct ConnectionPool {
    cache: Cache<CredentialKey, Arc<FlussConnection>>,
    coordinator: String,
    auth: AuthConfig,
}

impl ConnectionPool {
    pub fn new(coordinator: &str, auth: AuthConfig, pool: PoolConfig) -> Self {
        let cache: Cache<CredentialKey, Arc<FlussConnection>> = Cache::builder()
            .max_capacity(pool.max_connections)
            .time_to_idle(std::time::Duration::from_secs(pool.idle_timeout_secs))
            .build();

        Self {
            cache,
            coordinator: coordinator.to_string(),
            auth,
        }
    }

    /// Get an existing connection or create a new one for the given credentials.
    /// When `creds` is `None`, uses the startup credentials from config.
    pub async fn get_or_create(
        &self,
        creds: Option<(&str, &str)>,
    ) -> Arc<FlussConnection> {
        let (username, password) = match creds {
            Some((u, p)) => (u.to_string(), p.to_string()),
            None => (
                self.auth.startup_username.clone(),
                self.auth.startup_password.clone(),
            ),
        };

        let key = hash_credentials(&username, &password);

        self.cache
            .get_with(key, async {
                Arc::new(create_connection(&self.coordinator, &username, &password).await)
            })
            .await
    }
}

/// Hash password with SHA-256 for use as cache key.
fn hash_credentials(username: &str, password: &str) -> CredentialKey {
    let mut hasher = Sha256::new();
    hasher.update(username.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    let hash: [u8; 32] = hasher.finalize().into();
    (username.to_string(), hash)
}

/// Create a new FlussConnection with optional SASL credentials.
async fn create_connection(
    coordinator: &str,
    username: &str,
    password: &str,
) -> FlussConnection {
    use fluss::config::Config;

    let config = Config {
        bootstrap_servers: coordinator.to_string(),
        security_protocol: if !username.is_empty() && !password.is_empty() {
            "sasl".to_string()
        } else {
            String::new()
        },
        security_sasl_mechanism: if !username.is_empty() && !password.is_empty() {
            "PLAIN".to_string()
        } else {
            String::new()
        },
        security_sasl_username: username.to_string(),
        security_sasl_password: password.to_string(),
        ..Default::default()
    };

    FlussConnection::new(config).await.unwrap_or_else(|e| {
        panic!("failed to connect to Fluss coordinator {coordinator}: {e}")
    })
}
