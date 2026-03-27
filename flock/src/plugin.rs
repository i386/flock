use crate::config::RoutingConfig;
use anyhow::Result;
use mesh_llm_plugin::{
    plugin_server_info, proto, PluginInitializeRequest, PluginMetadata, PluginRuntime,
    PluginStartupPolicy, SimplePlugin,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex;

const PLUGIN_ID: &str = "flock";

#[derive(Debug)]
pub struct PluginState {
    pub config_path: PathBuf,
    pub routing: RoutingConfig,
    pub started_at: Instant,
    pub last_initialize: Option<PluginInitializeRequest>,
    pub local: LocalNodeState,
    pub known_hosts: BTreeMap<String, KnownHost>,
    pub session_bindings: BTreeMap<String, String>,
    pub next_chat_target: Option<String>,
}

#[derive(Debug)]
pub struct LocalNodeState {
    pub hostname: String,
    pub display_name: String,
    pub local_peer_id: Option<String>,
    pub mesh_id: Option<String>,
}

#[derive(Debug)]
pub struct KnownHost {
    pub peer_id: String,
    pub role: String,
    pub version: String,
    pub rtt_ms: Option<u32>,
    pub capabilities: Vec<String>,
    pub last_seen: SystemTime,
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
}

pub async fn run_plugin(config_path: PathBuf, routing: RoutingConfig) -> Result<()> {
    let hostname = detect_hostname();
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
        },
        known_hosts: BTreeMap::new(),
        session_bindings: BTreeMap::new(),
    }));

    let initialize_state = state.clone();
    let health_state = state.clone();
    let mesh_event_state = state.clone();

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
        .with_capabilities(vec!["mesh-events".to_string()])
        .with_startup_policy(PluginStartupPolicy::PrivateMeshOnly),
    )
    .on_initialize(move |request, _context| {
        let state = initialize_state.clone();
        Box::pin(async move {
            state.lock().await.last_initialize = Some(request);
            Ok(())
        })
    })
    .with_health(move |_context| {
        let state = health_state.clone();
        Box::pin(async move {
            let state = state.lock().await;
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
                    })
                    .collect(),
                session_binding_count: state.session_bindings.len(),
                uptime_secs: state.started_at.elapsed().as_secs(),
                publish_interval_secs: state.routing.publish_interval_secs,
                stale_after_secs: state.routing.stale_after_secs,
            };
            Ok(serde_json::to_string(&snapshot)?)
        })
    })
    .on_mesh_event(move |event, _context| {
        let state = mesh_event_state.clone();
        Box::pin(async move {
            let mut state = state.lock().await;
            apply_mesh_event(&mut state, event);
            Ok(())
        })
    });

    PluginRuntime::run(plugin).await
}

fn apply_mesh_event(state: &mut PluginState, event: proto::MeshEvent) {
    if !event.local_peer_id.is_empty() {
        state.local.local_peer_id = Some(event.local_peer_id.clone());
    }
    if !event.mesh_id.is_empty() {
        state.local.mesh_id = Some(event.mesh_id.clone());
    }

    if let Some(peer) = event.peer {
        let kind = proto::mesh_event::Kind::try_from(event.kind)
            .unwrap_or(proto::mesh_event::Kind::Unspecified);

        match kind {
            proto::mesh_event::Kind::PeerDown => {
                state.known_hosts.remove(&peer.peer_id);
            }
            proto::mesh_event::Kind::PeerUp | proto::mesh_event::Kind::PeerUpdated => {
                state.known_hosts.insert(
                    peer.peer_id.clone(),
                    KnownHost {
                        peer_id: peer.peer_id,
                        role: peer.role,
                        version: peer.version,
                        rtt_ms: peer.rtt_ms,
                        capabilities: peer.capabilities,
                        last_seen: SystemTime::now(),
                    },
                );
            }
            proto::mesh_event::Kind::LocalAccepting
            | proto::mesh_event::Kind::LocalStandby
            | proto::mesh_event::Kind::MeshIdUpdated
            | proto::mesh_event::Kind::Unspecified => {}
        }
    }

    prune_stale_hosts(state);
}

fn prune_stale_hosts(state: &mut PluginState) {
    let stale_after = Duration::from_secs(state.routing.stale_after_secs);
    let now = SystemTime::now();
    state.known_hosts.retain(|_, host| match now.duration_since(host.last_seen) {
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
    use super::sanitize_hostname;

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
}
