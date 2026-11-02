//! OSC 7501, the [Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status),
//! spec revision 0.4.
//!
//! This crate re-exports [`termstate_core`] (builder, encoder, parser,
//! scanner) and [`termstate_store`] (the terminal-side record set) under one
//! name. Depend on those directly for `no_std` use.
//!
//! ```
//! use termstate::{Report, State};
//!
//! let report = Report::builder(State::Done).app("brew").msg("Upgraded 12 packages").build()?;
//! assert_eq!(report.to_string(), "\x1b]7501;state=done:app=brew:msg=VXBncmFkZWQgMTIgcGFja2FnZXM=\x1b\\");
//! # Ok::<(), termstate::Error>(())
//! ```
#![no_std]

pub use termstate_core::*;
pub use termstate_store::{Change, Record, Store};
