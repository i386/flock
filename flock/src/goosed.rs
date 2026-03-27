use anyhow::{anyhow, Context, Result};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

const DEFAULT_GOOSED_PORT_OFFSET: u16 = 1;

#[derive(Debug)]
pub struct GoosedSupervisor {
    binary_path: Option<PathBuf>,
    secret_key: String,
    port: u16,
    child: Option<Child>,
    version: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GoosedStatus {
    pub available: bool,
    pub healthy: bool,
    pub version: Option<String>,
    pub port: u16,
    pub binary_path: Option<String>,
    pub last_error: Option<String>,
}

impl GoosedSupervisor {
    pub fn new(local_port: u16) -> Self {
        let binary_path = resolve_goosed_binary();
        let version = binary_path
            .as_ref()
            .and_then(|path| read_goosed_version(path).ok());

        Self {
            binary_path,
            secret_key: random_secret_key(),
            port: local_port.saturating_add(DEFAULT_GOOSED_PORT_OFFSET),
            child: None,
            version,
            last_error: None,
        }
    }

    pub fn snapshot(&self, healthy: bool) -> GoosedStatus {
        GoosedStatus {
            available: self.binary_path.is_some(),
            healthy,
            version: self.version.clone(),
            port: self.port,
            binary_path: self
                .binary_path
                .as_ref()
                .map(|path| path.display().to_string()),
            last_error: self.last_error.clone(),
        }
    }

    pub async fn ensure_started(&mut self) -> Result<()> {
        let Some(binary_path) = self.binary_path.clone() else {
            self.last_error = Some("goosed binary not found".to_string());
            return Ok(());
        };

        if self.health_check().await {
            self.last_error = None;
            return Ok(());
        }

        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.last_error = Some(format!("goosed exited with status {status}"));
                    self.child = None;
                }
                Ok(None) => {
                    return Ok(());
                }
                Err(error) => {
                    self.last_error = Some(format!("failed to poll goosed: {error}"));
                    self.child = None;
                }
            }
        }

        let mut command = Command::new(&binary_path);
        command
            .arg("agent")
            .env("GOOSE_HOST", "127.0.0.1")
            .env("GOOSE_PORT", self.port.to_string())
            .env("GOOSE_TLS", "false")
            .env("GOOSE_SERVER__SECRET_KEY", &self.secret_key)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = command
            .spawn()
            .with_context(|| format!("failed to start goosed from {}", binary_path.display()))?;
        self.child = Some(child);

        for _ in 0..20 {
            if self.health_check().await {
                self.last_error = None;
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        self.last_error = Some("goosed did not become healthy in time".to_string());
        Ok(())
    }

    pub async fn health_check(&self) -> bool {
        let url = format!("http://127.0.0.1:{}/status", self.port);
        let client = reqwest::Client::new();
        match client.get(url).timeout(Duration::from_secs(1)).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}

impl Drop for GoosedSupervisor {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
    }
}

fn random_secret_key() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn resolve_goosed_binary() -> Option<PathBuf> {
    if let Ok(explicit) = env::var("FLOCK_GOOSED_BIN") {
        let path = PathBuf::from(explicit);
        if path.exists() {
            return Some(path);
        }
    }

    resolve_from_path("goosed").or_else(resolve_from_local_checkout)
}

fn resolve_from_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn resolve_from_local_checkout() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let candidate = home.join("code/goose/target/debug/goosed");
    candidate.exists().then_some(candidate)
}

fn read_goosed_version(path: &Path) -> Result<String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .output()
        .with_context(|| format!("failed to run {} --version", path.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "{} --version exited with {}",
            path.display(),
            output.status
        ));
    }

    let version = String::from_utf8(output.stdout)
        .context("goosed --version returned non-utf8 output")?
        .trim()
        .to_string();
    if version.is_empty() {
        return Err(anyhow!("goosed --version returned empty output"));
    }

    Ok(version)
}
