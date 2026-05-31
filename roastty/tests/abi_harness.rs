use std::env;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn c_harness_links_against_roastty_header_and_roastty_dylib() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .expect("roastty crate should live under repo root");
    let target_dir = repo_root.join("target").join("debug");
    let dylib = target_dir.join("libroastty.dylib");

    let status = Command::new("cargo")
        .args(["build", "-p", "roastty"])
        .current_dir(repo_root)
        .status()
        .expect("failed to run cargo build for roastty");
    assert!(status.success(), "cargo build -p roastty failed");
    assert!(
        dylib.exists(),
        "cargo build did not produce libroastty.dylib"
    );

    let out_dir = target_dir.join("roastty-abi-harness");
    std::fs::create_dir_all(&out_dir).expect("failed to create ABI harness output dir");
    let binary = out_dir.join("abi_harness");
    let source = manifest_dir.join("tests").join("abi_harness.c");
    let include_dir = manifest_dir.join("include");

    let status = Command::new("clang")
        .arg("-DROASTTY_STATIC")
        .arg("-I")
        .arg(&include_dir)
        .arg(&source)
        .arg(&dylib)
        .arg("-Wl,-rpath")
        .arg("-Wl,@executable_path/..")
        .arg("-o")
        .arg(&binary)
        .status()
        .expect("failed to compile ABI harness");
    assert!(status.success(), "ABI harness compile/link failed");

    let status = Command::new(&binary)
        .status()
        .expect("failed to run ABI harness");
    assert!(status.success(), "ABI harness failed");
}
