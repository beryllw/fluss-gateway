//! Teardown test: cleans up the Fluss cluster and Gateway binary after integration tests.
//!
//! Run after integration tests: `cargo test --test teardown`
//!
//! Uses `docker` in CI, `podman` locally.

use std::env;
use std::process::Command;

const COMPOSE_FILE: &str = "deploy/docker/docker-compose.dev.yml";
const COMPOSE_PROJECT: &str = "fluss-gateway";

fn runtime() -> &'static str {
    if env::var("CI").is_ok() {
        "docker"
    } else {
        "podman"
    }
}

fn compose(args: &[&str]) -> std::process::ExitStatus {
    Command::new(runtime())
        .args(["compose", "--project-name", COMPOSE_PROJECT, "-f", COMPOSE_FILE])
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("Failed to run {} compose: {}", runtime(), e))
}

#[test]
fn test_teardown_cluster() {
    let rt = runtime();

    // Kill the gateway process (started by setup test)
    if let Ok(pid_str) = std::fs::read_to_string("/tmp/fluss-gateway-test.pid") {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            println!("Killing gateway process (PID: {})", pid);
            let _ = Command::new("kill").arg(pid.to_string()).status();
            let _ = std::fs::remove_file("/tmp/fluss-gateway-test.pid");
        }
    }

    // Stop Fluss cluster
    println!("Stopping Fluss cluster...");
    assert!(compose(&["down", "--remove-orphans"]).success(), "{} compose down failed", rt);

    // Remove any dangling containers from legacy runs (docker- prefix)
    let output = Command::new(rt)
        .args(["ps", "-a", "--filter", "name=^docker-", "--format", "{{.ID}}"])
        .output()
        .expect("Failed to list containers");

    for id in String::from_utf8_lossy(&output.stdout).lines() {
        if !id.is_empty() {
            println!("Removing legacy container: {}", id);
            let _ = Command::new(rt).args(["rm", "-f", id]).status();
        }
    }

    println!("Teardown complete");
}
