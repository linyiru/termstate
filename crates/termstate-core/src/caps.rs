use core::fmt;

use crate::encode::Terminator;
use crate::model::{Kind, State};

/// What a terminal accepts, from its feature-detection reply.
///
/// [`Default`] is the spec's fixed default: every revision-0.4 state and kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Capabilities {
    states: u8,
    kinds: u8,
}

const fn state_bit(s: State) -> u8 {
    1 << s as u8
}

const fn kind_bit(k: Kind) -> u8 {
    1 << k as u8
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities { states: 0b1_1111, kinds: 0b111 }
    }
}

impl Capabilities {
    /// No states and no kinds.
    pub const fn empty() -> Self {
        Capabilities { states: 0, kinds: 0 }
    }

    /// Whether `state` is accepted.
    pub const fn supports_state(&self, state: State) -> bool {
        self.states & state_bit(state) != 0
    }

    /// Whether `kind` is accepted.
    pub const fn supports_kind(&self, kind: Kind) -> bool {
        self.kinds & kind_bit(kind) != 0
    }

    /// Adds or removes `state`.
    pub fn set_state(&mut self, state: State, on: bool) {
        if on { self.states |= state_bit(state) } else { self.states &= !state_bit(state) }
    }

    /// Adds or removes `kind`.
    pub fn set_kind(&mut self, kind: Kind, on: bool) {
        if on { self.kinds |= kind_bit(kind) } else { self.kinds &= !kind_bit(kind) }
    }

    /// Accepted states, in spec order.
    pub fn states(&self) -> impl Iterator<Item = State> + '_ {
        State::ALL.into_iter().filter(|&s| self.supports_state(s))
    }

    /// Accepted kinds, in spec order.
    pub fn kinds(&self) -> impl Iterator<Item = Kind> + '_ {
        Kind::ALL.into_iter().filter(|&k| self.supports_kind(k))
    }

    /// Applies one `key=value` pair of a reply. Unknown keys and values are ignored.
    pub(crate) fn apply_pair(&mut self, key: &str, value: &str) {
        let items = value.split(',').map(str::trim).filter(|s| !s.is_empty());
        match key {
            "states" => {
                self.states = 0;
                items.filter_map(State::from_wire).for_each(|s| self.set_state(s, true));
            }
            "kinds" => {
                self.kinds = 0;
                items.filter_map(Kind::from_wire).for_each(|k| self.set_kind(k, true));
            }
            _ => {}
        }
    }

    /// Writes the terminal's reply, listing states and kinds explicitly.
    pub fn write_reply<W: fmt::Write + ?Sized>(&self, w: &mut W, term: Terminator) -> fmt::Result {
        w.write_str("\x1b]7501;?:states=")?;
        write_list(w, self.states().map(State::as_str))?;
        w.write_str(":kinds=")?;
        write_list(w, self.kinds().map(Kind::as_str))?;
        w.write_str(term.as_str())
    }
}

fn write_list<'a, W: fmt::Write + ?Sized>(
    w: &mut W,
    items: impl Iterator<Item = &'a str>,
) -> fmt::Result {
    for (i, item) in items.enumerate() {
        if i > 0 {
            w.write_char(',')?;
        }
        w.write_str(item)?;
    }
    Ok(())
}

/// Writes the program's feature-detection query, `OSC 7501 ; ? ST`.
///
/// To avoid waiting on a terminal that never answers, follow it with
/// primary device attributes (`CSI c`, `"\x1b[c"`): if that reply arrives
/// first, the protocol is unsupported.
pub fn write_query<W: fmt::Write + ?Sized>(w: &mut W, term: Terminator) -> fmt::Result {
    w.write_str("\x1b]7501;?")?;
    w.write_str(term.as_str())
}
