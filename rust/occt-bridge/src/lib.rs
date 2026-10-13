//! Safe, dependency-free Rust wrapper around `opencascade-c-bridge`.

use std::{
    cell::Cell,
    ffi::{CStr, CString, c_char, c_int, c_void},
    marker::PhantomData,
    path::Path,
    ptr::{self, NonNull},
};

mod construction;
mod exchange;
mod ffi;
mod inspection;
mod mesh;
mod operations;
mod projection;
mod refinement;
mod session;
mod types;

use ffi::*;
pub use refinement::*;
pub use session::*;
pub use types::*;

#[cfg(test)]
mod tests;
