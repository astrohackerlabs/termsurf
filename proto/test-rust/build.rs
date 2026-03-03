fn main() {
    prost_build::compile_protos(&["../hello.proto"], &["../"]).unwrap();
}
