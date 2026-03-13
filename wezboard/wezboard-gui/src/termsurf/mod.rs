pub mod conn;
pub mod input;
pub mod listener;
pub mod metrics;
pub mod state;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

pub use conn::set_overlay_frame;
pub use listener::spawn_termsurf_server;
pub use state::global as shared_state;
