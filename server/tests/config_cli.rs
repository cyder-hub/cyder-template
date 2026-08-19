use std::{fs, process::Command};

use serde_json::Value;
use tempfile::tempdir;

fn application_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cyder-template"));
    command.env_clear();
    command
}

#[test]
fn endpoint_command_reports_the_resolved_yaml_endpoint_as_json() {
    let temporary_directory = tempdir().expect("temporary directory should be created");
    let config_path = temporary_directory.path().join("config.yaml");
    fs::write(&config_path, "host: 0.0.0.0\nport: 19031\n")
        .expect("temporary config should be written");

    let output = application_command()
        .args(["config", "endpoint", "--format", "json"])
        .env("APP_CONFIG_PATH", &config_path)
        .output()
        .expect("configuration command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let endpoint: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(
        endpoint,
        serde_json::json!({ "host": "0.0.0.0", "port": 19031 })
    );
}

#[test]
fn endpoint_command_applies_environment_overrides() {
    let temporary_directory = tempdir().expect("temporary directory should be created");
    let config_path = temporary_directory.path().join("config.yaml");
    fs::write(&config_path, "host: 127.0.0.1\nport: 19031\n")
        .expect("temporary config should be written");

    let output = application_command()
        .args(["config", "endpoint", "--format", "json"])
        .env("APP_CONFIG_PATH", &config_path)
        .env("APP_HOST", "0.0.0.0")
        .env("APP_PORT", "19032")
        .output()
        .expect("configuration command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let endpoint: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(
        endpoint,
        serde_json::json!({ "host": "0.0.0.0", "port": 19032 })
    );
}

#[test]
fn endpoint_command_reports_an_ipv6_bind_host() {
    let output = application_command()
        .args(["config", "endpoint", "--format", "json"])
        .env("APP_HOST", "::")
        .env("APP_PORT", "19033")
        .output()
        .expect("configuration command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let endpoint: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(endpoint, serde_json::json!({ "host": "::", "port": 19033 }));
}

#[test]
fn unknown_arguments_fail_without_starting_the_server() {
    let output = application_command()
        .arg("serve")
        .output()
        .expect("application command should run");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("command-line error"));
}
