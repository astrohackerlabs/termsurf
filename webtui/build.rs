fn main() {
    println!("cargo:rerun-if-changed=../proto/termsurf.proto");

    prost_build::Config::new()
        .compile_protos(&["../proto/termsurf.proto"], &["../proto/"])
        .unwrap();
}
