use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_LOCAL_PORT: i64 = 43123;
pub const DEFAULT_PUBLISH_INTERVAL_SECS: i64 = 5;
pub const DEFAULT_STALE_AFTER_SECS: i64 = 20;
pub const DEFAULT_MAX_CPU_LOAD_PCT: i64 = 95;
pub const DEFAULT_MAX_MEMORY_USED_PCT: i64 = 95;
pub const DEFAULT_MIN_DISK_AVAILABLE_BYTES: i64 = 10 * 1024 * 1024 * 1024;
pub const DEFAULT_WEIGHT_RTT: f64 = 1.0;
pub const DEFAULT_WEIGHT_ACTIVE_CHATS: f64 = 15.0;
pub const DEFAULT_WEIGHT_CPU_LOAD: f64 = 0.7;
pub const DEFAULT_WEIGHT_MEMORY_USED: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub mesh_dir: PathBuf,
    pub config_path: PathBuf,
    pub installed_binary: PathBuf,
    pub current_binary: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self> {
        let config_path = mesh_config_path()?;
        let mesh_dir = config_path
            .parent()
            .ok_or_else(|| anyhow!("invalid mesh config path: {}", config_path.display()))?
            .to_path_buf();

        Ok(Self {
            installed_binary: mesh_dir.join("flock"),
            current_binary: std::env::current_exe().context("failed to resolve current executable")?,
            mesh_dir,
            config_path,
        })
    }
}

pub fn mesh_config_path() -> Result<PathBuf> {
    if let Ok(override_path) = std::env::var("MESH_LLM_CONFIG") {
        return Ok(PathBuf::from(override_path));
    }

    let home = dirs::home_dir().ok_or_else(|| anyhow!("failed to determine home directory"))?;
    Ok(home.join(".mesh-llm").join("config.toml"))
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub fn default_working_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("failed to determine home directory"))?;
    let code_dir = home.join("code");
    if code_dir.exists() {
        Ok(code_dir)
    } else {
        Ok(home)
    }
}

#[derive(Debug, Clone)]
pub struct RoutingConfig {
    pub publish_interval_secs: u64,
    pub stale_after_secs: u64,
    pub local_port: u16,
    pub working_dir: PathBuf,
    pub default_strategy: String,
    pub next_chat_target: Option<String>,
    pub default_host_preference: Option<String>,
    pub require_healthy_goosed: bool,
    pub max_cpu_load_pct: u8,
    pub max_memory_used_pct: u8,
    pub min_disk_available_bytes: u64,
    pub weight_rtt: f64,
    pub weight_active_chats: f64,
    pub weight_cpu_load: f64,
    pub weight_memory_used: f64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            publish_interval_secs: DEFAULT_PUBLISH_INTERVAL_SECS as u64,
            stale_after_secs: DEFAULT_STALE_AFTER_SECS as u64,
            local_port: DEFAULT_LOCAL_PORT as u16,
            working_dir: default_working_dir().unwrap_or_else(|_| PathBuf::from(".")),
            default_strategy: "balanced".to_string(),
            next_chat_target: None,
            default_host_preference: None,
            require_healthy_goosed: true,
            max_cpu_load_pct: DEFAULT_MAX_CPU_LOAD_PCT as u8,
            max_memory_used_pct: DEFAULT_MAX_MEMORY_USED_PCT as u8,
            min_disk_available_bytes: DEFAULT_MIN_DISK_AVAILABLE_BYTES as u64,
            weight_rtt: DEFAULT_WEIGHT_RTT,
            weight_active_chats: DEFAULT_WEIGHT_ACTIVE_CHATS,
            weight_cpu_load: DEFAULT_WEIGHT_CPU_LOAD,
            weight_memory_used: DEFAULT_WEIGHT_MEMORY_USED,
        }
    }
}

impl RoutingConfig {
    pub fn load(config_path: &Path) -> Result<Self> {
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        let raw = toml::from_str::<RawRootConfig>(&contents)
            .with_context(|| format!("failed to parse {}", config_path.display()))?;

        let raw_routing = raw
            .flock
            .and_then(|flock| flock.routing)
            .unwrap_or_default();

        let mut config = Self::default();
        if let Some(value) = raw_routing.publish_interval_secs {
            config.publish_interval_secs = value;
        }
        if let Some(value) = raw_routing.stale_after_secs {
            config.stale_after_secs = value;
        }
        if let Some(value) = raw_routing.local_port {
            config.local_port = value;
        }
        if let Some(value) = raw_routing.working_dir {
            config.working_dir = PathBuf::from(value);
        }
        if let Some(value) = raw_routing.default_strategy {
            config.default_strategy = value;
        }
        if let Some(value) = normalize_optional_string(raw_routing.next_chat_target) {
            config.next_chat_target = Some(value);
        }
        if let Some(value) = normalize_optional_string(raw_routing.default_host_preference) {
            config.default_host_preference = Some(value);
        }
        if let Some(value) = raw_routing.require_healthy_goosed {
            config.require_healthy_goosed = value;
        }
        if let Some(value) = raw_routing.max_cpu_load_pct {
            config.max_cpu_load_pct = value;
        }
        if let Some(value) = raw_routing.max_memory_used_pct {
            config.max_memory_used_pct = value;
        }
        if let Some(value) = raw_routing.min_disk_available_bytes {
            config.min_disk_available_bytes = value;
        }
        if let Some(value) = raw_routing.weight_rtt {
            config.weight_rtt = value;
        }
        if let Some(value) = raw_routing.weight_active_chats {
            config.weight_active_chats = value;
        }
        if let Some(value) = raw_routing.weight_cpu_load {
            config.weight_cpu_load = value;
        }
        if let Some(value) = raw_routing.weight_memory_used {
            config.weight_memory_used = value;
        }

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.publish_interval_secs == 0 {
            return Err(anyhow!("flock.routing.publish_interval_secs must be >= 1"));
        }
        if self.stale_after_secs <= self.publish_interval_secs {
            return Err(anyhow!(
                "flock.routing.stale_after_secs must be greater than publish_interval_secs"
            ));
        }
        if self.default_strategy != "balanced" {
            return Err(anyhow!(
                "unsupported flock.routing.default_strategy `{}`; only `balanced` is currently supported",
                self.default_strategy
            ));
        }
        Ok(())
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[derive(Debug, Deserialize)]
struct RawRootConfig {
    flock: Option<RawFlockConfig>,
}

#[derive(Debug, Deserialize)]
struct RawFlockConfig {
    routing: Option<RawRoutingConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct RawRoutingConfig {
    publish_interval_secs: Option<u64>,
    stale_after_secs: Option<u64>,
    local_port: Option<u16>,
    working_dir: Option<String>,
    default_strategy: Option<String>,
    next_chat_target: Option<String>,
    default_host_preference: Option<String>,
    require_healthy_goosed: Option<bool>,
    max_cpu_load_pct: Option<u8>,
    max_memory_used_pct: Option<u8>,
    min_disk_available_bytes: Option<u64>,
    weight_rtt: Option<f64>,
    weight_active_chats: Option<f64>,
    weight_cpu_load: Option<f64>,
    weight_memory_used: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_loaded_when_config_is_missing() {
        let missing = PathBuf::from("/tmp/definitely-missing-flock-config.toml");
        let config = RoutingConfig::load(&missing).expect("missing config should use defaults");

        assert_eq!(config.local_port, DEFAULT_LOCAL_PORT as u16);
        assert_eq!(
            config.publish_interval_secs,
            DEFAULT_PUBLISH_INTERVAL_SECS as u64
        );
        assert_eq!(config.default_strategy, "balanced");
        assert_eq!(config.next_chat_target, None);
    }

    #[test]
    fn routing_config_values_are_read_from_toml() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        fs::write(
            temp.path(),
            r#"
[flock.routing]
publish_interval_secs = 7
stale_after_secs = 30
local_port = 44000
working_dir = "/tmp/flock-work"
default_strategy = "balanced"
next_chat_target = "node-123"
default_host_preference = "node-456"
require_healthy_goosed = false
max_cpu_load_pct = 88
max_memory_used_pct = 77
min_disk_available_bytes = 12345
weight_rtt = 2.0
weight_active_chats = 3.0
weight_cpu_load = 4.0
weight_memory_used = 5.0
"#,
        )
        .expect("write config");

        let config = RoutingConfig::load(temp.path()).expect("load config");

        assert_eq!(config.publish_interval_secs, 7);
        assert_eq!(config.stale_after_secs, 30);
        assert_eq!(config.local_port, 44000);
        assert_eq!(config.working_dir, PathBuf::from("/tmp/flock-work"));
        assert_eq!(config.next_chat_target.as_deref(), Some("node-123"));
        assert_eq!(config.default_host_preference.as_deref(), Some("node-456"));
        assert!(!config.require_healthy_goosed);
        assert_eq!(config.max_cpu_load_pct, 88);
        assert_eq!(config.max_memory_used_pct, 77);
        assert_eq!(config.min_disk_available_bytes, 12345);
        assert_eq!(config.weight_rtt, 2.0);
        assert_eq!(config.weight_active_chats, 3.0);
        assert_eq!(config.weight_cpu_load, 4.0);
        assert_eq!(config.weight_memory_used, 5.0);
    }

    #[test]
    fn invalid_stale_window_is_rejected() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        fs::write(
            temp.path(),
            r#"
[flock.routing]
publish_interval_secs = 10
stale_after_secs = 10
"#,
        )
        .expect("write config");

        let err = RoutingConfig::load(temp.path()).expect_err("config should be rejected");
        assert!(err.to_string().contains("stale_after_secs"));
    }
}
