//! Parser, encoder and stream scanner for **OSC 7501**, the
//! [Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status)
//! (spec revision 0.4).
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[allow(dead_code)]
mod base64;
mod error;

pub mod limits;

pub use error::{Error, Field};
