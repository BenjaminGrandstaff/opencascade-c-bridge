//! Safe, dependency-free Rust wrapper around `opencascade-c-bridge`.

use std::{
    cell::Cell,
    error::Error,
    ffi::{CStr, CString, c_char, c_int, c_void},
    fmt,
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
mod session;
mod types;

use ffi::*;
pub use projection::{AnalyticCurve, BezierSpan};
pub use session::*;
pub use types::*;

#[cfg(test)]
mod tests;
