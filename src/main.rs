mod backend;
mod config;
mod pool;
mod server;
mod types;

use std::path::Path;

use clap::Parser;
use config::GatewayCliArgs;
use server::GatewayServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli_args = GatewayCliArgs::parse();

    // Load config: file -> CLI override -> defaults
    let mut config = match &cli_args.config {
        Some(path) => config::GatewayConfig::from_file(Path::new(path))?,
        None => {
            // Try default config file location
            let default_path = Path::new("gateway.toml");
            config::GatewayConfig::from_file(default_path).unwrap_or_default()
        }
    };
    config.apply_cli_args(&cli_args);

    // Set up logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(
                    format!("fluss_gateway={}", config.log.level)
                        .parse()?,
                ),
        )
        .init();

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
