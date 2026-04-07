//! Setup test: starts the Fluss cluster and Gateway binary.
//!
//! Run before integration tests: `cargo test --test setup`
//!
//! ```bash
//! # Manual workflow
//! cargo test --test setup          # start cluster + gateway
//! cargo test --test integration     # run tests
//! cargo test --test teardown        # stop cluster + gateway
//! ```
//!
//! Uses `docker` in CI, `podman` locally.

use std::env;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const GATEWAY_URL: &str = "http://localhost:8080";
const COMPOSE_FILE: &str = "deploy/docker/docker-compose.dev.yml";
const COMPOSE_PROJECT: &str = "fluss-gateway";
const COORDINATOR_NAME: &str = "fluss-gateway-coordinator-server-1";
const TABLET_NAME: &str = "fluss-gateway-tablet-server-1";

fn runtime() -> &'static str {
    if env::var("CI").is_ok() {
        "docker"
    } else {
        "podman"
    }
}

fn compose(args: &[&str]) -> std::process::ExitStatus {
    Command::new(runtime())
        .args([
            "compose",
            "--project-name",
            COMPOSE_PROJECT,
            "-f",
            COMPOSE_FILE,
        ])
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("Failed to run {} compose: {}", runtime(), e))
}

fn container_state(container: &str) -> Option<String> {
    Command::new(runtime())
        .args([
            "ps",
            "--filter",
            &format!("name={}", container),
            "--format",
            "{{.State}}",
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

fn health_status(container: &str) -> Option<String> {
    Command::new(runtime())
        .args(["inspect", "--format={{.State.Health.Status}}", container])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

async fn is_gateway_ready() -> bool {
    // Check coordinator container is running
    if container_state(COORDINATOR_NAME) != Some("running".into()) {
        return false;
    }
    // Check gateway responds
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get(format!("{}/health", GATEWAY_URL))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[tokio::test]
async fn test_setup_cluster() {
    let rt = runtime();

    // Check if already running
    if is_gateway_ready().await {
        println!("Gateway already running, skipping startup");
        return;
    }

    // Start Fluss cluster
    println!("Starting Fluss cluster via {} compose...", rt);
    assert!(compose(&["up", "-d"]).success(), "{} compose up failed", rt);

    // Wait for Fluss coordinator to be healthy
    let start = Instant::now();
    loop {
        if health_status(COORDINATOR_NAME).as_deref() == Some("healthy") {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "Coordinator did not become healthy within 120s"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Wait for tablet server to be healthy (registers with coordinator)
    println!("Waiting for tablet server to be healthy...");
    let start = Instant::now();
    loop {
        if health_status(TABLET_NAME).as_deref() == Some("healthy") {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "Tablet server did not become healthy within 120s"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Start gateway binary via nohup so it survives after cargo test exits
    println!("Starting gateway binary...");
    let binary = env!("CARGO_BIN_EXE_fluss-gateway");
    #[allow(clippy::zombie_processes)]
    let output = Command::new("nohup")
        .arg(binary)
        .arg("serve")
        .arg("--fluss-coordinator=localhost:9123")
        .arg("--port=8080")
        .arg("--auth-type=none")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start gateway binary");

    let pid = output.id();
    std::fs::write("/tmp/fluss-gateway-test.pid", pid.to_string())
        .expect("Failed to write PID file");

    // Wait for gateway to be ready
    let start = Instant::now();
    loop {
        if is_gateway_ready().await {
            println!("Gateway is ready! (PID: {})", pid);
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "Gateway did not become ready within 60s"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
