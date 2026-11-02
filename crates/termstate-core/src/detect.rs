//! Feature detection from the program side, as a pure state machine.
//!
//! The spec's recipe: send the query `OSC 7501 ; ? ST` and right after it
//! primary device attributes (`CSI c`). Every terminal answers DA1, so if
//! its reply arrives without a 7501 reply before it, the protocol is
//! unsupported. A supporting terminal's reply arrives first.
//!
//! [`Detector`] does no I/O: write [`QUERY`] to the terminal (in raw mode),
//! feed whatever comes back to [`Detector::feed`] until it returns an
//! [`Outcome`], or call [`Detector::finish`] when your timeout expires.
//! It keeps reading until the DA1 reply so neither reply leaks into the
//! shell afterwards.

use crate::caps::Capabilities;
use crate::model::Message;
use crate::scan::{Event, Scanner};

/// The bytes to write: the OSC 7501 query followed by `CSI c`.
pub const QUERY: &[u8] = b"\x1b]7501;?\x1b\\\x1b[c";

/// The result of detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The terminal answered the query and accepts these states and kinds.
    Supported(Capabilities),
    /// The terminal answered DA1 without answering the query, or nothing
    /// arrived before the timeout.
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Da1 {
    Idle,
    Esc,
    Csi,
    Params,
}

/// Interprets the terminal's replies. See the [module docs](self).
#[derive(Clone, Debug)]
pub struct Detector {
    scanner: Scanner,
    caps: Option<Capabilities>,
    da1: Da1,
}

impl Default for Detector {
    fn default() -> Self {
        Self::new()
    }
}

impl Detector {
    /// A detector waiting for replies.
    pub const fn new() -> Self {
        Detector {
            scanner: Scanner::new(),
            caps: None,
            da1: Da1::Idle,
        }
    }

    /// Feeds bytes read from the terminal. Returns the outcome once the DA1
    /// reply has arrived; bytes after it are ignored.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<Outcome> {
        let mut caps = self.caps;
        let mut da1 = self.da1;
        let mut done = false;
        self.scanner.feed(bytes, |event| {
            if done {
                return;
            }
            match event {
                Event::Sequence {
                    message: Ok(Message::Feature(c)),
                    ..
                } => caps = Some(c),
                Event::Sequence { .. } => {}
                Event::Passthrough(b) => {
                    for &byte in b {
                        da1 = match (da1, byte) {
                            (_, 0x1b) => Da1::Esc,
                            (Da1::Esc, b'[') => Da1::Csi,
                            (Da1::Csi, b'?') => Da1::Params,
                            (Da1::Params, b'0'..=b'9' | b';') => Da1::Params,
                            (Da1::Params, b'c') => {
                                done = true;
                                break;
                            }
                            _ => Da1::Idle,
                        };
                    }
                }
            }
        });
        self.caps = caps;
        self.da1 = da1;
        done.then(|| self.outcome())
    }

    /// The outcome when no more bytes will be read (for example, on
    /// timeout): supported if the 7501 reply arrived, otherwise unsupported.
    pub fn finish(&self) -> Outcome {
        self.outcome()
    }

    fn outcome(&self) -> Outcome {
        self.caps.map_or(Outcome::Unsupported, Outcome::Supported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, State};

    const REPLY: &[u8] =
        b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\";
    const DA1: &[u8] = b"\x1b[?62;22;52c";

    #[test]
    // spec: FD-DA1
    fn supported_when_reply_precedes_da1() {
        let mut d = Detector::new();
        assert_eq!(d.feed(REPLY), None);
        assert_eq!(
            d.feed(DA1),
            Some(Outcome::Supported(Capabilities::default()))
        );
    }

    #[test]
    // spec: FD-DA1
    fn unsupported_when_da1_arrives_alone() {
        let mut d = Detector::new();
        assert_eq!(d.feed(DA1), Some(Outcome::Unsupported));
    }

    #[test]
    fn handles_replies_split_byte_by_byte() {
        let mut d = Detector::new();
        let mut out = None;
        for chunk in [REPLY, DA1].concat().chunks(1) {
            out = out.or(d.feed(chunk));
        }
        assert_eq!(out, Some(Outcome::Supported(Capabilities::default())));
    }

    #[test]
    fn reduced_reply_and_timeout() {
        let mut d = Detector::new();
        assert_eq!(d.feed(b"\x1b]7501;?:states=idle,done:kinds=auth\x07"), None);
        let Outcome::Supported(c) = d.finish() else {
            panic!()
        };
        assert!(c.supports_state(State::Done) && !c.supports_state(State::Blocked));
        assert!(c.supports_kind(Kind::Auth) && !c.supports_kind(Kind::Question));
        assert_eq!(Detector::new().finish(), Outcome::Unsupported);
    }

    #[test]
    fn other_csi_replies_are_not_da1() {
        let mut d = Detector::new();
        // A cursor position report and DA2 must not end detection.
        assert_eq!(d.feed(b"\x1b[12;40R\x1b[>1;10;0c"), None);
        assert_eq!(d.feed(DA1), Some(Outcome::Unsupported));
    }
}
