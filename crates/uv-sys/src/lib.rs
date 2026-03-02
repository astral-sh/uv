//! Platform-agnostic system interfaces for uv.

mod ctrl_c;

pub use ctrl_c::{CtrlCError, on_ctrl_c};
