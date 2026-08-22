fn main() {
    println!("cargo:rerun-if-env-changed=ASTROHACKER_VERSION");
    let version = std::env::var("ASTROHACKER_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=ASTROHACKER_CLI_VERSION={version}");
}
