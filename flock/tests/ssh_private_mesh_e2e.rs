use std::process::{Command, Stdio};

#[test]
#[ignore = "requires FLOCK_SSH_HOST plus local and remote mesh/goose binaries"]
fn ssh_private_mesh_e2e() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let script = format!("{manifest_dir}/../scripts/test-ssh-private-mesh-e2e.sh");
    let flock_bin = env!("CARGO_BIN_EXE_flock");

    let status = Command::new("bash")
        .arg(script)
        .env("FLOCK_BIN", flock_bin)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("failed to run SSH private-mesh E2E script");

    assert!(status.success(), "SSH private-mesh E2E script failed");
}
