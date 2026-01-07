//! Mach-O delocate functionality for Python wheels.
//!
//! This crate provides functionality to:
//!
//! 1. Parse Mach-O binaries and extract dependency information.
//! 2. Copy external library dependencies into Python wheels.
//! 3. Update install names to use relative paths (`@loader_path`).
//! 4. Validate binary architectures.
//!
//! This library is derived from [`delocate`](https://github.com/matthew-brett/delocate) by Matthew
//! Brett and contributors, which is available under the following BSD-2-Clause license:
//!
//! ```text
//! Copyright (c) 2014-2025, Matthew Brett and the Delocate contributors.
//! All rights reserved.
//!
//! Redistribution and use in source and binary forms, with or without
//! modification, are permitted provided that the following conditions are met:
//!
//! 1. Redistributions of source code must retain the above copyright notice, this
//!    list of conditions and the following disclaimer.
//!
//! 2. Redistributions in binary form must reproduce the above copyright notice,
//!    this list of conditions and the following disclaimer in the documentation
//!    and/or other materials provided with the distribution.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
//! AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
//! IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//! DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
//! FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
//! DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//! SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
//! CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
//! OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
//! OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//! ```

mod delocate;
mod error;
pub mod macho;
pub mod wheel;

pub use delocate::{DelocateOptions, delocate_wheel, list_wheel_dependencies};
pub use error::DelocateError;
pub use macho::{Arch, MacOSVersion, MachOFile};
