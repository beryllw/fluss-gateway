use std::path::Path;

use clap::Parser;
use serde::Deserialize;

/// Gateway configuration, loaded from `gateway.toml` and overridden by CLI args.
#[derive(Clone, Debug, Default)]
pub struct GatewayConfig {
    pub server: ServerConfig,
    pub fluss: FlussConfig,
    pub auth: AuthConfig,
    pub pool: PoolConfig,
    pub log: LogConfig,
}

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 8080,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FlussConfig {
    pub coordinator: String,
}

impl Default for FlussConfig {
    fn default() -> Self {
        Self {
            coordinator: "localhost:9123".into(),
        }
    }
}

/// Auth mode: `"none"` (default, uses static startup credentials) or
/// `"passthrough"` (per-request credentials required via HTTP Basic Auth).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    #[default]
    None,
    Passthrough,
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub r#type: AuthType,
    pub startup_username: String,
    pub startup_password: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            r#type: AuthType::None,
            startup_username: String::new(),
            startup_password: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PoolConfig {
    pub max_connections: u64,
    pub idle_timeout_secs: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 500,
            idle_timeout_secs: 600,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogConfig {
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
        }
    }
}

// === File config (serde) ===

#[derive(Deserialize, Default)]
struct FileConfig {
    server: Option<FileServerConfig>,
    fluss: Option<FileFlussConfig>,
    auth: Option<FileAuthConfig>,
    pool: Option<FilePoolConfig>,
    log: Option<FileLogConfig>,
}

#[derive(Deserialize, Default)]
struct FileServerConfig {
    host: Option<String>,
    port: Option<u16>,
}

#[derive(Deserialize, Default)]
struct FileFlussConfig {
    coordinator: Option<String>,
}

#[derive(Deserialize, Default)]
struct FileAuthConfig {
    r#type: Option<String>,
    startup_username: Option<String>,
    startup_password: Option<String>,
}

#[derive(Deserialize, Default)]
struct FilePoolConfig {
    max_connections: Option<u64>,
    idle_timeout_secs: Option<u64>,
}

#[derive(Deserialize, Default)]
struct FileLogConfig {
    level: Option<String>,
}

// === Loading ===

impl GatewayConfig {
    /// Load from `gateway.toml` at the given path. If the file doesn't exist,
    /// return default config.
    pub fn from_file(path: &Path) -> std::io::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let file: FileConfig = toml::from_str(&content).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("invalid TOML: {e}"))
        })?;

        let auth_type = match file.auth.as_ref().and_then(|a| a.r#type.as_deref()) {
            Some("none") | None => AuthType::None,
            Some("passthrough") => AuthType::Passthrough,
            Some(other) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid auth type: {other}"),
                ))
            }
        };

        Ok(Self {
            server: ServerConfig {
                host: file
                    .server
                    .as_ref()
                    .and_then(|s| s.host.clone())
                    .unwrap_or_default(),
                port: file.server.as_ref().and_then(|s| s.port).unwrap_or_default(),
            },
            fluss: FlussConfig {
                coordinator: file
                    .fluss
                    .as_ref()
                    .and_then(|f| f.coordinator.clone())
                    .unwrap_or_default(),
            },
            auth: AuthConfig {
                r#type: auth_type,
                startup_username: file
                    .auth
                    .as_ref()
                    .and_then(|a| a.startup_username.clone())
                    .unwrap_or_default(),
                startup_password: file
                    .auth
                    .as_ref()
                    .and_then(|a| a.startup_password.clone())
                    .unwrap_or_default(),
            },
            pool: PoolConfig {
                max_connections: file
                    .pool
                    .as_ref()
                    .and_then(|p| p.max_connections)
                    .unwrap_or(500),
                idle_timeout_secs: file
                    .pool
                    .as_ref()
                    .and_then(|p| p.idle_timeout_secs)
                    .unwrap_or(600),
            },
            log: LogConfig {
                level: file
                    .log
                    .as_ref()
                    .and_then(|l| l.level.clone())
                    .unwrap_or_default(),
            },
        })
    }

    /// Override config fields with CLI args (CLI takes priority).
    pub fn apply_cli_args(&mut self, args: &GatewayCliArgs) {
        if let Some(ref v) = args.host {
            self.server.host = v.clone();
        }
        if let Some(v) = args.port {
            self.server.port = v;
        }
        if let Some(ref v) = args.fluss_coordinator {
            self.fluss.coordinator = v.clone();
        }
        if let Some(ref v) = args.auth_type {
            self.auth.r#type = match v.as_str() {
                "none" => AuthType::None,
                "passthrough" => AuthType::Passthrough,
                _ => self.auth.r#type.clone(),
            };
        }
        if let Some(ref v) = args.sasl_username {
            self.auth.startup_username = v.clone();
        }
        if let Some(ref v) = args.sasl_password {
            self.auth.startup_password = v.clone();
        }
        if let Some(v) = args.pool_max_connections {
            self.pool.max_connections = v;
        }
        if let Some(v) = args.pool_idle_timeout_secs {
            self.pool.idle_timeout_secs = v;
        }
        if let Some(ref v) = args.log_level {
            self.log.level = v.clone();
        }
    }
}

/// CLI argument definitions (mirror of config fields).
#[derive(Parser, Debug)]
#[command(name = "fluss-gateway")]
#[command(about = "REST API Gateway for Apache Fluss")]
pub struct GatewayCliArgs {
    /// Host to bind to
    #[arg(long)]
    pub host: Option<String>,

    /// Port to listen on
    #[arg(long)]
    pub port: Option<u16>,

    /// Fluss coordinator address (e.g. localhost:9123)
    #[arg(long)]
    pub fluss_coordinator: Option<String>,

    /// Auth type
    #[arg(long)]
    pub auth_type: Option<String>,

    /// SASL username for Fluss authentication (fallback in "none" mode)
    #[arg(long)]
    pub sasl_username: Option<String>,

    /// SASL password for Fluss authentication (fallback in "none" mode)
    #[arg(long)]
    pub sasl_password: Option<String>,

    /// Config file path
    #[arg(long)]
    pub config: Option<String>,

    /// Pool max connections
    #[arg(long)]
    pub pool_max_connections: Option<u64>,

    /// Pool idle timeout in seconds
    #[arg(long)]
    pub pool_idle_timeout_secs: Option<u64>,

    /// Log level
    #[arg(long)]
    pub log_level: Option<String>,
}
