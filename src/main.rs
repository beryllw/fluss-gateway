mod backend;
mod server;
mod types;

use clap::Parser;
use server::GatewayServer;

#[derive(Parser, Debug)]
#[command(name = "fluss-gateway")]
#[command(about = "REST API Gateway for Apache Fluss")]
struct Args {
    /// Host to bind to
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Port to listen on
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Fluss coordinator address (e.g. localhost:9123)
    #[arg(long, default_value = "localhost:9123")]
    fluss_coordinator: String,

    /// SASL username for Fluss authentication
    #[arg(long, default_value = "")]
    sasl_username: String,

    /// SASL password for Fluss authentication
    #[arg(long, default_value = "")]
    sasl_password: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("fluss_gateway=info".parse()?),
        )
        .init();

    let args = Args::parse();
    let addr = format!("{}:{}", args.host, args.port);

    tracing::info!(
        coordinator = %args.fluss_coordinator,
        addr = %addr,
        "starting flus-gateway"
    );

    let server = if !args.sasl_username.is_empty() {
        GatewayServer::new_with_auth(&args.fluss_coordinator, &args.sasl_username, &args.sasl_password).await?
    } else {
        GatewayServer::new(&args.fluss_coordinator).await?
    };
    server.run(&addr).await?;

    Ok(())
}
