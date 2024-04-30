pub mod criterion {
    //! This module re-exports the criterion API but picks the right backend depending on whether
    //! the benchmarks are built to run locally or with codspeed

    #[cfg(not(codspeed))]
    pub use criterion::*;

    #[cfg(codspeed)]
    pub use codspeed_criterion_compat::*;
}
