#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLOCK_BIN="${FLOCK_BIN:-$ROOT_DIR/target/debug/flock}"
MESH_LLM_BIN="${MESH_LLM_BIN:-$ROOT_DIR/../mesh-llm/target/debug/mesh-llm}"
FLOCK_GOOSED_BIN="${FLOCK_GOOSED_BIN:-$ROOT_DIR/../goose/target/debug/goosed}"
OLLAMA_MODEL="${OLLAMA_MODEL:-llama3.1}"
LOCAL_PORT="${FLOCK_TEST_LOCAL_PORT:-44123}"
MESH_API_PORT="${FLOCK_TEST_API_PORT:-9547}"
MESH_CONSOLE_PORT="${FLOCK_TEST_CONSOLE_PORT:-3347}"
LOCAL_SECRET="flock-local"

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

for cmd in curl jq mktemp ollama python3; do
  command -v "$cmd" >/dev/null 2>&1 || skip "$cmd is not installed"
done

[[ -x "$FLOCK_BIN" ]] || skip "flock binary not found at $FLOCK_BIN"
[[ -x "$MESH_LLM_BIN" ]] || skip "mesh-llm binary not found at $MESH_LLM_BIN"
[[ -x "$FLOCK_GOOSED_BIN" ]] || skip "goosed binary not found at $FLOCK_GOOSED_BIN"

if ! curl -fsS http://127.0.0.1:11434/api/tags >/dev/null 2>&1; then
  skip "local Ollama is not running on 127.0.0.1:11434"
fi

if ! curl -fsS http://127.0.0.1:11434/api/tags | jq -e --arg model "$OLLAMA_MODEL" '
  any(.models[]?; .name == $model or .name == ($model + ":latest"))
' >/dev/null; then
  skip "Ollama model '$OLLAMA_MODEL' is not present"
fi

TEST_ROOT="$(mktemp -d /tmp/flock-ollama-e2e.XXXXXX)"
GOOSE_ROOT="$TEST_ROOT/goose-root"
WORKSPACE_DIR="$TEST_ROOT/workspace"
MESH_CONFIG="$TEST_ROOT/mesh-config.toml"
HOST_LOG="$TEST_ROOT/mesh-llm.log"
EVENTS_FILE="$TEST_ROOT/events.log"

mkdir -p "$GOOSE_ROOT/config" "$GOOSE_ROOT/data" "$GOOSE_ROOT/state" "$WORKSPACE_DIR"

cat >"$GOOSE_ROOT/config/config.yaml" <<YAML
GOOSE_PROVIDER: ollama
GOOSE_MODEL: $OLLAMA_MODEL
OLLAMA_HOST: localhost
YAML

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
"$MESH_LLM_BIN" --client --port "$MESH_API_PORT" --console "$MESH_CONSOLE_PORT" \
  >"$HOST_LOG" 2>&1 &
HOST_PID=$!

for _ in $(seq 1 120); do
  if curl -fsS "http://127.0.0.1:$LOCAL_PORT/status" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

curl -fsS "http://127.0.0.1:$LOCAL_PORT/status" >/dev/null 2>&1 || fail "flock local endpoint did not become ready"

provider="$(curl -fsS -X POST -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  -d '{"key":"GOOSE_PROVIDER","is_secret":false}' "http://127.0.0.1:$LOCAL_PORT/config/read" | jq -r '.')"
model="$(curl -fsS -X POST -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  -d '{"key":"GOOSE_MODEL","is_secret":false}' "http://127.0.0.1:$LOCAL_PORT/config/read" | jq -r '.')"

[[ "$provider" == "ollama" ]] || fail "expected provider 'ollama', got '$provider'"
[[ "$model" == "$OLLAMA_MODEL" ]] || fail "expected model '$OLLAMA_MODEL', got '$model'"

SESSION_ID="$(curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  -d "{\"working_dir\":\"$WORKSPACE_DIR\"}" "http://127.0.0.1:$LOCAL_PORT/agent/start" | jq -r '.id')"
[[ -n "$SESSION_ID" && "$SESSION_ID" != "null" ]] || fail "agent/start did not return a session id"

curl -fsS -H "X-Secret-Key: $LOCAL_SECRET" -H 'Content-Type: application/json' \
  -d "{\"session_id\":\"$SESSION_ID\",\"provider\":\"ollama\",\"model\":\"$OLLAMA_MODEL\"}" \
  "http://127.0.0.1:$LOCAL_PORT/agent/update_provider" >/dev/null

curl -fsS -N --max-time 90 -H "X-Secret-Key: $LOCAL_SECRET" \
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
for _ in $(seq 1 90); do
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

echo "PASS: flock Ollama E2E"
