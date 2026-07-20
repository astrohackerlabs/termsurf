pub mod termsurf {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

pub use termsurf::term_surf_message::Msg;
pub use termsurf::TermSurfMessage;
