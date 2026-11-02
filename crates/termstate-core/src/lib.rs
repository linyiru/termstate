//! Parser, encoder and stream scanner for **OSC 7501**, the
//! [Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status)
//! (spec revision 0.4).
//!
//! `no_std`, no allocation, no dependencies. Enable the `alloc` feature for
//! owned conversions and text sanitizing.
//!
//! - Program side: build a [`Report`] with [`Report::builder`] and write it.
//! - Terminal side: feed bytes to a [`scan::Scanner`], or call [`parse_body`]
//!   on an already-extracted OSC body.
//!
//! ```
//! use termstate_core::{Report, State, Terminator};
//!
//! let report = Report::builder(State::Working)
//!     .app("cargo")
//!     .progress(40)
//!     .msg("Compiling termstate")
//!     .build()
//!     .unwrap();
//! let mut out = String::new();
//! report.write(&mut out, Terminator::St).unwrap();
//! assert_eq!(out, "\x1b]7501;state=working:progress=40:app=cargo:msg=Q29tcGlsaW5nIHRlcm1zdGF0ZQ==\x1b\\");
//! ```
#![no_std]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#[cfg(feature = "alloc")]
extern crate alloc;

mod base64;
mod caps;
mod encode;
mod error;
mod model;
mod parse;
mod text;

pub mod limits;
pub mod scan;

pub use caps::{Capabilities, write_query};
pub use encode::{ReportBuilder, Terminator, write_clear};
pub use error::{Error, Field};
pub use model::{App, Id, Kind, Message, Report, State};
pub use parse::parse_body;
pub use text::Text;
#[cfg(feature = "alloc")]
pub use text::sanitize;
