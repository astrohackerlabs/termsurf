use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../proto/termsurf.proto");

    // Chromium build output directory (relative to repo root).
    let chromium_out = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../chromium/src/out/Default")
        .canonicalize()
        .expect("chromium/src/out/Default must exist — build Chromium first");

    // Link-time: find libtermsurf_chromium.dylib.
    println!("cargo:rustc-link-search=native={}", chromium_out.display());
    println!("cargo:rustc-link-lib=dylib=termsurf_chromium");

    // Runtime: two rpaths.
    // 1. @loader_path/. — for release (dylib colocated with binary).
    // 2. Chromium build dir — for development (binary in target/, dylib in
    //    chromium/src/out/Default/).
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/.");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", chromium_out.display());

    // Compile protobuf (same pattern as TUI).
    prost_build::Config::new()
        .compile_protos(&["../proto/termsurf.proto"], &["../proto/"])
        .unwrap();
}
