//! Easy Fortran 77 — core logic.
//!
//! Deliberately free of GUI dependencies so the whole pipeline can be tested
//! headlessly, with no window server and no compiler installed.
//!
//! The one invariant that outranks everything else here: **the user's source files
//! are only ever read.** `fs_guard` is the sole module permitted to touch the
//! filesystem, and it is what makes that structural rather than aspirational.

pub mod build;
pub mod config;
pub mod error;
pub mod fs_guard;
pub mod help;
pub mod i18n;
pub mod paths;
pub mod project;
pub mod text;
pub mod toolchain;
pub mod update;

pub use error::{EfError, Result};
