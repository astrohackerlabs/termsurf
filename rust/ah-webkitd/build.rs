use std::env;
use std::path::PathBuf;

fn main() {
    emit_astrohacker_cli_version();
    emit_installed_webkit_framework_path();
    println!("cargo:rerun-if-changed=../proto/termsurf.proto");

    // WebKit C ABI build output directory (relative to this crate).
    let webkit_abi_out = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("libtermsurf_webkit/build")
        .canonicalize()
        .expect("webkit/libtermsurf_webkit/build must exist — build libtermsurf_webkit first");

    // Link-time: find libtermsurf_webkit.dylib.
    println!(
        "cargo:rustc-link-search=native={}",
        webkit_abi_out.display()
    );
    println!("cargo:rustc-link-lib=dylib=termsurf_webkit");

    // Runtime: two rpaths.
    // 1. @loader_path/. — for release (dylib colocated with binary).
    // 2. WebKit C ABI build dir — for development (binary in target/, dylib in
    //    webkit/libtermsurf_webkit/build/).
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/.");
    println!(
        "cargo:rustc-link-arg=-Wl,-rpath,{}",
        webkit_abi_out.display()
    );

    // Compile protobuf (same pattern as TUI).
    prost_build::Config::new()
        .compile_protos(&["../proto/termsurf.proto"], &["../proto/"])
        .unwrap();
}

fn emit_installed_webkit_framework_path() {
    if env::var("PROFILE").as_deref() == Ok("release") {
        println!(
            "cargo:rustc-link-arg=-Wl,-dyld_env,DYLD_FRAMEWORK_PATH=/opt/homebrew/opt/astrohacker-terminal-ah-webkitd"
        );
    }
}

fn emit_astrohacker_cli_version() {
    println!("cargo:rerun-if-env-changed=ASTROHACKER_VERSION");
    let version =
        env::var("ASTROHACKER_VERSION").unwrap_or_else(|_| env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=ASTROHACKER_CLI_VERSION={version}");
}
