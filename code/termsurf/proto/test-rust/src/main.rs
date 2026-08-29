use prost::Message;

pub mod termsurf {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

fn main() {
    // Create a TermSurfMessage wrapping a CreateTab.
    let original = termsurf::TermSurfMessage {
        msg: Some(termsurf::term_surf_message::Msg::CreateTab(
            termsurf::CreateTab {
                url: "https://termsurf.com".to_string(),
                pane_id: "pane-1".to_string(),
                pixel_width: 1920,
                pixel_height: 1080,
                dark: true,
            },
        )),
    };

    // Serialize.
    let mut buf = Vec::new();
    original.encode(&mut buf).unwrap();

    // Deserialize.
    let decoded = termsurf::TermSurfMessage::decode(buf.as_slice()).unwrap();

    // Verify the oneof round-trips correctly.
    match decoded.msg {
        Some(termsurf::term_surf_message::Msg::CreateTab(tab)) => {
            assert_eq!(tab.url, "https://termsurf.com");
            assert_eq!(tab.pane_id, "pane-1");
            assert_eq!(tab.pixel_width, 1920);
            assert_eq!(tab.pixel_height, 1080);
            assert!(tab.dark);
        }
        other => panic!("Expected CreateTab, got {:?}", other),
    }

    println!("Rust: pass ({} bytes)", buf.len());
}
