#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLOCK_BIN="${FLOCK_BIN:-$ROOT_DIR/target/debug/flock}"
MESH_LLM_BIN="${MESH_LLM_BIN:-$ROOT_DIR/../mesh-llm/target/debug/mesh-llm}"
FLOCK_GOOSED_BIN="${FLOCK_GOOSED_BIN:-$ROOT_DIR/../goose/target/debug/goosed}"
LLAMA_BIN_DIR="${LLAMA_BIN_DIR:-$ROOT_DIR/../mesh-llm/llama.cpp/build-flock/bin}"
MESH_MODEL="${FLOCK_TEST_MODEL:-Qwen2.5-3B}"
MESH_MODEL_ID="${FLOCK_TEST_MODEL_ID:-Qwen2.5-3B-Instruct-Q4_K_M}"
MESH_MODEL_FILE="${FLOCK_TEST_MODEL_FILE:-$HOME/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf}"
PORT_SEED="${FLOCK_TEST_PORT_SEED:-$(( (${RANDOM:-0} + $$) % 1000 ))}"
LOCAL_PORT="${FLOCK_TEST_LOCAL_PORT:-$((43000 + PORT_SEED))}"
MESH_API_PORT="${FLOCK_TEST_API_PORT:-$((44000 + PORT_SEED))}"
MESH_CONSOLE_PORT="${FLOCK_TEST_CONSOLE_PORT:-$((45000 + PORT_SEED))}"
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
  if [[ -n "${HOST_LOG:-}" && -f "${HOST_LOG:-}" ]]; then
    echo "--- mesh-llm/flock log ---" >&2
    cat "$HOST_LOG" >&2
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
  if [[ -n "${HOST_PID:-}" ]]; then
    kill "$HOST_PID" >/dev/null 2>&1 || true
    wait "$HOST_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${TEST_ROOT:-}" && -d "${TEST_ROOT:-}" ]]; then
    rm -rf "$TEST_ROOT"
  fi
}

trap cleanup EXIT

for cmd in curl jq mktemp python3; do
  command -v "$cmd" >/dev/null 2>&1 || skip "$cmd is not installed"
done

[[ -x "$FLOCK_BIN" ]] || skip "flock binary not found at $FLOCK_BIN"
[[ -x "$MESH_LLM_BIN" ]] || skip "mesh-llm binary not found at $MESH_LLM_BIN"
[[ -x "$FLOCK_GOOSED_BIN" ]] || skip "goosed binary not found at $FLOCK_GOOSED_BIN"
[[ -x "$LLAMA_BIN_DIR/rpc-server" ]] || skip "rpc-server binary not found at $LLAMA_BIN_DIR/rpc-server"
[[ -x "$LLAMA_BIN_DIR/llama-server" ]] || skip "llama-server binary not found at $LLAMA_BIN_DIR/llama-server"
[[ -f "$MESH_MODEL_FILE" ]] || skip "mesh model file not found at $MESH_MODEL_FILE"

pgrep -f "$MESH_LLM_BIN" >/dev/null 2>&1 && skip "another mesh-llm process is already running locally"
pgrep -f "$LLAMA_BIN_DIR/rpc-server" >/dev/null 2>&1 && skip "another local rpc-server is already running"
pgrep -f "$LLAMA_BIN_DIR/llama-server" >/dev/null 2>&1 && skip "another local llama-server is already running"

if [[ -e /tmp/mesh-llm-llama-server.log ]]; then
  [[ -w /tmp/mesh-llm-llama-server.log ]] || skip "/tmp/mesh-llm-llama-server.log is not writable for the current user"
else
  touch /tmp/mesh-llm-llama-server.log && rm /tmp/mesh-llm-llama-server.log || skip "cannot create /tmp/mesh-llm-llama-server.log"
fi

TEST_ROOT="$(mktemp -d /tmp/flock-local-mesh-e2e.XXXXXX)"
GOOSE_ROOT="$TEST_ROOT/goose-root"
WORKSPACE_DIR="$TEST_ROOT/workspace"
MESH_CONFIG="$TEST_ROOT/mesh-config.toml"
HOST_LOG="$TEST_ROOT/mesh-llm.log"
EVENTS_FILE="$TEST_ROOT/events.log"

mkdir -p "$GOOSE_ROOT/config/custom_providers" "$GOOSE_ROOT/data" "$GOOSE_ROOT/state" "$WORKSPACE_DIR"

cat >"$GOOSE_ROOT/config/config.yaml" <<YAML
GOOSE_PROVIDER: mesh
GOOSE_MODEL: $MESH_MODEL_ID
YAML

cat >"$GOOSE_ROOT/config/custom_providers/mesh.json" <<JSON
{
  "name": "mesh",
  "engine": "openai",
  "display_name": "mesh-llm",
  "description": "Distributed LLM inference via mesh-llm",
  "api_key_env": "",
  "base_url": "http://127.0.0.1:$MESH_API_PORT",
  "models": [
    { "name": "$MESH_MODEL_ID", "context_limit": 65536 }
  ],
  "timeout_seconds": 600,
  "supports_streaming": true,
  "requires_auth": false
}
JSON

cat >"$MESH_CONFIG" <<TOML
[[plugin]]
name = "flock"
enabled = true
command = "$FLOCK_BIN"
args = ["--plugin"]

[flock.routing]
publish_interval_secs = 5
stale_after_secs = 20
local_port = $LOCAL_PORT
working_dir = "$WORKSPACE_DIR"
TOML

GOOSE_PATH_ROOT="$GOOSE_ROOT" \
GOOSE_DISABLE_KEYRING=true \
MESH_LLM_CONFIG="$MESH_CONFIG" \
FLOCK_GOOSED_BIN="$FLOCK_GOOSED_BIN" \
RUST_LOG=warn \
"$MESH_LLM_BIN" --model "$MESH_MODEL" --bin-dir "$LLAMA_BIN_DIR" \
  --port "$MESH_API_PORT" --console "$MESH_CONSOLE_PORT" --config "$MESH_CONFIG" \
  >"$HOST_LOG" 2>&1 &
HOST_PID=$!

for _ in $(seq 1 240); do
  if curl -fsS "http://127.0.0.1:$LOCAL_PORT/status" >/dev/null 2>&1 && \
     curl -fsS "http://127.0.0.1:$MESH_API_PORT/v1/models" | jq -e --arg model "$MESH_MODEL_ID" 'any(.data[]?; .id == $model)' >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

curl -fsS "http://127.0.0.1:$LOCAL_PORT/status" >/dev/null 2>&1 || fail "flock local endpoint did not become ready"
curl -fsS "http://127.0.0.1:$MESH_API_PORT/v1/models" | jq -e --arg model "$MESH_MODEL_ID" 'any(.data[]?; .id == $model)' >/dev/null 2>&1 || fail "mesh-llm did not start serving $MESH_MODEL_ID"

provider="$(curl -fsS -X POST -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  -d '{"key":"GOOSE_PROVIDER","is_secret":false}' "http://127.0.0.1:$LOCAL_PORT/config/read" | jq -r '.')"
model="$(curl -fsS -X POST -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  -d '{"key":"GOOSE_MODEL","is_secret":false}' "http://127.0.0.1:$LOCAL_PORT/config/read" | jq -r '.')"

[[ "$provider" == "mesh" ]] || fail "expected provider 'mesh', got '$provider'"
[[ "$model" == "$MESH_MODEL_ID" ]] || fail "expected model '$MESH_MODEL_ID', got '$model'"

SESSION_ID="$(curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  -d "{\"working_dir\":\"$WORKSPACE_DIR\"}" "http://127.0.0.1:$LOCAL_PORT/agent/start" | jq -r '.id')"
[[ -n "$SESSION_ID" && "$SESSION_ID" != "null" ]] || fail "agent/start did not return a session id"

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

normalized="$(printf '%s' "$assistant_text" | tr '[:upper:]' '[:lower:]' | tr -cd '[:alpha:]')"
[[ "$normalized" == "pong" ]] || fail "final assistant text was '$assistant_text'"

echo "PASS: flock local mesh E2E"
