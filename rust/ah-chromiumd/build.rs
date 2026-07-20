use std::env;
use std::path::PathBuf;

fn main() {
    emit_astrohacker_cli_version();
    println!("cargo:rerun-if-changed=../proto/termsurf.proto");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // Crate lives at rust/ah-chromiumd; monorepo root is two parents up.
    let repo_root = manifest_dir
        .parent()
        .and_then(|rust_dir| rust_dir.parent())
        .expect("ah-chromiumd must live under rust/ in the monorepo");

    // Chromium build output directory in the ignored top-level fork checkout.
    let chromium_out = env::var_os("TERMSURF_CHROMIUM_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("forks/chromium/src/out/Default"))
        .canonicalize()
        .expect("forks/chromium/src/out/Default must exist — build Chromium first");

    // Link-time: find libtermsurf_chromium.dylib.
    println!("cargo:rustc-link-search=native={}", chromium_out.display());
    println!("cargo:rustc-link-lib=dylib=termsurf_chromium");

    // Runtime: two rpaths.
    // 1. @loader_path/. — for release (dylib colocated with binary).
    // 2. Chromium build dir — for development (binary in target/, dylib in
    //    forks/chromium/src/out/Default/.
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/.");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", chromium_out.display());

    // Compile protobuf (same pattern as TUI).
    prost_build::Config::new()
        .compile_protos(&["../proto/termsurf.proto"], &["../proto/"])
        .unwrap();
}

fn emit_astrohacker_cli_version() {
    println!("cargo:rerun-if-env-changed=ASTROHACKER_VERSION");
    let version =
        env::var("ASTROHACKER_VERSION").unwrap_or_else(|_| env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=ASTROHACKER_CLI_VERSION={version}");
}
