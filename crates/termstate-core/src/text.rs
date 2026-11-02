use core::fmt;

use crate::base64;
use crate::error::{Error, Field};
use crate::limits::MAX_MSG_DECODED;

/// A free-text value (`msg` or `title`): one line of UTF-8 with no control
/// characters, already validated against its limits.
///
/// A parsed report keeps the base64 slice from the input and decodes on
/// demand; a built report holds the plain text and encodes on write.
#[derive(Clone, Copy, Debug)]
pub struct Text<'a>(Repr<'a>);

#[derive(Clone, Copy, Debug)]
enum Repr<'a> {
    Plain(&'a str),
    Base64 { encoded: &'a str, decoded_len: usize },
}

impl<'a> Text<'a> {
    /// Validates plain text for `field`.
    pub(crate) fn plain(s: &'a str, field: Field, max_decoded: usize) -> Result<Self, Error> {
        if s.len() > max_decoded {
            return Err(Error::TooLong(field));
        }
        if s.chars().any(char::is_control) {
            return Err(Error::ControlCharacter(field));
        }
        Ok(Text(Repr::Plain(s)))
    }

    /// Validates a base64 value from the wire for `field`. Checks the encoded
    /// size before decoding, as the spec requires.
    pub(crate) fn encoded(
        s: &'a str,
        field: Field,
        max_encoded: usize,
        max_decoded: usize,
    ) -> Result<Self, Error> {
        if s.len() > max_encoded {
            return Err(Error::TooLong(field));
        }
        let decoded_len = base64::decoded_len(s.as_bytes()).ok_or(Error::InvalidBase64(field))?;
        if decoded_len > max_decoded {
            return Err(Error::TooLong(field));
        }
        let mut buf = [0u8; MAX_MSG_DECODED];
        let n = base64::decode(s.as_bytes(), &mut buf).ok_or(Error::InvalidBase64(field))?;
        let text = core::str::from_utf8(&buf[..n]).map_err(|_| Error::InvalidUtf8(field))?;
        if text.chars().any(char::is_control) {
            return Err(Error::ControlCharacter(field));
        }
        Ok(Text(Repr::Base64 { encoded: s, decoded_len }))
    }

    /// Decoded length in bytes.
    pub fn len(&self) -> usize {
        match self.0 {
            Repr::Plain(s) => s.len(),
            Repr::Base64 { decoded_len, .. } => decoded_len,
        }
    }

    /// Whether the decoded text is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Decodes into `buf`. Returns `None` if `buf` is shorter than [`len`](Self::len).
    pub fn decode_into<'b>(&self, buf: &'b mut [u8]) -> Option<&'b str> {
        let out = buf.get_mut(..self.len())?;
        match self.0 {
            Repr::Plain(s) => out.copy_from_slice(s.as_bytes()),
            Repr::Base64 { encoded, .. } => {
                base64::decode(encoded.as_bytes(), out)?;
            }
        }
        core::str::from_utf8(out).ok()
    }

    /// The plain text, if this value was built rather than parsed.
    pub fn as_plain(&self) -> Option<&'a str> {
        match self.0 {
            Repr::Plain(s) => Some(s),
            Repr::Base64 { .. } => None,
        }
    }

    pub(crate) fn write_base64<W: fmt::Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        match self.0 {
            Repr::Plain(s) => base64::encode(s.as_bytes(), w),
            Repr::Base64 { encoded, .. } => w.write_str(encoded),
        }
    }

    pub(crate) fn base64_len(&self) -> usize {
        match self.0 {
            Repr::Plain(s) => base64::encoded_len(s.len()),
            Repr::Base64 { encoded, .. } => encoded.len(),
        }
    }
}

/// Writes the decoded text.
impl fmt::Display for Text<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0u8; MAX_MSG_DECODED];
        f.write_str(self.decode_into(&mut buf).ok_or(fmt::Error)?)
    }
}

/// Compares decoded text.
impl PartialEq for Text<'_> {
    fn eq(&self, other: &Self) -> bool {
        let (mut a, mut b) = ([0u8; MAX_MSG_DECODED], [0u8; MAX_MSG_DECODED]);
        self.decode_into(&mut a) == other.decode_into(&mut b)
    }
}

impl Eq for Text<'_> {}

/// Makes arbitrary text safe to send as `msg` or `title`: every control
/// character (including newline and tab) becomes a space, and text longer
/// than `max_bytes` is cut at a character boundary and ends with `…`.
///
/// Use [`limits::MAX_MSG_DECODED`](crate::limits::MAX_MSG_DECODED) or
/// [`limits::MAX_TITLE_DECODED`](crate::limits::MAX_TITLE_DECODED) as `max_bytes`.
#[cfg(feature = "alloc")]
pub fn sanitize(s: &str, max_bytes: usize) -> alloc::string::String {
    const ELLIPSIS: char = '…';
    let mut out = alloc::string::String::with_capacity(s.len().min(max_bytes));
    let fits = s.len() <= max_bytes;
    let budget = if fits { max_bytes } else { max_bytes.saturating_sub(ELLIPSIS.len_utf8()) };
    for c in s.chars() {
        let c = if c.is_control() { ' ' } else { c };
        if out.len() + c.len_utf8() > budget {
            break;
        }
        out.push(c);
    }
    if !fits && max_bytes >= ELLIPSIS.len_utf8() {
        out.push(ELLIPSIS);
    }
    out
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_and_truncates() {
        assert_eq!(sanitize("a\nb\tc\u{85}d", 100), "a b c d");
        assert_eq!(sanitize("héllo wörld", 8), "héll…");
        assert!(sanitize("héllo wörld", 8).len() <= 8);
        assert_eq!(sanitize("abc", 3), "abc");
    }
}
