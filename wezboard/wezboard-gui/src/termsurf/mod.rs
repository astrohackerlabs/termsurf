pub mod conn;
pub mod listener;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

pub use listener::spawn_termsurf_server;
