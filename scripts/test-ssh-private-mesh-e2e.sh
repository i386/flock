#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLOCK_BIN="${FLOCK_BIN:-$ROOT_DIR/target/debug/flock}"
MESH_LLM_BIN="${MESH_LLM_BIN:-$ROOT_DIR/../mesh-llm/target/debug/mesh-llm}"
FLOCK_GOOSED_BIN="${FLOCK_GOOSED_BIN:-$ROOT_DIR/../goose/target/debug/goosed}"
LLAMA_BIN_DIR="${LLAMA_BIN_DIR:-$ROOT_DIR/../mesh-llm/llama.cpp/build-flock/bin}"
SSH_HOST="${FLOCK_SSH_HOST:-}"
REMOTE_SSH_OPTS="${FLOCK_SSH_OPTS:-}"
MESH_MODEL="${FLOCK_TEST_MODEL:-Qwen2.5-3B}"
MESH_MODEL_ID="${FLOCK_TEST_MODEL_ID:-Qwen2.5-3B-Instruct-Q4_K_M}"
MESH_MODEL_FILE="${FLOCK_TEST_MODEL_FILE:-$HOME/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf}"
REMOTE_MODEL_FILE="${FLOCK_TEST_REMOTE_MODEL_FILE:-~/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf}"
PORT_SEED="${FLOCK_TEST_PORT_SEED:-$(( (${RANDOM:-0} + $$) % 1000 ))}"
LOCAL_PORT="${FLOCK_TEST_LOCAL_PORT:-$((43000 + PORT_SEED))}"
LOCAL_MESH_API_PORT="${FLOCK_TEST_LOCAL_API_PORT:-$((44000 + PORT_SEED))}"
LOCAL_MESH_CONSOLE_PORT="${FLOCK_TEST_LOCAL_CONSOLE_PORT:-$((45000 + PORT_SEED))}"
LOCAL_BIND_PORT="${FLOCK_TEST_LOCAL_BIND_PORT:-$((46000 + PORT_SEED))}"
REMOTE_PORT="${FLOCK_TEST_REMOTE_PORT:-$((47000 + PORT_SEED))}"
REMOTE_MESH_API_PORT="${FLOCK_TEST_REMOTE_API_PORT:-$((48000 + PORT_SEED))}"
REMOTE_MESH_CONSOLE_PORT="${FLOCK_TEST_REMOTE_CONSOLE_PORT:-$((49000 + PORT_SEED))}"
REMOTE_BIND_PORT="${FLOCK_TEST_REMOTE_BIND_PORT:-$((50000 + PORT_SEED))}"
LOCAL_SECRET="flock-local"

canonical_path() {
  python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$1"
}

FLOCK_BIN="$(canonical_path "$FLOCK_BIN")"
MESH_LLM_BIN="$(canonical_path "$MESH_LLM_BIN")"
FLOCK_GOOSED_BIN="$(canonical_path "$FLOCK_GOOSED_BIN")"
LLAMA_BIN_DIR="$(canonical_path "$LLAMA_BIN_DIR")"

skip() {
  echo "SKIP: $*"
  exit 0
}

fail() {
  echo "FAIL: $*" >&2
  if [[ -n "${LOCAL_LOG:-}" && -f "${LOCAL_LOG:-}" ]]; then
    echo "--- local mesh log ---" >&2
    cat "$LOCAL_LOG" >&2
  fi
  if [[ -n "${REMOTE_ROOT:-}" && -n "$SSH_HOST" ]]; then
    echo "--- remote mesh log ---" >&2
    ssh $REMOTE_SSH_OPTS "$SSH_HOST" "test -f '$REMOTE_ROOT/mesh.log' && tail -200 '$REMOTE_ROOT/mesh.log'" >&2 || true
  fi
  if [[ -n "${EVENTS_FILE:-}" && -f "${EVENTS_FILE:-}" ]]; then
    echo "--- events ---" >&2
    cat "$EVENTS_FILE" >&2
  fi
  exit 1
}

cleanup() {
  if [[ -n "${EVENTS_PID:-}" ]]; then
    kill "$EVENTS_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${LOCAL_PID:-}" ]]; then
    kill "$LOCAL_PID" >/dev/null 2>&1 || true
    wait "$LOCAL_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${REMOTE_ROOT:-}" && -n "$SSH_HOST" ]]; then
    ssh $REMOTE_SSH_OPTS "$SSH_HOST" "
      if test -f '$REMOTE_ROOT/mesh.pid'; then
        kill \$(cat '$REMOTE_ROOT/mesh.pid') >/dev/null 2>&1 || true
      fi
      pkill -f '$REMOTE_ROOT/bin/mesh-llm' >/dev/null 2>&1 || true
      pkill -f '$REMOTE_ROOT/bin/flock' >/dev/null 2>&1 || true
      pkill -f '$REMOTE_ROOT/bin/goosed' >/dev/null 2>&1 || true
      pkill -f '$REMOTE_ROOT/bin/llama-server' >/dev/null 2>&1 || true
      pkill -f '$REMOTE_ROOT/bin/rpc-server' >/dev/null 2>&1 || true
      rm -rf '$REMOTE_ROOT'
    " >/dev/null 2>&1 || true
  fi
  if [[ -n "${LOCAL_ROOT:-}" && -d "${LOCAL_ROOT:-}" ]]; then
    rm -rf "$LOCAL_ROOT"
  fi
}

trap cleanup EXIT

for cmd in bash curl jq mktemp python3 scp ssh; do
  command -v "$cmd" >/dev/null 2>&1 || skip "$cmd is not installed"
done

[[ -n "$SSH_HOST" ]] || skip "set FLOCK_SSH_HOST to a reachable SSH host"
[[ -x "$FLOCK_BIN" ]] || skip "flock binary not found at $FLOCK_BIN"
[[ -x "$MESH_LLM_BIN" ]] || skip "mesh-llm binary not found at $MESH_LLM_BIN"
[[ -x "$FLOCK_GOOSED_BIN" ]] || skip "goosed binary not found at $FLOCK_GOOSED_BIN"
[[ -x "$LLAMA_BIN_DIR/rpc-server" ]] || skip "rpc-server binary not found at $LLAMA_BIN_DIR/rpc-server"
[[ -x "$LLAMA_BIN_DIR/llama-server" ]] || skip "llama-server binary not found at $LLAMA_BIN_DIR/llama-server"
[[ -f "$MESH_MODEL_FILE" ]] || skip "local mesh model file not found at $MESH_MODEL_FILE"

ssh $REMOTE_SSH_OPTS -o BatchMode=yes "$SSH_HOST" "echo ok" >/dev/null 2>&1 || skip "cannot SSH to $SSH_HOST with BatchMode=yes"
ssh $REMOTE_SSH_OPTS "$SSH_HOST" "test -f $REMOTE_MODEL_FILE" >/dev/null 2>&1 || skip "remote mesh model file not found at $REMOTE_MODEL_FILE on $SSH_HOST"
ssh $REMOTE_SSH_OPTS "$SSH_HOST" '
  if [ -e /tmp/mesh-llm-llama-server.log ]; then
    [ -w /tmp/mesh-llm-llama-server.log ]
  else
    touch /tmp/mesh-llm-llama-server.log && rm /tmp/mesh-llm-llama-server.log
  fi
' >/dev/null 2>&1 || skip "remote host cannot write /tmp/mesh-llm-llama-server.log required by mesh-llm"

LOCAL_ROOT="$(mktemp -d /tmp/flock-ssh-e2e.XXXXXX)"
REMOTE_ROOT="$(ssh $REMOTE_SSH_OPTS "$SSH_HOST" 'mktemp -d /tmp/flock-ssh-e2e.XXXXXX')"
LOCAL_LOG="$LOCAL_ROOT/local-mesh.log"
EVENTS_FILE="$LOCAL_ROOT/events.log"
LOCAL_CONFIG="$LOCAL_ROOT/config.toml"

mkdir -p "$LOCAL_ROOT/work"

scp $REMOTE_SSH_OPTS \
  "$MESH_LLM_BIN" \
  "$FLOCK_BIN" \
  "$FLOCK_GOOSED_BIN" \
  "$LLAMA_BIN_DIR/rpc-server" \
  "$LLAMA_BIN_DIR/llama-server" \
  "$SSH_HOST:$REMOTE_ROOT/" >/dev/null

ssh $REMOTE_SSH_OPTS "$SSH_HOST" "
  mkdir -p '$REMOTE_ROOT/bin' '$REMOTE_ROOT/goose/config/custom_providers' '$REMOTE_ROOT/work'
  mv '$REMOTE_ROOT/mesh-llm' '$REMOTE_ROOT/bin/mesh-llm'
  mv '$REMOTE_ROOT/flock' '$REMOTE_ROOT/bin/flock'
  mv '$REMOTE_ROOT/goosed' '$REMOTE_ROOT/bin/goosed'
  mv '$REMOTE_ROOT/rpc-server' '$REMOTE_ROOT/bin/rpc-server'
  mv '$REMOTE_ROOT/llama-server' '$REMOTE_ROOT/bin/llama-server'
  chmod +x '$REMOTE_ROOT/bin/mesh-llm' '$REMOTE_ROOT/bin/flock' '$REMOTE_ROOT/bin/goosed' '$REMOTE_ROOT/bin/rpc-server' '$REMOTE_ROOT/bin/llama-server'
"

cat >"$LOCAL_CONFIG" <<TOML
[[plugin]]
name = "blackboard"
enabled = false

[[plugin]]
name = "flock"
enabled = true
command = "$FLOCK_BIN"
args = ["--plugin"]

[flock.routing]
publish_interval_secs = 5
stale_after_secs = 20
local_port = $LOCAL_PORT
working_dir = "$LOCAL_ROOT/work"
default_strategy = "balanced"
next_chat_target = ""
default_host_preference = ""
require_healthy_goosed = true
max_cpu_load_pct = 95
max_memory_used_pct = 95
min_disk_available_bytes = 10737418240
weight_rtt = 1.0
weight_active_chats = 15.0
weight_cpu_load = 0.7
weight_memory_used = 0.5
TOML

ssh $REMOTE_SSH_OPTS "$SSH_HOST" "cat > '$REMOTE_ROOT/config.toml' <<'EOF'
[[plugin]]
name = \"blackboard\"
enabled = false

[[plugin]]
name = \"flock\"
enabled = true
command = \"$REMOTE_ROOT/bin/flock\"
args = [\"--plugin\"]

[flock.routing]
publish_interval_secs = 5
stale_after_secs = 20
local_port = $REMOTE_PORT
working_dir = \"$REMOTE_ROOT/work\"
default_strategy = \"balanced\"
next_chat_target = \"\"
default_host_preference = \"\"
require_healthy_goosed = true
max_cpu_load_pct = 95
max_memory_used_pct = 95
min_disk_available_bytes = 10737418240
weight_rtt = 1.0
weight_active_chats = 15.0
weight_cpu_load = 0.7
weight_memory_used = 0.5
EOF

cat > '$REMOTE_ROOT/goose/config/config.yaml' <<'EOF'
GOOSE_PROVIDER: mesh
GOOSE_MODEL: $MESH_MODEL_ID
EOF

cat > '$REMOTE_ROOT/goose/config/custom_providers/mesh.json' <<'EOF'
{
  \"name\": \"mesh\",
  \"engine\": \"openai\",
  \"display_name\": \"mesh-llm\",
  \"description\": \"Distributed LLM inference via mesh-llm\",
  \"api_key_env\": \"\",
  \"base_url\": \"http://127.0.0.1:$REMOTE_MESH_API_PORT\",
  \"models\": [
    { \"name\": \"$MESH_MODEL_ID\", \"context_limit\": 65536 }
  ],
  \"timeout_seconds\": 600,
  \"supports_streaming\": true,
  \"requires_auth\": false
}
EOF"

ssh $REMOTE_SSH_OPTS "$SSH_HOST" "
  cd '$REMOTE_ROOT'
  env \
    MESH_LLM_CONFIG='$REMOTE_ROOT/config.toml' \
    GOOSE_PATH_ROOT='$REMOTE_ROOT/goose' \
    FLOCK_GOOSED_BIN='$REMOTE_ROOT/bin/goosed' \
    nohup '$REMOTE_ROOT/bin/mesh-llm' \
      --model '$MESH_MODEL' \
      --bin-dir '$REMOTE_ROOT/bin' \
      --port '$REMOTE_MESH_API_PORT' \
      --console '$REMOTE_MESH_CONSOLE_PORT' \
      --bind-port '$REMOTE_BIND_PORT' \
      --config '$REMOTE_ROOT/config.toml' \
      > '$REMOTE_ROOT/mesh.log' 2>&1 < /dev/null &
  echo \$! > '$REMOTE_ROOT/mesh.pid'
" >/dev/null

INVITE=""
for _ in $(seq 1 180); do
  INVITE="$(ssh $REMOTE_SSH_OPTS "$SSH_HOST" "curl -fsS http://127.0.0.1:$REMOTE_MESH_CONSOLE_PORT/api/status 2>/dev/null | jq -r '.token // empty'")"
  if [[ -n "$INVITE" ]]; then
    break
  fi
  sleep 1
done
[[ -n "$INVITE" ]] || fail "remote mesh invite token did not appear"

MESH_LLM_CONFIG="$LOCAL_CONFIG" \
FLOCK_GOOSED_BIN="/tmp/flock-goosed-disabled" \
RUST_LOG=warn \
"$MESH_LLM_BIN" --client \
  --join "$INVITE" \
  --port "$LOCAL_MESH_API_PORT" \
  --console "$LOCAL_MESH_CONSOLE_PORT" \
  --config "$LOCAL_CONFIG" \
  >"$LOCAL_LOG" 2>&1 &
LOCAL_PID=$!

for _ in $(seq 1 120); do
  if curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" "http://127.0.0.1:$LOCAL_PORT/flock/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" "http://127.0.0.1:$LOCAL_PORT/flock/health" >/dev/null 2>&1 || fail "local flock endpoint did not become ready"

REMOTE_PEER_ID=""
for _ in $(seq 1 120); do
  REMOTE_PEER_ID="$(curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" "http://127.0.0.1:$LOCAL_PORT/flock/hosts" | jq -r '.[0].peer_id // empty')"
  if [[ -n "$REMOTE_PEER_ID" ]]; then
    break
  fi
  sleep 1
done
[[ -n "$REMOTE_PEER_ID" ]] || fail "local flock did not discover the remote host"

SESSION_ID="$(curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  -d "{\"working_dir\":\"$REMOTE_ROOT/work\"}" \
  "http://127.0.0.1:$LOCAL_PORT/agent/start" | jq -r '.id')"
[[ -n "$SESSION_ID" && "$SESSION_ID" != "null" ]] || fail "agent/start did not return a session id"

curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" "http://127.0.0.1:$LOCAL_PORT/sessions/$SESSION_ID" | \
  jq -e --arg wd "$REMOTE_ROOT/work" --arg provider "mesh" --arg model "$MESH_MODEL_ID" '
    .working_dir == $wd and
    .provider_name == $provider and
    .model_config.model_name == $model
  ' >/dev/null || fail "session metadata did not match the remote mesh-backed session"

grep -q "flock agent/start using remote target=$REMOTE_PEER_ID" "$LOCAL_LOG" || fail "local flock did not route agent/start to the remote peer"

curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  -d "{\"session_id\":\"$SESSION_ID\",\"provider\":\"mesh\",\"model\":\"$MESH_MODEL_ID\"}" \
  "http://127.0.0.1:$LOCAL_PORT/agent/update_provider" >/dev/null

curl -fsS -N --max-time 180 -H "X-Secret-Key: $LOCAL_SECRET" \
  "http://127.0.0.1:$LOCAL_PORT/sessions/$SESSION_ID/events" >"$EVENTS_FILE" 2>/dev/null &
EVENTS_PID=$!

sleep 1

REQUEST_ID="$(python3 - <<'PY'
import uuid
print(uuid.uuid4())
PY
)"
NOW="$(date +%s)"

jq -n \
  --arg rid "$REQUEST_ID" \
  --arg now "$NOW" \
  '{
    request_id: $rid,
    user_message: {
      role: "user",
      created: ($now | tonumber),
      metadata: { agentVisible: true, userVisible: true },
      content: [
        {
          type: "text",
          text: "Reply with exactly the lowercase word pong. Do not call tools."
        }
      ]
    }
  }' | curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  --data-binary @- "http://127.0.0.1:$LOCAL_PORT/sessions/$SESSION_ID/reply" >/dev/null

assistant_text=""
normalized=""
finished=0
for _ in $(seq 1 180); do
  if [[ -f "$EVENTS_FILE" ]] && grep -q '"type":"Finish"' "$EVENTS_FILE"; then
    finished=1
  fi

  assistant_text="$(curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" \
    "http://127.0.0.1:$LOCAL_PORT/sessions/$SESSION_ID" | \
    jq -r '
      [
        .conversation[]?
        | select(.role == "assistant")
        | .content[]?
        | select(.type == "text")
        | .text
      ] | join("")
    ')"

  normalized="$(printf '%s' "$assistant_text" | tr '[:upper:]' '[:lower:]' | tr -cd '[:alpha:]')"
  if [[ "$normalized" == "pong" && "$finished" -eq 1 ]]; then
    break
  fi
  sleep 1
done

kill "$EVENTS_PID" >/dev/null 2>&1 || true
wait "$EVENTS_PID" >/dev/null 2>&1 || true

grep -q '"type":"Message"' "$EVENTS_FILE" || fail "events stream did not emit any message events"
grep -q '"type":"Finish"' "$EVENTS_FILE" || fail "events stream did not emit a finish event"
[[ "$normalized" == "pong" ]] || fail "final assistant text was '$assistant_text'"

echo "PASS: flock SSH private-mesh E2E"
