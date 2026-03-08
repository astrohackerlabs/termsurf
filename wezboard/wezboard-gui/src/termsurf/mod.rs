pub mod conn;
pub mod listener;
pub mod metrics;
pub mod state;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

pub use listener::spawn_termsurf_server;
pub use state::SharedState;
