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

#[test]
fn config_check_reports_a_safe_postgres_and_http_summary() {
    let temporary_directory = tempdir().expect("temporary directory should be created");
    let secret = "unique-cli-secret-marker";
    let config_path = temporary_directory.path().join("config.yaml");
    fs::write(
        &config_path,
        "http_request_timeout_ms: 45000\nhttp_max_concurrent_requests: 128\nhttp_max_request_body_bytes: 2097152\n",
    )
    .expect("temporary config should be written");
    let output = application_command()
        .args(["config", "check", "--format", "json"])
        .env("APP_DATA_DIR", temporary_directory.path())
        .env("APP_CONFIG_PATH", config_path)
        .env(
            "APP_DATABASE_URL",
            format!("postgres://app:{secret}@localhost/app"),
        )
        .output()
        .expect("configuration command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should contain summary JSON");
    assert_eq!(summary["valid"], true);
    assert_eq!(summary["database_kind"], "postgres");
    assert_eq!(summary["database_pool_size"], 5);
    assert_eq!(summary["http_request_timeout_ms"], 45_000);
    assert_eq!(summary["http_max_concurrent_requests"], 128);
    assert_eq!(summary["http_max_request_body_bytes"], 2_097_152);
    assert!(summary.get("database_url").is_none());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
}

#[test]
fn unknown_environment_keys_warn_at_runtime_and_fail_config_checks_without_values() {
    let temporary_directory = tempdir().expect("temporary directory should be created");
    let secret = "unknown-value-secret-marker";

    let compatible = application_command()
        .args(["config", "endpoint", "--format", "json"])
        .env("APP_DATA_DIR", temporary_directory.path())
        .env("APP_MISSPELLED_SETTING", secret)
        .output()
        .expect("compatible check should run");
    assert!(compatible.status.success());
    let compatible_stderr = String::from_utf8_lossy(&compatible.stderr);
    assert!(compatible_stderr.contains("APP_MISSPELLED_SETTING"));
    assert!(!compatible_stderr.contains(secret));
    assert!(!String::from_utf8_lossy(&compatible.stdout).contains(secret));

    let checked = application_command()
        .args(["config", "check"])
        .env("APP_DATA_DIR", temporary_directory.path())
        .env("APP_MISSPELLED_SETTING", secret)
        .output()
        .expect("configuration check should run");
    assert!(!checked.status.success());
    let checked_stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(checked_stderr.contains("APP_MISSPELLED_SETTING"));
    assert!(!checked_stderr.contains(secret));
}

#[test]
fn explicit_missing_config_and_invalid_log_filter_fail_fast() {
    let temporary_directory = tempdir().expect("temporary directory should be created");
    let missing = temporary_directory.path().join("missing.yaml");
    let missing_output = application_command()
        .args(["config", "check"])
        .env("APP_CONFIG_PATH", missing)
        .output()
        .expect("configuration command should run");
    assert!(!missing_output.status.success());
    assert!(String::from_utf8_lossy(&missing_output.stderr).contains("does not exist"));

    let invalid_log_output = application_command()
        .args(["config", "check"])
        .env("APP_DATA_DIR", temporary_directory.path())
        .env("APP_LOG_LEVEL", "[")
        .output()
        .expect("configuration command should run");
    assert!(!invalid_log_output.status.success());
    assert!(String::from_utf8_lossy(&invalid_log_output.stderr).contains("log_level"));
}

#[test]
fn default_config_file_is_loaded_from_the_data_directory() {
    let temporary_directory = tempdir().expect("temporary directory should be created");
    let config_directory = temporary_directory.path().join("config");
    fs::create_dir_all(&config_directory).expect("config directory should be created");
    fs::write(
        config_directory.join("config.yaml"),
        "host: 0.0.0.0\nport: 19034\n",
    )
    .expect("configuration should be written");

    let output = application_command()
        .args(["config", "endpoint", "--format", "json"])
        .env("APP_DATA_DIR", temporary_directory.path())
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
        serde_json::json!({ "host": "0.0.0.0", "port": 19034 })
    );
}

#[test]
fn config_check_does_not_create_the_data_directory_or_database() {
    let temporary_directory = tempdir().expect("temporary directory should be created");
    let data_directory = temporary_directory.path().join("absent-data");
    fs::write(
        temporary_directory.path().join(".env"),
        "APP_PORT=19099\nAPP_DATABASE_URL=postgres://app:secret@database/app\n",
    )
    .expect("dotenv decoy should be written");

    let output = application_command()
        .args(["config", "check"])
        .current_dir(temporary_directory.path())
        .env("APP_DATA_DIR", &data_directory)
        .output()
        .expect("configuration command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!data_directory.exists());
    let summary = String::from_utf8(output.stdout).expect("summary should be UTF-8");
    assert!(summary.contains("listen: 127.0.0.1:8000"));
    assert!(summary.contains("database_kind: sqlite"));
    assert!(!summary.contains("secret"));
}
