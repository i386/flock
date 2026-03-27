use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use mesh_llm_plugin::{
    plugin_server_info, PluginMetadata, PluginRuntime, PluginStartupPolicy, SimplePlugin,
};
use std::fs;
use std::path::{Path, PathBuf};

const PLUGIN_ID: &str = "flock";
const DEFAULT_LOCAL_PORT: i64 = 43123;
const DEFAULT_PUBLISH_INTERVAL_SECS: i64 = 5;
const DEFAULT_STALE_AFTER_SECS: i64 = 20;
const DEFAULT_MAX_CPU_LOAD_PCT: i64 = 95;
const DEFAULT_MAX_MEMORY_USED_PCT: i64 = 95;
const DEFAULT_MIN_DISK_AVAILABLE_BYTES: i64 = 10 * 1024 * 1024 * 1024;
const DEFAULT_WEIGHT_RTT: f64 = 1.0;
const DEFAULT_WEIGHT_ACTIVE_CHATS: f64 = 15.0;
const DEFAULT_WEIGHT_CPU_LOAD: f64 = 0.7;
const DEFAULT_WEIGHT_MEMORY_USED: f64 = 0.5;

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
        None => Err(anyhow!("no command provided; use `flock install`, `flock goose`, or `flock --plugin`")),
    }
}

async fn run_plugin() -> Result<()> {
    let plugin = SimplePlugin::new(
        PluginMetadata::new(
            PLUGIN_ID,
            env!("CARGO_PKG_VERSION"),
            plugin_server_info(
                PLUGIN_ID,
                env!("CARGO_PKG_VERSION"),
                "Flock",
                "Private-mesh Goose backend router",
                Some("Routes Goose traffic over a private mesh to remote goosed instances."),
            ),
        )
        .with_startup_policy(PluginStartupPolicy::PrivateMeshOnly),
    )
    .with_health(|_context| Box::pin(async move { Ok("ok".to_string()) }));

    PluginRuntime::run(plugin).await
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
    println!("`flock goose` is not implemented yet.");
    println!(
        "expected local flock endpoint: http://127.0.0.1:{}",
        current_local_port(&paths.config_path).unwrap_or(DEFAULT_LOCAL_PORT as u16)
    );
    Ok(())
}

struct AppPaths {
    mesh_dir: PathBuf,
    config_path: PathBuf,
    installed_binary: PathBuf,
    current_binary: PathBuf,
}

impl AppPaths {
    fn resolve() -> Result<Self> {
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

fn mesh_config_path() -> Result<PathBuf> {
    if let Ok(override_path) = std::env::var("MESH_LLM_CONFIG") {
        return Ok(PathBuf::from(override_path));
    }

    let home = dirs::home_dir().ok_or_else(|| anyhow!("failed to determine home directory"))?;
    Ok(home.join(".mesh-llm").join("config.toml"))
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

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
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

    let plugin_table = plugins
        .iter_mut()
        .find_map(|entry| {
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

    insert_default_int(routing, "publish_interval_secs", DEFAULT_PUBLISH_INTERVAL_SECS);
    insert_default_int(routing, "stale_after_secs", DEFAULT_STALE_AFTER_SECS);
    insert_default_int(routing, "local_port", DEFAULT_LOCAL_PORT);
    insert_default_string(routing, "working_dir", working_dir.display().to_string());
    insert_default_string(routing, "default_strategy", "balanced".to_string());
    insert_default_string(routing, "next_chat_target", String::new());
    insert_default_string(routing, "default_host_preference", String::new());
    insert_default_bool(routing, "require_healthy_goosed", true);
    insert_default_int(routing, "max_cpu_load_pct", DEFAULT_MAX_CPU_LOAD_PCT);
    insert_default_int(
        routing,
        "max_memory_used_pct",
        DEFAULT_MAX_MEMORY_USED_PCT,
    );
    insert_default_int(
        routing,
        "min_disk_available_bytes",
        DEFAULT_MIN_DISK_AVAILABLE_BYTES,
    );
    insert_default_float(routing, "weight_rtt", DEFAULT_WEIGHT_RTT);
    insert_default_float(
        routing,
        "weight_active_chats",
        DEFAULT_WEIGHT_ACTIVE_CHATS,
    );
    insert_default_float(routing, "weight_cpu_load", DEFAULT_WEIGHT_CPU_LOAD);
    insert_default_float(
        routing,
        "weight_memory_used",
        DEFAULT_WEIGHT_MEMORY_USED,
    );
}

fn default_working_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("failed to determine home directory"))?;
    let code_dir = home.join("code");
    if code_dir.exists() {
        Ok(code_dir)
    } else {
        Ok(home)
    }
}

fn current_local_port(config_path: &Path) -> Option<u16> {
    let contents = fs::read_to_string(config_path).ok()?;
    let root = contents.parse::<toml::Value>().ok()?;
    let port = root
        .get("flock")?
        .get("routing")?
        .get("local_port")?
        .as_integer()?;
    u16::try_from(port).ok()
}

fn insert_default_string(table: &mut toml::map::Map<String, toml::Value>, key: &str, value: String) {
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
