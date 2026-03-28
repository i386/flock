use std::process::{Command, Stdio};

#[test]
#[ignore = "requires local Ollama, mesh-llm, and goosed binaries"]
fn ollama_e2e() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let script = format!("{manifest_dir}/../scripts/test-ollama-e2e.sh");
    let flock_bin = env!("CARGO_BIN_EXE_flock");

    let status = Command::new("bash")
        .arg(script)
        .env("FLOCK_BIN", flock_bin)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("failed to run Ollama E2E script");

    assert!(status.success(), "Ollama E2E script failed");
}
