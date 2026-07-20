fn main() {
    emit_astrohacker_cli_version();
    println!("cargo:rerun-if-changed=../proto/termsurf.proto");

    prost_build::Config::new()
        .compile_protos(&["../proto/termsurf.proto"], &["../proto/"])
        .unwrap();
}

fn emit_astrohacker_cli_version() {
    println!("cargo:rerun-if-env-changed=ASTROHACKER_VERSION");
    let version = std::env::var("ASTROHACKER_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=ASTROHACKER_CLI_VERSION={version}");
}
