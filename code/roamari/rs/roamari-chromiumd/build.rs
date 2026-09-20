use std::env;
use std::path::PathBuf;

fn main() {
    emit_cli_version();
    println!("cargo:rerun-if-changed=../../../termsurf/proto/termsurf.proto");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // Crate lives at code/roamari/rs/roamari-chromiumd; monorepo root is four parents up.
    let repo_root = manifest_dir
        .parent()
        .and_then(|rs_dir| rs_dir.parent())
        .and_then(|roamari_dir| roamari_dir.parent())
        .and_then(|code_dir| code_dir.parent())
        .expect("roamari-chromiumd must live under code/roamari/rs/ in the monorepo");

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
        .compile_protos(
            &["../../../termsurf/proto/termsurf.proto"],
            &["../../../termsurf/proto/"],
        )
        .unwrap();
}

fn emit_cli_version() {
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    println!("cargo:rustc-env=ASTROHACKER_CLI_VERSION={version}");
}
