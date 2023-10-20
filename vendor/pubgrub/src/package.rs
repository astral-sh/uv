// SPDX-License-Identifier: MPL-2.0

//! Trait for identifying packages.
//! Automatically implemented for traits implementing
//! [Clone] + [Eq] + [Hash] + [Debug] + [Display].

use std::fmt::{Debug, Display};
use std::hash::Hash;

/// Trait for identifying packages.
/// Automatically implemented for types already implementing
/// [Clone] + [Eq] + [Hash] + [Debug] + [Display].
pub trait Package: Clone + Eq + Hash + Debug + Display {}

/// Automatically implement the Package trait for any type
/// that already implement [Clone] + [Eq] + [Hash] + [Debug] + [Display].
impl<T: Clone + Eq + Hash + Debug + Display> Package for T {}
