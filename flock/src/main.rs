mod config;
mod goosed;
mod plugin;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use config::{default_working_dir, same_file, AppPaths, RoutingConfig};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

const PLUGIN_ID: &str = "flock";
const DEFAULT_LOCAL_SECRET: &str = "flock-local";
use config::{
    mesh_config_path, DEFAULT_LOCAL_PORT, DEFAULT_MAX_CPU_LOAD_PCT, DEFAULT_MAX_MEMORY_USED_PCT,
    DEFAULT_MIN_DISK_AVAILABLE_BYTES, DEFAULT_PUBLISH_INTERVAL_SECS, DEFAULT_STALE_AFTER_SECS,
    DEFAULT_WEIGHT_ACTIVE_CHATS, DEFAULT_WEIGHT_CPU_LOAD, DEFAULT_WEIGHT_MEMORY_USED,
    DEFAULT_WEIGHT_RTT,
};

#[derive(Parser, Debug)]
#[command(name = "flock")]
#[command(version, about = "Remote Goose execution over a private mesh-llm mesh")]
struct Cli {
    #[arg(long)]
    plugin: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Install,
    Goose,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.plugin {
        return run_plugin().await;
    }

    match cli.command {
        Some(Command::Install) => install(),
        Some(Command::Goose) => goose(),
        None => Err(anyhow!(
            "no command provided; use `flock install`, `flock goose`, or `flock --plugin`"
        )),
    }
}

async fn run_plugin() -> Result<()> {
    let config_path = mesh_config_path()?;
    let routing = RoutingConfig::load(&config_path)?;
    plugin::run_plugin(config_path, routing).await
}

fn install() -> Result<()> {
    let paths = AppPaths::resolve()?;
    fs::create_dir_all(&paths.mesh_dir)
        .with_context(|| format!("failed to create {}", paths.mesh_dir.display()))?;

    install_binary(&paths)?;
    write_or_update_config(&paths)?;

    println!("installed flock to {}", paths.installed_binary.display());
    println!("updated config at {}", paths.config_path.display());
    println!("plugin entry uses `--plugin` and routing defaults are present");

    Ok(())
}

fn goose() -> Result<()> {
    let paths = AppPaths::resolve()?;
    let routing = RoutingConfig::load(&paths.config_path)?;
    let settings_path = goose_settings_path()?;
    configure_goose_external_backend(&settings_path, routing.local_port, DEFAULT_LOCAL_SECRET)?;

    println!(
        "configured Goose external backend at {}",
        settings_path.display()
    );
    println!(
        "Goose will use http://127.0.0.1:{} with a local flock secret",
        routing.local_port
    );
    println!("mesh-llm with flock plugin mode must be running for the endpoint to respond");

    launch_goose_if_available()?;
    Ok(())
}

fn goose_settings_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("failed to determine home directory"))?;
    #[cfg(target_os = "macos")]
    {
        return Ok(home
            .join("Library")
            .join("Application Support")
            .join("Goose")
            .join("settings.json"));
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg_config_home)
                .join("Goose")
                .join("settings.json"));
        }
        return Ok(home.join(".config").join("Goose").join("settings.json"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Ok(PathBuf::from(appdata).join("Goose").join("settings.json"));
        }
        return Ok(home
            .join("AppData")
            .join("Roaming")
            .join("Goose")
            .join("settings.json"));
    }

    #[allow(unreachable_code)]
    Ok(home.join(".config").join("Goose").join("settings.json"))
}

fn configure_goose_external_backend(
    settings_path: &Path,
    local_port: u16,
    secret: &str,
) -> Result<()> {
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut root = if settings_path.exists() {
        let contents = fs::read_to_string(settings_path)
            .with_context(|| format!("failed to read {}", settings_path.display()))?;
        serde_json::from_str::<serde_json::Value>(&contents)
            .with_context(|| format!("failed to parse {}", settings_path.display()))?
    } else {
        json!({})
    };

    let object = root
        .as_object_mut()
        .ok_or_else(|| anyhow!("Goose settings file must contain a top-level JSON object"))?;
    object.insert(
        "externalGoosed".to_string(),
        json!({
            "enabled": true,
            "url": format!("http://127.0.0.1:{local_port}"),
            "secret": secret,
        }),
    );

    fs::write(settings_path, serde_json::to_string_pretty(&root)?)
        .with_context(|| format!("failed to write {}", settings_path.display()))?;
    Ok(())
}

fn launch_goose_if_available() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = ProcessCommand::new("open").arg("-a").arg("Goose").status();
        match status {
            Ok(status) if status.success() => {
                println!("launched Goose");
                return Ok(());
            }
            Ok(_) | Err(_) => {
                println!("Goose app was not launched automatically");
                return Ok(());
            }
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}

fn install_binary(paths: &AppPaths) -> Result<()> {
    if same_file(&paths.current_binary, &paths.installed_binary) {
        return Ok(());
    }

    fs::copy(&paths.current_binary, &paths.installed_binary).with_context(|| {
        format!(
            "failed to copy {} to {}",
            paths.current_binary.display(),
            paths.installed_binary.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&paths.installed_binary)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&paths.installed_binary, perms)?;
    }

    Ok(())
}

fn write_or_update_config(paths: &AppPaths) -> Result<()> {
    let mut root = if paths.config_path.exists() {
        let contents = fs::read_to_string(&paths.config_path)
            .with_context(|| format!("failed to read {}", paths.config_path.display()))?;
        contents
            .parse::<toml::Value>()
            .with_context(|| format!("failed to parse {}", paths.config_path.display()))?
    } else {
        toml::Value::Table(Default::default())
    };

    ensure_plugin_entry(&mut root, &paths.installed_binary)?;
    ensure_routing_defaults(&mut root, &default_working_dir()?);

    let rendered = toml::to_string_pretty(&root).context("failed to render config toml")?;
    fs::write(&paths.config_path, rendered)
        .with_context(|| format!("failed to write {}", paths.config_path.display()))?;

    Ok(())
}

fn ensure_plugin_entry(root: &mut toml::Value, installed_binary: &Path) -> Result<()> {
    let table = root
        .as_table_mut()
        .ok_or_else(|| anyhow!("top-level config must be a TOML table"))?;

    let plugins = table
        .entry("plugin")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| anyhow!("`plugin` must be an array of tables"))?;

    let plugin_table = plugins.iter_mut().find_map(|entry| {
        let table = entry.as_table_mut()?;
        let name = table.get("name")?.as_str()?;
        if name == PLUGIN_ID {
            Some(table)
        } else {
            None
        }
    });

    let plugin_table = match plugin_table {
        Some(table) => table,
        None => {
            plugins.push(toml::Value::Table(Default::default()));
            plugins
                .last_mut()
                .and_then(toml::Value::as_table_mut)
                .ok_or_else(|| anyhow!("failed to create plugin table"))?
        }
    };

    plugin_table.insert("name".into(), toml::Value::String(PLUGIN_ID.to_string()));
    plugin_table.insert("enabled".into(), toml::Value::Boolean(true));
    plugin_table.insert(
        "command".into(),
        toml::Value::String(installed_binary.display().to_string()),
    );
    plugin_table.insert(
        "args".into(),
        toml::Value::Array(vec![toml::Value::String("--plugin".to_string())]),
    );

    Ok(())
}

fn ensure_routing_defaults(root: &mut toml::Value, working_dir: &Path) {
    let table = root.as_table_mut().expect("top-level config must be table");

    let flock_table = table
        .entry("flock")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .expect("flock table must be a table");

    let routing = flock_table
        .entry("routing")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .expect("flock.routing must be a table");

    insert_default_int(
        routing,
        "publish_interval_secs",
        DEFAULT_PUBLISH_INTERVAL_SECS,
    );
    insert_default_int(routing, "stale_after_secs", DEFAULT_STALE_AFTER_SECS);
    insert_default_int(routing, "local_port", DEFAULT_LOCAL_PORT);
    insert_default_string(routing, "working_dir", working_dir.display().to_string());
    insert_default_string(routing, "default_strategy", "balanced".to_string());
    insert_default_string(routing, "next_chat_target", String::new());
    insert_default_string(routing, "default_host_preference", String::new());
    insert_default_bool(routing, "require_healthy_goosed", true);
    insert_default_int(routing, "max_cpu_load_pct", DEFAULT_MAX_CPU_LOAD_PCT);
    insert_default_int(routing, "max_memory_used_pct", DEFAULT_MAX_MEMORY_USED_PCT);
    insert_default_int(
        routing,
        "min_disk_available_bytes",
        DEFAULT_MIN_DISK_AVAILABLE_BYTES,
    );
    insert_default_float(routing, "weight_rtt", DEFAULT_WEIGHT_RTT);
    insert_default_float(routing, "weight_active_chats", DEFAULT_WEIGHT_ACTIVE_CHATS);
    insert_default_float(routing, "weight_cpu_load", DEFAULT_WEIGHT_CPU_LOAD);
    insert_default_float(routing, "weight_memory_used", DEFAULT_WEIGHT_MEMORY_USED);
}

fn insert_default_string(
    table: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    value: String,
) {
    table
        .entry(key.to_string())
        .or_insert_with(|| toml::Value::String(value));
}

fn insert_default_bool(table: &mut toml::map::Map<String, toml::Value>, key: &str, value: bool) {
    table
        .entry(key.to_string())
        .or_insert_with(|| toml::Value::Boolean(value));
}

fn insert_default_int(table: &mut toml::map::Map<String, toml::Value>, key: &str, value: i64) {
    table
        .entry(key.to_string())
        .or_insert_with(|| toml::Value::Integer(value));
}

fn insert_default_float(table: &mut toml::map::Map<String, toml::Value>, key: &str, value: f64) {
    table
        .entry(key.to_string())
        .or_insert_with(|| toml::Value::Float(value));
}

#[cfg(test)]
mod tests {
    use super::configure_goose_external_backend;
    use std::fs;

    #[test]
    fn goose_external_backend_config_is_written() {
        let temp = tempfile::tempdir().expect("temp dir");
        let settings_path = temp.path().join("settings.json");

        configure_goose_external_backend(&settings_path, 43123, "test-secret").expect("config");

        let contents = fs::read_to_string(settings_path).expect("settings contents");
        let parsed: serde_json::Value = serde_json::from_str(&contents).expect("json");
        assert_eq!(
            parsed["externalGoosed"],
            serde_json::json!({
                "enabled": true,
                "url": "http://127.0.0.1:43123",
                "secret": "test-secret",
            })
        );
    }
}
