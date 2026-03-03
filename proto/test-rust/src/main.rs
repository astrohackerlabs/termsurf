use prost::Message;

pub mod termsurf {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

fn main() {
    let original = termsurf::Hello {
        name: "TermSurf".to_string(),
        id: -42,
        size: 1024,
        x: 3.14,
        active: true,
    };

    // Serialize.
    let mut buf = Vec::new();
    original.encode(&mut buf).unwrap();

    // Deserialize.
    let decoded = termsurf::Hello::decode(buf.as_slice()).unwrap();

    // Verify.
    assert_eq!(decoded.name, "TermSurf");
    assert_eq!(decoded.id, -42);
    assert_eq!(decoded.size, 1024);
    assert!((decoded.x - 3.14).abs() < f64::EPSILON);
    assert!(decoded.active);

    println!("Rust: pass ({} bytes)", buf.len());
}
