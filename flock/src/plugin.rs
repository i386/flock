use crate::config::RoutingConfig;
use crate::goosed::{GoosedStatus, GoosedSupervisor};
use anyhow::Result;
use axum::{extract::State as AxumState, http::StatusCode, routing::get, Json, Router};
use mesh_llm_plugin::{
    plugin_server_info, proto, PluginContext, PluginInitializeRequest, PluginMetadata,
    PluginRuntime, PluginStartupPolicy, SimplePlugin,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Disks, System};
use tokio::sync::Mutex;

const PLUGIN_ID: &str = "flock";
const FLOCK_CHANNEL: &str = "flock";
const FLOCK_PROTOCOL_VERSION: u32 = 1;
const MESSAGE_KIND_ADVERTISEMENT: &str = "flock.advertisement.v1";
const MESSAGE_KIND_SNAPSHOT_REQUEST: &str = "flock.snapshot_request.v1";

#[derive(Debug)]
pub struct PluginState {
    config_path: PathBuf,
    routing: RoutingConfig,
    started_at: Instant,
    last_initialize: Option<PluginInitializeRequest>,
    local: LocalNodeState,
    known_hosts: BTreeMap<String, KnownHost>,
    session_bindings: BTreeMap<String, String>,
    next_chat_target: Option<String>,
    http_server_started: bool,
    goosed: GoosedSupervisor,
}

#[derive(Debug)]
pub struct LocalNodeState {
    hostname: String,
    display_name: String,
    local_peer_id: Option<String>,
    mesh_id: Option<String>,
    last_advertisement: Option<NodeAdvertisement>,
}

#[derive(Debug)]
pub struct KnownHost {
    peer_id: String,
    role: String,
    version: String,
    rtt_ms: Option<u32>,
    capabilities: Vec<String>,
    last_seen: SystemTime,
    advertisement: Option<NodeAdvertisement>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SnapshotRequest {
    protocol_version: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct NodeAdvertisement {
    protocol_version: u32,
    plugin_version: String,
    node_id: Option<String>,
    mesh_id: Option<String>,
    hostname: String,
    display_name: String,
    local_port: u16,
    goosed: GoosedStatus,
    working_dir: String,
    active_chat_count: usize,
    emitted_at_unix_ms: u64,
    os: OsSnapshot,
    cpu: CpuSnapshot,
    memory: MemorySnapshot,
    disk: DiskSnapshot,
    load: LoadSnapshot,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OsSnapshot {
    family: String,
    name: String,
    version: Option<String>,
    kernel_version: Option<String>,
    arch: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CpuSnapshot {
    brand: String,
    vendor_id: String,
    physical_cores: Option<u32>,
    logical_cores: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MemorySnapshot {
    total_bytes: u64,
    used_bytes: u64,
    available_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DiskSnapshot {
    available_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LoadSnapshot {
    cpu_load_pct: f32,
    memory_used_pct: f32,
}

#[derive(Debug, Serialize)]
struct HealthSnapshot<'a> {
    plugin: &'a str,
    config_path: String,
    hostname: &'a str,
    display_name: &'a str,
    local_peer_id: Option<&'a str>,
    mesh_id: Option<&'a str>,
    local_port: u16,
    working_dir: String,
    next_chat_target: Option<&'a str>,
    default_host_preference: Option<&'a str>,
    local_advertisement: Option<&'a NodeAdvertisement>,
    known_host_count: usize,
    known_hosts: Vec<KnownHostSnapshot<'a>>,
    session_binding_count: usize,
    uptime_secs: u64,
    publish_interval_secs: u64,
    stale_after_secs: u64,
}

#[derive(Debug, Serialize)]
struct KnownHostSnapshot<'a> {
    peer_id: &'a str,
    role: &'a str,
    version: &'a str,
    rtt_ms: Option<u32>,
    capability_count: usize,
    hostname: Option<&'a str>,
    operating_system: Option<&'a str>,
    goosed_healthy: Option<bool>,
    goosed_version: Option<&'a str>,
    cpu_load_pct: Option<f32>,
    memory_used_pct: Option<f32>,
}

pub async fn run_plugin(config_path: PathBuf, routing: RoutingConfig) -> Result<()> {
    let hostname = detect_hostname();
    let goosed = GoosedSupervisor::new(routing.local_port);
    let state = Arc::new(Mutex::new(PluginState {
        config_path,
        next_chat_target: routing.next_chat_target.clone(),
        routing,
        started_at: Instant::now(),
        last_initialize: None,
        local: LocalNodeState {
            hostname: hostname.clone(),
            display_name: hostname,
            local_peer_id: None,
            mesh_id: None,
            last_advertisement: None,
        },
        known_hosts: BTreeMap::new(),
        session_bindings: BTreeMap::new(),
        http_server_started: false,
        goosed,
    }));

    {
        let mut state = state.lock().await;
        refresh_local_advertisement(&mut state).await;
    }

    let initialize_state = state.clone();
    let initialized_state = state.clone();
    let health_state = state.clone();
    let mesh_event_state = state.clone();
    let channel_state = state.clone();

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
        .with_capabilities(vec!["mesh-events".to_string(), "channel:flock".to_string()])
        .with_startup_policy(PluginStartupPolicy::PrivateMeshOnly),
    )
    .on_initialize(move |request, _context| {
        let state = initialize_state.clone();
        Box::pin(async move {
            state.lock().await.last_initialize = Some(request);
            Ok(())
        })
    })
    .on_initialized(move |context| {
        let state = initialized_state.clone();
        Box::pin(async move {
            ensure_local_http_server(state.clone()).await?;
            {
                let mut state = state.lock().await;
                state.goosed.ensure_started().await?;
                refresh_local_advertisement(&mut state).await;
            }
            send_local_advertisement(&state, String::new(), context).await?;
            context
                .send_json_channel(
                    FLOCK_CHANNEL,
                    String::new(),
                    MESSAGE_KIND_SNAPSHOT_REQUEST,
                    &SnapshotRequest {
                        protocol_version: FLOCK_PROTOCOL_VERSION,
                    },
                )
                .await?;
            Ok(())
        })
    })
    .with_health(move |context| {
        let state = health_state.clone();
        Box::pin(async move {
            let mut state = state.lock().await;
            state.goosed.ensure_started().await?;
            refresh_local_advertisement(&mut state).await;
            let advertisement = state.local.last_advertisement.clone();
            let detail = health_json(&state)?.to_string();
            drop(state);
            if let Some(advertisement) = advertisement {
                context
                    .send_json_channel(
                        FLOCK_CHANNEL,
                        String::new(),
                        MESSAGE_KIND_ADVERTISEMENT,
                        &advertisement,
                    )
                    .await?;
            }
            Ok(detail)
        })
    })
    .on_mesh_event(move |event, context| {
        let state = mesh_event_state.clone();
        Box::pin(async move {
            let target_peer_id = {
                let mut state = state.lock().await;
                apply_mesh_event(&mut state, &event)
            };
            if let Some(target_peer_id) = target_peer_id {
                send_local_advertisement(&state, target_peer_id, context).await?;
            }
            Ok(())
        })
    })
    .on_channel_message(move |message, context| {
        let state = channel_state.clone();
        Box::pin(async move { handle_channel_message(&state, message, context).await })
    });

    PluginRuntime::run(plugin).await
}

async fn ensure_local_http_server(state: Arc<Mutex<PluginState>>) -> Result<()> {
    let bind_addr = {
        let mut state = state.lock().await;
        if state.http_server_started {
            return Ok(());
        }
        state.http_server_started = true;
        std::net::SocketAddr::from(([127, 0, 0, 1], state.routing.local_port))
    };

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    let app = Router::new()
        .route("/status", get(http_status))
        .route("/flock/health", get(http_flock_health))
        .route("/flock/hosts", get(http_flock_hosts))
        .with_state(state.clone());

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            eprintln!("flock local http server exited: {error}");
            let mut state = state.lock().await;
            state.http_server_started = false;
        }
    });

    Ok(())
}

async fn handle_channel_message(
    state: &Arc<Mutex<PluginState>>,
    message: proto::ChannelMessage,
    context: &mut PluginContext<'_>,
) -> Result<()> {
    if message.channel != FLOCK_CHANNEL {
        return Ok(());
    }

    match message.message_kind.as_str() {
        MESSAGE_KIND_ADVERTISEMENT => {
            let advertisement: NodeAdvertisement = serde_json::from_slice(&message.body)?;
            let source_peer_id = source_peer_id(&message, &advertisement);
            let mut state = state.lock().await;
            merge_advertisement(&mut state, source_peer_id, advertisement);
            Ok(())
        }
        MESSAGE_KIND_SNAPSHOT_REQUEST => {
            let _: SnapshotRequest = serde_json::from_slice(&message.body)?;
            send_local_advertisement(state, message.source_peer_id, context).await
        }
        _ => Ok(()),
    }
}

async fn http_status() -> &'static str {
    "ok"
}

async fn http_flock_health(
    AxumState(state): AxumState<Arc<Mutex<PluginState>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut state = state.lock().await;
    let _ = state.goosed.ensure_started().await;
    refresh_local_advertisement(&mut state).await;
    health_json(&state)
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn http_flock_hosts(
    AxumState(state): AxumState<Arc<Mutex<PluginState>>>,
) -> Json<Vec<serde_json::Value>> {
    let state = state.lock().await;
    Json(
        state
            .known_hosts
            .values()
            .map(|host| {
                serde_json::json!({
                    "peer_id": host.peer_id,
                    "role": host.role,
                    "version": host.version,
                    "rtt_ms": host.rtt_ms,
                    "capabilities": host.capabilities,
                    "last_seen_unix_ms": unix_time_millis(host.last_seen),
                    "advertisement": host.advertisement,
                })
            })
            .collect(),
    )
}

fn source_peer_id(message: &proto::ChannelMessage, advertisement: &NodeAdvertisement) -> String {
    if !message.source_peer_id.trim().is_empty() {
        message.source_peer_id.clone()
    } else if let Some(node_id) = advertisement.node_id.as_ref() {
        node_id.clone()
    } else {
        "unknown-peer".to_string()
    }
}

async fn send_local_advertisement(
    state: &Arc<Mutex<PluginState>>,
    target_peer_id: String,
    context: &mut PluginContext<'_>,
) -> Result<()> {
    let advertisement = {
        let mut state = state.lock().await;
        state.goosed.ensure_started().await?;
        refresh_local_advertisement(&mut state).await;
        state.local.last_advertisement.clone()
    };

    if let Some(advertisement) = advertisement {
        context
            .send_json_channel(
                FLOCK_CHANNEL,
                target_peer_id,
                MESSAGE_KIND_ADVERTISEMENT,
                &advertisement,
            )
            .await?;
    }
    Ok(())
}

fn apply_mesh_event(state: &mut PluginState, event: &proto::MeshEvent) -> Option<String> {
    if !event.local_peer_id.is_empty() {
        state.local.local_peer_id = Some(event.local_peer_id.clone());
    }
    if !event.mesh_id.is_empty() {
        state.local.mesh_id = Some(event.mesh_id.clone());
    }

    let mut advertise_to_peer = None;
    if let Some(peer) = event.peer.as_ref() {
        let kind = proto::mesh_event::Kind::try_from(event.kind)
            .unwrap_or(proto::mesh_event::Kind::Unspecified);

        match kind {
            proto::mesh_event::Kind::PeerDown => {
                state.known_hosts.remove(&peer.peer_id);
            }
            proto::mesh_event::Kind::PeerUp | proto::mesh_event::Kind::PeerUpdated => {
                let entry = state
                    .known_hosts
                    .entry(peer.peer_id.clone())
                    .or_insert_with(|| KnownHost {
                        peer_id: peer.peer_id.clone(),
                        role: peer.role.clone(),
                        version: peer.version.clone(),
                        rtt_ms: peer.rtt_ms,
                        capabilities: peer.capabilities.clone(),
                        last_seen: SystemTime::now(),
                        advertisement: None,
                    });
                entry.role = peer.role.clone();
                entry.version = peer.version.clone();
                entry.rtt_ms = peer.rtt_ms;
                entry.capabilities = peer.capabilities.clone();
                entry.last_seen = SystemTime::now();
                advertise_to_peer = Some(peer.peer_id.clone());
            }
            proto::mesh_event::Kind::LocalAccepting
            | proto::mesh_event::Kind::LocalStandby
            | proto::mesh_event::Kind::MeshIdUpdated
            | proto::mesh_event::Kind::Unspecified => {}
        }
    }

    prune_stale_hosts(state);
    advertise_to_peer
}

fn merge_advertisement(state: &mut PluginState, peer_id: String, advertisement: NodeAdvertisement) {
    let entry = state
        .known_hosts
        .entry(peer_id.clone())
        .or_insert_with(|| KnownHost {
            peer_id,
            role: "flock".to_string(),
            version: advertisement.plugin_version.clone(),
            rtt_ms: None,
            capabilities: vec!["channel:flock".to_string()],
            last_seen: SystemTime::now(),
            advertisement: None,
        });

    entry.version = advertisement.plugin_version.clone();
    entry.last_seen = SystemTime::now();
    entry.advertisement = Some(advertisement);

    prune_stale_hosts(state);
}

async fn refresh_local_advertisement(state: &mut PluginState) {
    let healthy = state.goosed.health_check().await;
    if let Ok(advertisement) = build_local_advertisement(state, state.goosed.snapshot(healthy)) {
        state.local.last_advertisement = Some(advertisement);
    }
}

fn build_local_advertisement(
    state: &PluginState,
    goosed: GoosedStatus,
) -> Result<NodeAdvertisement> {
    let mut system = System::new_all();
    system.refresh_all();

    let cpu_load_pct = system.global_cpu_usage();
    let total_memory_bytes = system.total_memory();
    let used_memory_bytes = system.used_memory();
    let available_memory_bytes = total_memory_bytes.saturating_sub(used_memory_bytes);
    let memory_used_pct = percentage(used_memory_bytes, total_memory_bytes);
    let available_disk_bytes = estimate_available_disk_bytes(&state.routing.working_dir);

    let cpu_brand = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or_default();
    let cpu_vendor_id = system
        .cpus()
        .first()
        .map(|cpu| cpu.vendor_id().to_string())
        .unwrap_or_default();

    Ok(NodeAdvertisement {
        protocol_version: FLOCK_PROTOCOL_VERSION,
        plugin_version: env!("CARGO_PKG_VERSION").to_string(),
        node_id: state.local.local_peer_id.clone(),
        mesh_id: state.local.mesh_id.clone(),
        hostname: state.local.hostname.clone(),
        display_name: state.local.display_name.clone(),
        local_port: state.routing.local_port,
        goosed,
        working_dir: state.routing.working_dir.display().to_string(),
        active_chat_count: state.session_bindings.len(),
        emitted_at_unix_ms: unix_time_millis(SystemTime::now()),
        os: OsSnapshot {
            family: std::env::consts::OS.to_string(),
            name: System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
            version: System::long_os_version().or_else(System::os_version),
            kernel_version: System::kernel_version(),
            arch: std::env::consts::ARCH.to_string(),
        },
        cpu: CpuSnapshot {
            brand: cpu_brand,
            vendor_id: cpu_vendor_id,
            physical_cores: System::physical_core_count().map(|count| count as u32),
            logical_cores: system.cpus().len() as u32,
        },
        memory: MemorySnapshot {
            total_bytes: total_memory_bytes,
            used_bytes: used_memory_bytes,
            available_bytes: available_memory_bytes,
        },
        disk: DiskSnapshot {
            available_bytes: available_disk_bytes,
        },
        load: LoadSnapshot {
            cpu_load_pct,
            memory_used_pct,
        },
    })
}

fn health_json(state: &PluginState) -> Result<serde_json::Value> {
    let snapshot = HealthSnapshot {
        plugin: PLUGIN_ID,
        config_path: state.config_path.display().to_string(),
        hostname: &state.local.hostname,
        display_name: &state.local.display_name,
        local_peer_id: state.local.local_peer_id.as_deref(),
        mesh_id: state.local.mesh_id.as_deref(),
        local_port: state.routing.local_port,
        working_dir: state.routing.working_dir.display().to_string(),
        next_chat_target: state.next_chat_target.as_deref(),
        default_host_preference: state.routing.default_host_preference.as_deref(),
        local_advertisement: state.local.last_advertisement.as_ref(),
        known_host_count: state.known_hosts.len(),
        known_hosts: state
            .known_hosts
            .values()
            .map(|host| KnownHostSnapshot {
                peer_id: &host.peer_id,
                role: &host.role,
                version: &host.version,
                rtt_ms: host.rtt_ms,
                capability_count: host.capabilities.len(),
                hostname: host
                    .advertisement
                    .as_ref()
                    .map(|value| value.hostname.as_str()),
                operating_system: host
                    .advertisement
                    .as_ref()
                    .map(|value| value.os.name.as_str()),
                goosed_healthy: host
                    .advertisement
                    .as_ref()
                    .map(|value| value.goosed.healthy),
                goosed_version: host
                    .advertisement
                    .as_ref()
                    .and_then(|value| value.goosed.version.as_deref()),
                cpu_load_pct: host
                    .advertisement
                    .as_ref()
                    .map(|value| value.load.cpu_load_pct),
                memory_used_pct: host
                    .advertisement
                    .as_ref()
                    .map(|value| value.load.memory_used_pct),
            })
            .collect(),
        session_binding_count: state.session_bindings.len(),
        uptime_secs: state.started_at.elapsed().as_secs(),
        publish_interval_secs: state.routing.publish_interval_secs,
        stale_after_secs: state.routing.stale_after_secs,
    };
    Ok(serde_json::to_value(snapshot)?)
}

fn estimate_available_disk_bytes(working_dir: &Path) -> u64 {
    let resolved_dir = fs::canonicalize(working_dir).unwrap_or_else(|_| working_dir.to_path_buf());
    let disks = Disks::new_with_refreshed_list();

    disks
        .iter()
        .filter(|disk| resolved_dir.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space())
        .unwrap_or(0)
}

fn percentage(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        ((used as f64 / total as f64) * 100.0) as f32
    }
}

fn unix_time_millis(value: SystemTime) -> u64 {
    value
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn prune_stale_hosts(state: &mut PluginState) {
    let stale_after = Duration::from_secs(state.routing.stale_after_secs);
    let now = SystemTime::now();
    state
        .known_hosts
        .retain(|_, host| match now.duration_since(host.last_seen) {
            Ok(elapsed) => elapsed <= stale_after,
            Err(_) => true,
        });
}

fn detect_hostname() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("COMPUTERNAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .map(|value| sanitize_hostname(&value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown-node".to_string())
}

fn sanitize_hostname(raw: &str) -> String {
    let raw = raw.trim().trim_end_matches(".local").to_lowercase();
    let mut result = String::with_capacity(raw.len());
    let mut previous_dash = false;

    for ch in raw.chars() {
        let normalized = match ch {
            'a'..='z' | '0'..='9' | '.' => Some(ch),
            ' ' | '_' | '-' => Some('-'),
            _ => None,
        };

        if let Some(ch) = normalized {
            if ch == '-' {
                if previous_dash || result.is_empty() {
                    continue;
                }
                previous_dash = true;
            } else {
                previous_dash = false;
            }
            result.push(ch);
        }
    }

    result.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        merge_advertisement, percentage, sanitize_hostname, CpuSnapshot, DiskSnapshot, KnownHost,
        LoadSnapshot, LocalNodeState, MemorySnapshot, NodeAdvertisement, OsSnapshot, PluginState,
    };
    use crate::config::RoutingConfig;
    use crate::goosed::{GoosedStatus, GoosedSupervisor};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::{Instant, SystemTime};

    #[test]
    fn hostname_is_normalized_for_display() {
        assert_eq!(sanitize_hostname("JDs-Mac-Studio.local"), "jds-mac-studio");
        assert_eq!(sanitize_hostname("my_host name"), "my-host-name");
    }

    #[test]
    fn unsupported_characters_are_dropped() {
        assert_eq!(sanitize_hostname("host!@#name"), "hostname");
        assert_eq!(sanitize_hostname("--- spaced ---"), "spaced");
    }

    #[test]
    fn percentage_handles_zero_total() {
        assert_eq!(percentage(0, 0), 0.0);
        assert_eq!(percentage(50, 200), 25.0);
    }

    #[test]
    fn advertisements_attach_to_known_hosts() {
        let mut state = PluginState {
            config_path: PathBuf::from("/tmp/flock.toml"),
            routing: RoutingConfig::default(),
            started_at: Instant::now(),
            last_initialize: None,
            local: LocalNodeState {
                hostname: "local".to_string(),
                display_name: "local".to_string(),
                local_peer_id: None,
                mesh_id: None,
                last_advertisement: None,
            },
            known_hosts: BTreeMap::new(),
            session_bindings: BTreeMap::new(),
            next_chat_target: None,
            http_server_started: false,
            goosed: GoosedSupervisor::new(43123),
        };

        state.known_hosts.insert(
            "peer-a".to_string(),
            KnownHost {
                peer_id: "peer-a".to_string(),
                role: "flock".to_string(),
                version: "0.0.0".to_string(),
                rtt_ms: Some(12),
                capabilities: vec!["channel:flock".to_string()],
                last_seen: SystemTime::now(),
                advertisement: None,
            },
        );

        merge_advertisement(
            &mut state,
            "peer-a".to_string(),
            NodeAdvertisement {
                protocol_version: 1,
                plugin_version: "0.1.0".to_string(),
                node_id: Some("peer-a".to_string()),
                mesh_id: Some("mesh".to_string()),
                hostname: "host-a".to_string(),
                display_name: "host-a".to_string(),
                local_port: 43123,
                goosed: GoosedStatus {
                    available: true,
                    healthy: true,
                    version: Some("goosed 0.1.0".to_string()),
                    port: 43124,
                    binary_path: Some("/tmp/goosed".to_string()),
                    last_error: None,
                },
                working_dir: "/tmp".to_string(),
                active_chat_count: 0,
                emitted_at_unix_ms: 0,
                os: OsSnapshot {
                    family: "unix".to_string(),
                    name: "macOS".to_string(),
                    version: None,
                    kernel_version: None,
                    arch: "aarch64".to_string(),
                },
                cpu: CpuSnapshot {
                    brand: "Test CPU".to_string(),
                    vendor_id: "test".to_string(),
                    physical_cores: Some(4),
                    logical_cores: 8,
                },
                memory: MemorySnapshot {
                    total_bytes: 1,
                    used_bytes: 1,
                    available_bytes: 0,
                },
                disk: DiskSnapshot { available_bytes: 1 },
                load: LoadSnapshot {
                    cpu_load_pct: 12.5,
                    memory_used_pct: 40.0,
                },
            },
        );

        let host = state.known_hosts.get("peer-a").expect("known host");
        assert_eq!(host.version, "0.1.0");
        assert_eq!(
            host.advertisement
                .as_ref()
                .map(|value| value.hostname.as_str()),
            Some("host-a")
        );
    }
}
