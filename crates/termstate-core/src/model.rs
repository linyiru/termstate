use core::fmt;

use crate::caps::Capabilities;
use crate::error::Error;
use crate::limits::{MAX_APP, MAX_ID, MAX_ID_DEPTH, MAX_ID_SEGMENT};
use crate::text::Text;

/// A record state. `clear` is not a state; it is [`Message::Clear`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum State {
    /// At rest, waiting for the user's next instruction.
    Idle,
    /// Running. May carry `progress`.
    Working,
    /// Finished; the user has not seen the result yet.
    Done,
    /// Cannot continue until the user acts. `kind` says what, `msg` says why.
    Blocked,
    /// Failed and stopped.
    Error,
}

impl State {
    /// Every state in revision 0.4, in spec order.
    pub const ALL: [State; 5] = [State::Idle, State::Working, State::Done, State::Blocked, State::Error];

    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Working => "working",
            State::Done => "done",
            State::Blocked => "blocked",
            State::Error => "error",
        }
    }

    /// Parses a wire value. `clear` is not a state and returns `None`.
    pub fn from_wire(s: &str) -> Option<Self> {
        State::ALL.into_iter().find(|st| st.as_str() == s)
    }

    /// `working` and `blocked`: dropped when the process exits or a new shell
    /// prompt begins.
    pub const fn is_transient(self) -> bool {
        matches!(self, State::Working | State::Blocked)
    }

    /// Whether `progress` is meaningful in this state.
    pub const fn allows_progress(self) -> bool {
        matches!(self, State::Working | State::Blocked)
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a `blocked` program needs from the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// Approval to do something.
    Permission,
    /// The user must type an answer.
    Question,
    /// A login, token, or credential.
    Auth,
}

impl Kind {
    /// Every kind in revision 0.4, in spec order.
    pub const ALL: [Kind; 3] = [Kind::Permission, Kind::Question, Kind::Auth];

    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Permission => "permission",
            Kind::Question => "question",
            Kind::Auth => "auth",
        }
    }

    /// Parses a wire value.
    pub fn from_wire(s: &str) -> Option<Self> {
        Kind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-')
}

/// A validated record id: `segment ("/" segment)*`, segment
/// `[A-Za-z0-9_.+-]{1,32}`, at most 128 bytes and 8 levels.
/// The root record has no id; it is represented as `Option<Id>::None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id<'a>(&'a str);

impl<'a> Id<'a> {
    /// Validates `s` against the id grammar and limits.
    pub fn new(s: &'a str) -> Result<Self, Error> {
        if s.is_empty() || s.len() > MAX_ID {
            return Err(Error::InvalidId);
        }
        let mut depth = 0;
        for seg in s.split('/') {
            depth += 1;
            if seg.is_empty()
                || seg.len() > MAX_ID_SEGMENT
                || depth > MAX_ID_DEPTH
                || !seg.bytes().all(is_name_byte)
            {
                return Err(Error::InvalidId);
            }
        }
        Ok(Id(s))
    }

    /// The id as a string.
    pub const fn as_str(&self) -> &'a str {
        self.0
    }

    /// Path segments, outermost first.
    pub fn segments(&self) -> impl DoubleEndedIterator<Item = &'a str> + 'a {
        self.0.split('/')
    }

    /// The parent id, or `None` when the parent is the root record.
    pub fn parent(&self) -> Option<Id<'a>> {
        self.0.rfind('/').map(|i| Id(&self.0[..i]))
    }

    /// Whether `self` is `other` or one of its ancestors.
    pub fn contains(&self, other: &Id<'_>) -> bool {
        other.0 == self.0
            || (other.0.len() > self.0.len()
                && other.0.starts_with(self.0)
                && other.0.as_bytes()[self.0.len()] == b'/')
    }
}

impl fmt::Display for Id<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A validated program identifier: `[A-Za-z0-9_.+-]{1,32}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct App<'a>(&'a str);

impl<'a> App<'a> {
    /// Validates `s`.
    pub fn new(s: &'a str) -> Result<Self, Error> {
        if s.is_empty() || s.len() > MAX_APP || !s.bytes().all(is_name_byte) {
            return Err(Error::InvalidApp);
        }
        Ok(App(s))
    }

    /// The app as a string.
    pub const fn as_str(&self) -> &'a str {
        self.0
    }
}

impl fmt::Display for App<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// One validated report: exactly what the addressed record holds after it is
/// applied.
///
/// Normalized: `kind` is only present with `blocked`, `progress` only with
/// `working` or `blocked`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report<'a> {
    /// The state.
    pub state: State,
    /// The record id; `None` is the root record.
    pub id: Option<Id<'a>>,
    /// What a blocked program needs.
    pub kind: Option<Kind>,
    /// 0..=100; `None` means indeterminate.
    pub progress: Option<u8>,
    /// Program identifier. When absent, the record inherits it from its
    /// nearest ancestor (see `termstate-store`).
    pub app: Option<App<'a>>,
    /// Short label.
    pub title: Option<Text<'a>>,
    /// One line of status text.
    pub msg: Option<Text<'a>>,
}

/// One OSC 7501 sequence, decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Message<'a> {
    /// A report that sets a record.
    Report(Report<'a>),
    /// `state=clear`: removes the addressed record and every record beneath
    /// it; with no id, removes every record.
    Clear {
        /// The record to clear; `None` clears everything.
        id: Option<Id<'a>>,
    },
    /// A `?` body. Sent by a program it is the feature-detection query, and a
    /// terminal answers it. Sent by a terminal it is the reply, and
    /// `Capabilities` says what it accepts (the spec defaults when it lists
    /// nothing).
    Feature(Capabilities),
}
