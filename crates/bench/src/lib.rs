pub mod criterion {
    //! This module re-exports the criterion API unconditionally for now. It's
    //! intended that in the future this be a way to switch the backend to
    //! something else (like codspeed).

    pub use criterion::*;
}
