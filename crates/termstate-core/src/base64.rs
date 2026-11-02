//! Standard-alphabet base64 (RFC 4648 §4), padding optional on decode.

use core::fmt;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn value(c: u8) -> Option<u32> {
    Some(match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    } as u32)
}

/// Splits off padding and returns the data part, or `None` if the shape is invalid.
fn data_part(input: &[u8]) -> Option<&[u8]> {
    let pad = input.iter().rev().take_while(|&&c| c == b'=').count();
    if pad > 2 {
        return None;
    }
    let data = &input[..input.len() - pad];
    if pad > 0 && input.len() % 4 != 0 {
        return None;
    }
    if data.len() % 4 == 1 {
        return None;
    }
    Some(data)
}

/// Number of bytes `input` decodes to, or `None` if it is not valid base64.
/// Does not check the alphabet; [`decode`] does.
pub(crate) fn decoded_len(input: &[u8]) -> Option<usize> {
    let data = data_part(input)?;
    Some(data.len() / 4 * 3 + [0, 0, 1, 2][data.len() % 4])
}

/// Decodes into `out`, which must be at least [`decoded_len`] bytes.
/// Returns the number of bytes written. Non-zero trailing bits are accepted.
pub(crate) fn decode(input: &[u8], out: &mut [u8]) -> Option<usize> {
    let data = data_part(input)?;
    let mut n = 0;
    for chunk in data.chunks(4) {
        let mut acc: u32 = 0;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= value(c)? << (18 - 6 * i);
        }
        // Bits below the last whole byte must be zero (RFC 4648 §3.5,
        // canonical encoding), as Ghostty's decoder requires (INT-9).
        let unused: u32 = match chunk.len() {
            2 => 0xffff,
            3 => 0xff,
            _ => 0,
        };
        if acc & unused != 0 {
            return None;
        }
        let bytes = [(acc >> 16) as u8, (acc >> 8) as u8, acc as u8];
        let take = chunk.len() - 1;
        out.get_mut(n..n + take)?.copy_from_slice(&bytes[..take]);
        n += take;
    }
    Some(n)
}

/// Length of the padded encoding of `len` bytes.
pub(crate) const fn encoded_len(len: usize) -> usize {
    len.div_ceil(3) * 4
}

/// Writes the padded encoding of `input`.
pub(crate) fn encode<W: fmt::Write + ?Sized>(input: &[u8], w: &mut W) -> fmt::Result {
    let mut quad = [0u8; 4];
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let acc = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for (i, slot) in quad.iter_mut().enumerate() {
            *slot = if i <= chunk.len() {
                ALPHABET[(acc >> (18 - 6 * i) & 63) as usize]
            } else {
                b'='
            };
        }
        // `quad` is ASCII by construction.
        w.write_str(core::str::from_utf8(&quad).map_err(|_| fmt::Error)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Option<([u8; 64], usize)> {
        let mut out = [0u8; 64];
        let n = decode(s.as_bytes(), &mut out)?;
        assert_eq!(Some(n), decoded_len(s.as_bytes()));
        Some((out, n))
    }

    #[test]
    fn round_trips() {
        struct S([u8; 64], usize);
        impl fmt::Write for S {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0[self.1..self.1 + s.len()].copy_from_slice(s.as_bytes());
                self.1 += s.len();
                Ok(())
            }
        }
        for input in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let mut s = S([0; 64], 0);
            encode(input, &mut s).unwrap();
            assert_eq!(s.1, encoded_len(input.len()));
            let enc = core::str::from_utf8(&s.0[..s.1]).unwrap();
            let (out, n) = dec(enc).unwrap();
            assert_eq!(&out[..n], input);
            // Unpadded form decodes too.
            let (out, n) = dec(enc.trim_end_matches('=')).unwrap();
            assert_eq!(&out[..n], input);
        }
    }

    #[test]
    fn rejects_malformed() {
        assert!(dec("A").is_none());
        assert!(dec("AAAAA").is_none());
        assert!(dec("AA=").is_none());
        assert!(dec("A===").is_none());
        assert!(dec("AA=A").is_none());
        assert!(dec("AA-_").is_none());
        // Non-canonical: non-zero bits after the last whole byte.
        assert!(dec("QR==").is_none());
        assert!(dec("QR").is_none());
        assert!(dec("QUJ=").is_none());
        assert!(dec("QUI=").is_some());
    }
}
