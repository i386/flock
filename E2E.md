# E2E Testing

`flock` has two opt-in end-to-end tests:

- `local_mesh_e2e`
- `ssh_private_mesh_e2e`

They are both ignored by default and should be run explicitly.

## Commands

Local mesh E2E:

```bash
cargo test local_mesh_e2e -- --ignored --nocapture
```

SSH private-mesh E2E:

```bash
FLOCK_SSH_HOST=<host> cargo test ssh_private_mesh_e2e -- --ignored --nocapture
```

Example:

```bash
FLOCK_SSH_HOST=build.local cargo test ssh_private_mesh_e2e -- --ignored --nocapture
```

## What They Cover

`local_mesh_e2e`:

- starts a local `mesh-llm` node with a small model
- runs `flock` as the local Goose-facing backend
- starts `goosed` under an isolated Goose root
- creates a session through `flock`
- sends a real reply request
- verifies session state and SSE events

`ssh_private_mesh_e2e`:

- stages `mesh-llm`, `flock`, `goosed`, `rpc-server`, and `llama-server` to a remote host
- starts a remote private mesh node with a small model
- starts a local `mesh-llm --client` plus `flock`
- creates a session through the local `flock` endpoint
- verifies remote routing, pinned session state, reply flow, and SSE events

## Prerequisites

Local mesh E2E:

- local `mesh-llm` binary at `../mesh-llm/target/debug/mesh-llm`
- local `goosed` binary at `../goose/target/debug/goosed`
- local llama.cpp binaries at `../mesh-llm/llama.cpp/build-flock/bin`
- local model `~/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf`
- the current user can write `/tmp/mesh-llm-llama-server.log`

SSH private-mesh E2E:

- `FLOCK_SSH_HOST` points at a second machine reachable with SSH keys
- the remote machine can run the staged local binaries
- the remote machine already has `~/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf`
- local llama.cpp binaries exist at `../mesh-llm/llama.cpp/build-flock/bin`
- the remote machine can write `/tmp/mesh-llm-llama-server.log`

## Skip Behavior

The E2E scripts are designed to skip cleanly when prerequisites are missing.

Common skip reasons:

- another local `mesh-llm`, `rpc-server`, or `llama-server` is already running
- the required GGUF model is missing
- the SSH host is unreachable
- the remote host cannot write `/tmp/mesh-llm-llama-server.log`

`SKIP:` is not a test failure. It means the environment is not currently suitable for that E2E run.

## Common Failure Cases

`rpc-server failed to start`:

- another local inference stack is already using the GPU/runtime
- rerun after stopping the other stack, or let the test skip instead

`Failed to create llama-server log file`:

- upstream `mesh-llm` currently hardcodes `/tmp/mesh-llm-llama-server.log`
- the current user on that machine must be able to create or append to that file

`agent/start` or `/reply` works but the first reply fails unexpectedly:

- on the current Goose build, some flows still benefit from an explicit `agent/update_provider`
- see the note about [block/goose#8164](https://github.com/block/goose/pull/8164) in the README

## Notes

- Both E2E tests use a small `mesh-llm` model path now. They no longer rely on Ollama.
- Both tests use isolated Goose roots so they do not reuse your normal Goose config.
- The SSH E2E is the closest reusable test to the real `flock` product path.
