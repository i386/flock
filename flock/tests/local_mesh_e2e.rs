use std::process::{Command, Stdio};

#[test]
#[ignore = "requires local mesh-llm, goosed, llama.cpp binaries, and a small GGUF model"]
fn local_mesh_e2e() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let script = format!("{manifest_dir}/../scripts/test-local-mesh-e2e.sh");
    let flock_bin = env!("CARGO_BIN_EXE_flock");

    let status = Command::new("bash")
        .arg(script)
        .env("FLOCK_BIN", flock_bin)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("failed to run local mesh E2E script");

    assert!(status.success(), "local mesh E2E script failed");
}
