pub mod criterion {
    //! This module re-exports the criterion API but picks the right backend depending on whether
    //! the benchmarks are built to run locally or with codspeed

    #[cfg(not(feature = "codspeed"))]
    pub use criterion::*;

    #[cfg(feature = "codspeed")]
    pub use codspeed_criterion_compat::*;
}
