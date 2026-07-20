//! Integration tests for the shipped `ahsh` binary (real entry path).

use std::process::Command;

fn ahsh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ahsh"))
}

#[test]
fn exit_command_propagates_code_7() {
    let output = ahsh()
        .args(["-c", "exit 7"])
        .output()
        .expect("spawn ahsh");
    assert_eq!(
        output.status.code(),
        Some(7),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Exit doesn't catch"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn exit_command_propagates_code_0() {
    let status = ahsh()
        .args(["-c", "exit 0"])
        .status()
        .expect("spawn ahsh");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn exit_command_propagates_code_42() {
    let status = ahsh()
        .args(["-c", "exit 42"])
        .status()
        .expect("spawn ahsh");
    assert_eq!(status.code(), Some(42));
}
