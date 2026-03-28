# Flock Agent Notes

Run the opt-in local mesh E2E smoke when changing `flock` transport, session routing, local `goosed` supervision, or Goose compatibility routes.

Command:

- `cargo test local_mesh_e2e -- --ignored --nocapture`
- `FLOCK_SSH_HOST=<ssh-host> cargo test ssh_private_mesh_e2e -- --ignored --nocapture`

Local mesh E2E prerequisites:

- local `mesh-llm` binary available at `../mesh-llm/target/debug/mesh-llm`
- local `goosed` binary available at `../goose/target/debug/goosed`
- local llama.cpp binaries exist at `../mesh-llm/llama.cpp/build-flock/bin`
- local model `~/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf` is present
- the current user can write `/tmp/mesh-llm-llama-server.log`

SSH E2E prerequisites:

- `FLOCK_SSH_HOST` points at a second machine reachable with batch-mode SSH
- the remote machine can execute binaries built on the local machine
- the remote machine already has `~/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf`
- local llama.cpp binaries exist at `../mesh-llm/llama.cpp/build-flock/bin`
- the remote machine can write `/tmp/mesh-llm-llama-server.log` for the current user

If prerequisites are missing, the E2E harness prints `SKIP:` and exits successfully. Report that the smoke was skipped instead of treating it as a test failure.
