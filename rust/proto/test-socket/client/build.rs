fn main() {
    prost_build::compile_protos(&["../../termsurf.proto"], &["../../"]).unwrap();
}
