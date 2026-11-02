//! Extracting OSC 7501 sequences from a byte stream, such as the output a
//! multiplexer or PTY wrapper reads from a program.
//!
//! The [`Scanner`] is a small state machine over 7-bit escape sequences. It
//! needs no allocation, keeps at most one sequence body (≤ 4088 bytes), and
//! handles sequences split across any number of reads. Everything that is
//! not an OSC 7501 sequence is handed back unchanged as
//! [`Event::Passthrough`], so it also works as a filter that strips reports.
//!
//! Within a sequence it follows the DEC/xterm parser: C0 controls other than
//! the terminators are ignored, `CAN` and `SUB` abort the sequence, and an
//! `ESC` not followed by `\` aborts it and starts a new escape sequence.
//! 8-bit C1 controls (`0x9D` OSC, `0x9C` ST) are not recognized, matching
//! terminals that run in UTF-8 mode.

use crate::encode::Terminator;
use crate::error::Error;
use crate::limits::{MAX_BODY, MAX_SEQUENCE, PREFIX};
use crate::model::Message;
use crate::parse::parse_body;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// One thing the scanner found.
#[derive(Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// Bytes that are not part of an OSC 7501 sequence, in input order.
    Passthrough(&'a [u8]),
    /// A complete OSC 7501 sequence.
    Sequence {
        /// The decoded message, or why a terminal must discard it.
        message: Result<Message<'a>, Error>,
        /// How the sequence was terminated.
        terminator: Terminator,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum St {
    Ground,
    /// Matched this many bytes of [`PREFIX`] (1..PREFIX.len()).
    Prefix(usize),
    Body,
    BodyEsc,
}

/// Streaming OSC 7501 extractor. See the [module docs](self).
#[derive(Clone)]
pub struct Scanner {
    body: [u8; MAX_BODY],
    len: usize,
    overflow: bool,
    state: St,
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for Scanner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Scanner")
            .field("state", &self.state)
            .field("len", &self.len)
            .finish()
    }
}

impl Scanner {
    /// A scanner in the ground state.
    pub const fn new() -> Self {
        Scanner {
            body: [0; MAX_BODY],
            len: 0,
            overflow: false,
            state: St::Ground,
        }
    }

    /// Whether the scanner is between sequences, holding nothing back.
    pub fn is_ground(&self) -> bool {
        self.state == St::Ground
    }

    /// Scans `input`, calling `on` for every event in stream order.
    ///
    /// Bytes that might start an OSC 7501 sequence are held back until the
    /// next call decides; call [`finish`](Self::finish) at end of stream.
    pub fn feed(&mut self, input: &[u8], mut on: impl FnMut(Event<'_>)) {
        let mut ground_from = (self.state == St::Ground).then_some(0);
        let mut i = 0;
        while i < input.len() {
            let b = input[i];
            match self.state {
                St::Ground => {
                    if b == ESC {
                        if let Some(from) = ground_from.take().filter(|&from| from < i) {
                            on(Event::Passthrough(&input[from..i]));
                        }
                        self.state = St::Prefix(1);
                    }
                }
                St::Prefix(k) => {
                    if b == PREFIX[k] {
                        if k + 1 == PREFIX.len() {
                            self.state = St::Body;
                            self.len = 0;
                            self.overflow = false;
                        } else {
                            self.state = St::Prefix(k + 1);
                        }
                    } else {
                        // Some other escape sequence: give back what we held
                        // and look at this byte again from the ground state.
                        on(Event::Passthrough(&PREFIX[..k]));
                        self.state = St::Ground;
                        ground_from = Some(i);
                        continue;
                    }
                }
                St::Body => match b {
                    BEL => {
                        self.complete(Terminator::Bel, &mut on);
                        ground_from = Some(i + 1);
                    }
                    ESC => self.state = St::BodyEsc,
                    CAN | SUB => {
                        self.state = St::Ground;
                        ground_from = Some(i + 1);
                    }
                    0x00..=0x1f => {}
                    _ => {
                        if self.len < self.body.len() {
                            self.body[self.len] = b;
                            self.len += 1;
                        } else {
                            self.overflow = true;
                        }
                    }
                },
                St::BodyEsc => {
                    if b == b'\\' {
                        self.complete(Terminator::St, &mut on);
                        ground_from = Some(i + 1);
                    } else {
                        // The sequence is aborted; the ESC begins a new one.
                        self.state = St::Prefix(1);
                        continue;
                    }
                }
            }
            i += 1;
        }
        if let Some(from) = ground_from.filter(|&from| from < input.len()) {
            if self.state == St::Ground {
                on(Event::Passthrough(&input[from..]));
            }
        }
    }

    /// Ends the stream: bytes held back as a possible prefix are passed
    /// through, and an unterminated sequence is dropped.
    pub fn finish(&mut self, mut on: impl FnMut(Event<'_>)) {
        if let St::Prefix(k) = self.state {
            on(Event::Passthrough(&PREFIX[..k]));
        }
        self.state = St::Ground;
        self.len = 0;
        self.overflow = false;
    }

    fn complete(&mut self, terminator: Terminator, on: &mut impl FnMut(Event<'_>)) {
        let total = PREFIX.len() + self.len + terminator.as_str().len();
        let message = if self.overflow || total > MAX_SEQUENCE {
            Err(Error::SequenceTooLong)
        } else {
            parse_body(&self.body[..self.len])
        };
        on(Event::Sequence {
            message,
            terminator,
        });
        self.state = St::Ground;
        self.len = 0;
        self.overflow = false;
    }
}
