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

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const GATEWAY_URL: &str = "http://localhost:8080";
const COMPOSE_FILE: &str = "deploy/docker/docker-compose.dev.yml";
const COMPOSE_PROJECT: &str = "fluss-gateway";

fn compose(args: &[&str]) -> std::process::ExitStatus {
    Command::new("podman")
        .args(["compose", "--project-name", COMPOSE_PROJECT, "-f", COMPOSE_FILE])
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("Failed to run podman compose: {}", e))
}

async fn is_gateway_ready() -> bool {
    // Check containers are running
    let output = Command::new("podman")
        .args(["ps", "--filter", "name=fluss-gateway-coordinator", "--format", "{{.State}}"])
        .output()
        .ok();
    if !output.as_ref()
        .and_then(|o| String::from_utf8(o.stdout.clone()).ok())
        .map(|s| s.trim() == "running")
        .unwrap_or(false)
    {
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
    // Check if already running
    if is_gateway_ready().await {
        println!("Gateway already running, skipping startup");
        return;
    }

    // Start Fluss cluster
    println!("Starting Fluss cluster via podman compose...");
    assert!(compose(&["up", "-d"]).success(), "podman compose up failed");

    // Wait for Fluss coordinator to be healthy
    let start = Instant::now();
    loop {
        let status = Command::new("podman")
            .args(["inspect", "--format={{.State.Health.Status}}", "fluss-gateway-coordinator-server-1"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok());

        if status.as_deref().map(|s| s.trim()) == Some("healthy") {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(120), "Fluss cluster did not become healthy within 120s");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Start gateway binary via nohup so it survives after cargo test exits
    println!("Starting gateway binary...");
    let binary = env!("CARGO_BIN_EXE_fluss-gateway");
    let output = Command::new("nohup")
        .arg(binary)
        .arg("serve")
        .arg("--fluss-coordinator=localhost:9123")
        .arg("--port=8080")
        .arg("--auth-type=none")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start gateway binary with nohup");

    let pid = output.id();
    std::fs::write("/tmp/fluss-gateway-test.pid", pid.to_string())
        .expect("Failed to write PID file");
    // nohup fully detaches — no need to forget or keep the Child

    // Wait for gateway to be ready
    let start = Instant::now();
    loop {
        if is_gateway_ready().await {
            println!("Gateway is ready! (PID: {})", pid);
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "Gateway did not become ready within 60s");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
