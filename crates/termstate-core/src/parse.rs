use crate::caps::Capabilities;
use crate::error::{Error, Field};
use crate::limits::{
    MAX_APP, MAX_BODY, MAX_KEY, MAX_MSG_DECODED, MAX_MSG_ENCODED, MAX_TITLE_DECODED,
    MAX_TITLE_ENCODED,
};
use crate::model::{App, Id, Kind, Message, Report, State};
use crate::text::Text;

fn is_space(b: &u8) -> bool {
    matches!(b, b' ' | b'\t')
}

fn trim(mut s: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = s {
        if !is_space(first) {
            break;
        }
        s = rest;
    }
    while let [rest @ .., last] = s {
        if !is_space(last) {
            break;
        }
        s = rest;
    }
    s
}

fn is_value_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b',' | b'+' | b'/' | b'=' | b'-')
}

enum Pair<'a> {
    /// A well-formed `key=value`.
    Ok(&'a str, &'a str),
    /// A valid key whose value has a byte outside the value set.
    BadValue(&'a str),
    /// No `=`, or an empty or non-`[a-z]` key.
    Malformed,
}

/// Splits one pair. Both halves are ASCII by construction.
fn split_pair(part: &[u8]) -> Pair<'_> {
    let Some(eq) = part.iter().position(|&b| b == b'=') else {
        return Pair::Malformed;
    };
    let key = trim(&part[..eq]);
    let value = trim(&part[eq + 1..]);
    if key.is_empty() || !key.iter().all(u8::is_ascii_lowercase) {
        return Pair::Malformed;
    }
    let Ok(key) = core::str::from_utf8(key) else {
        return Pair::Malformed;
    };
    match core::str::from_utf8(value) {
        Ok(v) if v.bytes().all(is_value_byte) => Pair::Ok(key, v),
        _ => Pair::BadValue(key),
    }
}

/// Integer 0 to 100; anything else is absent.
fn parse_progress(v: &str) -> Option<u8> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = v.trim_start_matches('0');
    if digits.len() > 3 {
        return None;
    }
    let n: u16 = digits.bytes().fold(0, |n, b| n * 10 + u16::from(b - b'0'));
    (n <= 100).then_some(n as u8)
}

/// Parses the body of one OSC 7501 sequence: the bytes after `ESC ] 7501 ;`
/// and before the string terminator.
///
/// Implements the terminal-side rules of revision 0.4:
/// malformed pairs are skipped, unknown keys ignored, the last repeat of a
/// key wins, and any broken limit, bad base64 or control character discards
/// the whole report. An `Err` means nothing from the report may be applied.
///
/// See `docs/interpretations.md` for the places where the spec leaves room.
pub fn parse_body(body: &[u8]) -> Result<Message<'_>, Error> {
    if body.len() > MAX_BODY {
        return Err(Error::SequenceTooLong);
    }

    let mut parts = body.split(|&b| b == b':');
    let mut first = parts.next();
    let feature = first.is_some_and(|p| trim(p) == b"?");
    if feature {
        first = None;
    }

    let mut state = None;
    let mut id = None;
    let mut kind = None;
    let mut progress = None;
    let mut app = None;
    let mut title = None;
    let mut msg = None;
    let mut caps = Capabilities::default();

    for part in first.into_iter().chain(parts) {
        // A too-long key breaks a limit even if the pair is otherwise malformed.
        if let Some(eq) = part.iter().position(|&b| b == b'=') {
            if trim(&part[..eq]).len() > MAX_KEY {
                return Err(Error::TooLong(Field::Key));
            }
        }
        let (key, value) = match split_pair(part) {
            Pair::Ok(k, v) => (k, v),
            // A malformed pair is skipped, but skipping a malformed `id` would
            // make the report fall back to the root record, which the spec
            // forbids. Treat it as an id that does not match the grammar.
            Pair::BadValue("id") if !feature => return Err(Error::InvalidId),
            Pair::BadValue(_) | Pair::Malformed => continue,
        };
        if feature {
            caps.apply_pair(key, value);
            continue;
        }
        // Limits are checked on every occurrence, including ones a later
        // repeat overrides: "check every pair before touching any record".
        match key {
            "state" => state = Some(value),
            "id" => id = Some(value),
            "kind" => kind = Some(value),
            "progress" => progress = Some(value),
            "app" => {
                if value.len() > MAX_APP {
                    return Err(Error::TooLong(Field::App));
                }
                app = Some(value);
            }
            "title" => {
                title = Some(Text::encoded(
                    value,
                    Field::Title,
                    MAX_TITLE_ENCODED,
                    MAX_TITLE_DECODED,
                )?);
            }
            "msg" => {
                msg = Some(Text::encoded(
                    value,
                    Field::Msg,
                    MAX_MSG_ENCODED,
                    MAX_MSG_DECODED,
                )?);
            }
            _ => {}
        }
    }

    if feature {
        return Ok(Message::Feature(caps));
    }

    let id = id.map(Id::new).transpose()?;
    let state = match state {
        None => return Err(Error::MissingState),
        Some("clear") => return Ok(Message::Clear { id }),
        Some(s) => State::from_wire(s).ok_or(Error::UnknownState)?,
    };

    Ok(Message::Report(Report {
        state,
        id,
        kind: kind
            .filter(|_| state == State::Blocked)
            .and_then(Kind::from_wire),
        progress: progress
            .filter(|_| state.allows_progress())
            .and_then(parse_progress),
        app: app.and_then(|a| App::new(a).ok()),
        title,
        msg,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(body: &str) -> Report<'_> {
        match parse_body(body.as_bytes()) {
            Ok(Message::Report(r)) => r,
            other => panic!("{body:?}: {other:?}"),
        }
    }

    #[test]
    fn spec_example() {
        let r = report(
            "state=blocked:kind=permission:app=terraform:msg=QXBwbHkgMyB0byBhZGQsIDEgdG8gY2hhbmdlLCAwIHRvIGRlc3Ryb3k/",
        );
        assert_eq!(r.state, State::Blocked);
        assert_eq!(r.kind, Some(Kind::Permission));
        assert_eq!(r.app.unwrap().as_str(), "terraform");
        let mut buf = [0u8; 64];
        assert_eq!(
            r.msg.unwrap().decode_into(&mut buf),
            Some("Apply 3 to add, 1 to change, 0 to destroy?")
        );
    }

    #[test]
    fn progress_values() {
        for (v, want) in [
            ("0", Some(0)),
            ("100", Some(100)),
            ("0042", Some(42)),
            ("101", None),
            ("4.5", None),
            ("", None),
            ("99999999999999999999", None),
        ] {
            assert_eq!(parse_progress(v), want, "{v}");
        }
    }

    #[test]
    fn whitespace_and_malformed_pairs() {
        let r = report(" state = working : bogus : Foo=1 : progress= 7 :x=a b");
        assert_eq!(r.state, State::Working);
        assert_eq!(r.progress, Some(7));
    }

    #[test]
    fn feature() {
        assert_eq!(
            parse_body(b"?"),
            Ok(Message::Feature(Capabilities::default()))
        );
        let Ok(Message::Feature(c)) = parse_body(b"? : states=idle,done,future : kinds=") else {
            panic!()
        };
        assert_eq!(
            c.states().collect::<heapless_vec::V<State>>().0,
            [Some(State::Idle), Some(State::Done), None, None, None]
        );
        assert_eq!(c.kinds().count(), 0);
    }

    /// Tiny fixed-capacity collector so this test stays `no_std`.
    mod heapless_vec {
        pub struct V<T>(pub [Option<T>; 5]);
        impl<T: Copy> FromIterator<T> for V<T> {
            fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
                let mut a = [None; 5];
                for (slot, x) in a.iter_mut().zip(iter) {
                    *slot = Some(x);
                }
                V(a)
            }
        }
    }
}
