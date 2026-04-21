mod api_doc;
mod backend;
mod config;
mod metrics;
mod pool;
mod resilience;
mod server;
mod types;

use std::path::Path;

use clap::{Args, Parser, Subcommand};
use config::GatewayConfig;
use server::GatewayServer;

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start the gateway server in foreground
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
struct ServeArgs {
    /// Host to bind to
    #[arg(long)]
    host: Option<String>,

    /// Port to listen on
    #[arg(long)]
    port: Option<u16>,

    /// Fluss coordinator address (e.g. localhost:9123)
    #[arg(long)]
    fluss_coordinator: Option<String>,

    /// Auth type
    #[arg(long)]
    auth_type: Option<String>,

    /// SASL username for Fluss authentication (fallback in "none" mode)
    #[arg(long)]
    sasl_username: Option<String>,

    /// SASL password for Fluss authentication (fallback in "none" mode)
    #[arg(long)]
    sasl_password: Option<String>,

    /// Config file path
    #[arg(long)]
    config: Option<String>,

    /// Pool max connections
    #[arg(long)]
    pool_max_connections: Option<u32>,

    /// Pool idle timeout in seconds
    #[arg(long)]
    pool_idle_timeout_secs: Option<u64>,

    /// Log level
    #[arg(long)]
    log_level: Option<String>,
}

impl ServeArgs {
    fn load_config(&self) -> anyhow::Result<GatewayConfig> {
        let mut config = match &self.config {
            Some(path) => GatewayConfig::from_file(Path::new(path))?,
            None => {
                let default_path = Path::new("gateway.toml");
                GatewayConfig::from_file(default_path).unwrap_or_default()
            }
        };

        if let Some(ref v) = self.host {
            config.server.host = v.clone();
        }
        if let Some(v) = self.port {
            config.server.port = v;
        }
        if let Some(ref v) = self.fluss_coordinator {
            config.fluss.coordinator = v.clone();
        }
        if let Some(ref v) = self.auth_type {
            config.auth.r#type = match v.as_str() {
                "none" => config::AuthType::None,
                "passthrough" => config::AuthType::Passthrough,
                _ => config.auth.r#type.clone(),
            };
        }
        if let Some(ref v) = self.sasl_username {
            config.auth.startup_username = v.clone();
        }
        if let Some(ref v) = self.sasl_password {
            config.auth.startup_password = v.clone();
        }
        if let Some(v) = self.pool_max_connections {
            config.pool.max_connections = v;
        }
        if let Some(v) = self.pool_idle_timeout_secs {
            config.pool.idle_timeout_secs = v;
        }
        if let Some(ref v) = self.log_level {
            config.log.level = v.clone();
        }

        Ok(config)
    }
}

#[derive(Debug, Parser)]
#[command(name = "fluss-gateway")]
#[command(about = "REST API Gateway for Apache Fluss")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve(args) => {
            let config = args.load_config()?;

            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive(format!("fluss_gateway={}", config.log.level).parse()?),
                )
                .init();

            // Initialize Prometheus metrics recorder
            metrics::PrometheusRecorder::install()
                .map_err(|e| {
                    tracing::warn!(error = %e, "failed to initialize metrics recorder");
                })
                .ok();
            tracing::info!("prometheus metrics recorder initialized");

            let addr = format!("{}:{}", config.server.host, config.server.port);

            tracing::info!(
                coordinator = %config.fluss.coordinator,
                auth_type = ?config.auth.r#type,
                addr = %addr,
                "starting fluss-gateway"
            );

            let server = GatewayServer::new(config).await?;
            server.run(&addr).await?;

            Ok(())
        }
    }
}
