use core::fmt;

use crate::error::{Error, Field};
use crate::limits::{MAX_MSG_DECODED, MAX_SEQUENCE, MAX_TITLE_DECODED, PREFIX};
use crate::model::{App, Id, Kind, Report, State};
use crate::text::Text;

/// The string terminator to end a sequence with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Terminator {
    /// `ESC \` (the default; what the spec examples use).
    #[default]
    St,
    /// `BEL` (0x07). Shorter, and safer inside tmux passthrough.
    Bel,
}

impl Terminator {
    /// The terminator bytes.
    pub const fn as_str(self) -> &'static str {
        match self {
            Terminator::St => "\x1b\\",
            Terminator::Bel => "\x07",
        }
    }
}

/// Builds a [`Report`] from unvalidated program input. See [`Report::builder`].
#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct ReportBuilder<'a> {
    state: State,
    id: Option<&'a str>,
    kind: Option<Kind>,
    progress: Option<u8>,
    app: Option<&'a str>,
    title: Option<&'a str>,
    msg: Option<&'a str>,
}

impl<'a> Report<'a> {
    /// Starts a report in `state`.
    pub fn builder(state: State) -> ReportBuilder<'a> {
        ReportBuilder { state, id: None, kind: None, progress: None, app: None, title: None, msg: None }
    }

    /// Writes the full sequence, `OSC 7501 ; pairs ST`, in canonical key
    /// order: `state`, `id`, `kind`, `progress`, `app`, `title`, `msg`.
    pub fn write<W: fmt::Write + ?Sized>(&self, w: &mut W, term: Terminator) -> fmt::Result {
        w.write_str("\x1b]7501;state=")?;
        w.write_str(self.state.as_str())?;
        if let Some(id) = self.id {
            w.write_str(":id=")?;
            w.write_str(id.as_str())?;
        }
        if let Some(kind) = self.kind {
            w.write_str(":kind=")?;
            w.write_str(kind.as_str())?;
        }
        if let Some(p) = self.progress {
            write!(w, ":progress={p}")?;
        }
        if let Some(app) = self.app {
            w.write_str(":app=")?;
            w.write_str(app.as_str())?;
        }
        if let Some(title) = &self.title {
            w.write_str(":title=")?;
            title.write_base64(w)?;
        }
        if let Some(msg) = &self.msg {
            w.write_str(":msg=")?;
            msg.write_base64(w)?;
        }
        w.write_str(term.as_str())
    }

    /// Length in bytes of [`write`](Self::write)'s output.
    pub fn encoded_len(&self, term: Terminator) -> usize {
        let opt = |k: &str, v: Option<usize>| v.map_or(0, |n| 2 + k.len() + n);
        PREFIX.len()
            + "state=".len()
            + self.state.as_str().len()
            + opt("id", self.id.map(|i| i.as_str().len()))
            + opt("kind", self.kind.map(|k| k.as_str().len()))
            + opt("progress", self.progress.map(|p| if p >= 100 { 3 } else if p >= 10 { 2 } else { 1 }))
            + opt("app", self.app.map(|a| a.as_str().len()))
            + opt("title", self.title.map(|t| t.base64_len()))
            + opt("msg", self.msg.map(|m| m.base64_len()))
            + term.as_str().len()
    }
}

/// Writes the sequence with [`Terminator::St`].
impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(f, Terminator::St)
    }
}

impl<'a> ReportBuilder<'a> {
    /// Addresses a child record instead of the root.
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// What a blocked program needs. Dropped unless the state is `blocked`.
    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// 0..=100. Dropped unless the state is `working` or `blocked`.
    pub fn progress(mut self, progress: u8) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Program identifier, `[A-Za-z0-9_.+-]{1,32}`.
    pub fn app(mut self, app: &'a str) -> Self {
        self.app = Some(app);
        self
    }

    /// Short label, at most 192 bytes, no control characters.
    pub fn title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    /// One line of text, at most 2048 bytes, no control characters.
    /// Pass untrusted text through `sanitize` (feature `alloc`) first.
    pub fn msg(mut self, msg: &'a str) -> Self {
        self.msg = Some(msg);
        self
    }

    /// Validates everything a terminal would check, so a built report is
    /// never discarded by a conforming terminal.
    pub fn build(self) -> Result<Report<'a>, Error> {
        let state = self.state;
        if self.progress.is_some_and(|p| p > 100) {
            return Err(Error::InvalidProgress);
        }
        let report = Report {
            state,
            id: self.id.map(Id::new).transpose()?,
            kind: self.kind.filter(|_| state == State::Blocked),
            progress: self.progress.filter(|_| state.allows_progress()),
            app: self.app.map(App::new).transpose()?,
            title: self.title.map(|t| Text::plain(t, Field::Title, MAX_TITLE_DECODED)).transpose()?,
            msg: self.msg.map(|m| Text::plain(m, Field::Msg, MAX_MSG_DECODED)).transpose()?,
        };
        if report.encoded_len(Terminator::St) > MAX_SEQUENCE {
            return Err(Error::SequenceTooLong);
        }
        Ok(report)
    }
}

/// Writes `state=clear`, removing the record `id` and everything beneath it,
/// or every record when `id` is `None`.
pub fn write_clear<W: fmt::Write + ?Sized>(
    w: &mut W,
    id: Option<Id<'_>>,
    term: Terminator,
) -> fmt::Result {
    w.write_str("\x1b]7501;state=clear")?;
    if let Some(id) = id {
        w.write_str(":id=")?;
        w.write_str(id.as_str())?;
    }
    w.write_str(term.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed-capacity `fmt::Write` sink so tests stay `no_std`.
    struct Buf {
        bytes: [u8; MAX_SEQUENCE + 16],
        len: usize,
    }

    impl Buf {
        fn new() -> Self {
            Buf { bytes: [0; MAX_SEQUENCE + 16], len: 0 }
        }
        fn as_str(&self) -> &str {
            core::str::from_utf8(&self.bytes[..self.len]).unwrap()
        }
    }

    impl fmt::Write for Buf {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let end = self.len + s.len();
            self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?.copy_from_slice(s.as_bytes());
            self.len = end;
            Ok(())
        }
    }

    fn encode(r: &Report<'_>, term: Terminator) -> Buf {
        let mut b = Buf::new();
        r.write(&mut b, term).unwrap();
        assert_eq!(b.len, r.encoded_len(term));
        b
    }

    #[test]
    fn canonical_order_and_length() {
        let r = Report::builder(State::Blocked)
            .msg("Approve?")
            .title("EU West")
            .app("deploy")
            .progress(100)
            .kind(Kind::Permission)
            .id("eu-west")
            .build()
            .unwrap();
        assert_eq!(
            encode(&r, Terminator::Bel).as_str(),
            "\x1b]7501;state=blocked:id=eu-west:kind=permission:progress=100:app=deploy:title=RVUgV2VzdA==:msg=QXBwcm92ZT8=\x07"
        );
        for p in [0, 9, 10, 99] {
            let r = Report::builder(State::Working).progress(p).build().unwrap();
            encode(&r, Terminator::St);
        }
    }

    #[test]
    fn normalizes_keys_the_state_ignores() {
        let r = Report::builder(State::Done).kind(Kind::Auth).progress(5).build().unwrap();
        assert_eq!(encode(&r, Terminator::St).as_str(), "\x1b]7501;state=done\x1b\\");
    }

    #[test]
    fn refuses_what_a_terminal_would_discard() {
        let b = |s| Report::builder(s);
        assert_eq!(b(State::Working).progress(101).build(), Err(Error::InvalidProgress));
        assert_eq!(b(State::Idle).msg("a\nb").build(), Err(Error::ControlCharacter(Field::Msg)));
        assert_eq!(b(State::Idle).title("\u{9b}").build(), Err(Error::ControlCharacter(Field::Title)));
        assert_eq!(b(State::Idle).app("has space").build(), Err(Error::InvalidApp));
        assert_eq!(b(State::Idle).id("a//b").build(), Err(Error::InvalidId));
        let long = core::str::from_utf8(&[b'x'; MAX_MSG_DECODED + 1]).unwrap();
        assert_eq!(b(State::Idle).msg(long).build(), Err(Error::TooLong(Field::Msg)));
        assert!(b(State::Idle).msg(&long[1..]).build().is_ok());
    }

    #[test]
    // spec: LIM-LARGEST
    fn largest_legal_report_fits() {
        let msg = core::str::from_utf8(&[b'x'; MAX_MSG_DECODED]).unwrap();
        let title = core::str::from_utf8(&[b'y'; MAX_TITLE_DECODED]).unwrap();
        let id = "a234567890123456789012345678901/b/c/d/e/f/g/h";
        let r = Report::builder(State::Blocked)
            .id(id)
            .kind(Kind::Permission)
            .progress(100)
            .app("a2345678901234567890123456789012")
            .title(title)
            .msg(msg)
            .build()
            .unwrap();
        // The spec: "The largest legal report is under 3300."
        assert!(r.encoded_len(Terminator::St) < 3300);
    }

    #[test]
    fn clear() {
        let mut b = Buf::new();
        write_clear(&mut b, Some(Id::new("us-east").unwrap()), Terminator::St).unwrap();
        assert_eq!(b.as_str(), "\x1b]7501;state=clear:id=us-east\x1b\\");
        let mut b = Buf::new();
        write_clear(&mut b, None, Terminator::Bel).unwrap();
        assert_eq!(b.as_str(), "\x1b]7501;state=clear\x07");
    }
}
