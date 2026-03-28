# Flock Agent Notes

Run the opt-in Ollama E2E smoke when changing `flock` transport, session routing, local `goosed` supervision, or Goose compatibility routes.

Command:

- `cargo test ollama_e2e -- --ignored --nocapture`

Prerequisites:

- local Ollama reachable on `127.0.0.1:11434`
- model `llama3.1` already pulled
- local `mesh-llm` binary available at `../mesh-llm/target/debug/mesh-llm`
- local `goosed` binary available at `../goose/target/debug/goosed`

If prerequisites are missing, the E2E harness prints `SKIP:` and exits successfully. Report that the smoke was skipped instead of treating it as a test failure.
