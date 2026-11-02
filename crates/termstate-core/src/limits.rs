//! Hard limits from the "Limits" section of the spec (revision 0.4).
//!
//! A report that breaks any of these is discarded whole.

/// The OSC number of the protocol.
pub const OSC_NUMBER: u16 = 7501;
/// The spec revision this crate implements.
pub const SPEC_REVISION: &str = "0.4";

/// Whole sequence, OSC through ST.
pub const MAX_SEQUENCE: usize = 4096;
/// `ESC ] 7501 ;`
pub const PREFIX: &[u8] = b"\x1b]7501;";
/// Longest body that can still fit in [`MAX_SEQUENCE`] (with a one-byte BEL terminator).
pub const MAX_BODY: usize = MAX_SEQUENCE - PREFIX.len() - 1;

/// Key length in bytes.
pub const MAX_KEY: usize = 16;
/// `msg`, base64-encoded bytes.
pub const MAX_MSG_ENCODED: usize = 2732;
/// `msg`, decoded bytes.
pub const MAX_MSG_DECODED: usize = 2048;
/// `title`, base64-encoded bytes.
pub const MAX_TITLE_ENCODED: usize = 256;
/// `title`, decoded bytes.
pub const MAX_TITLE_DECODED: usize = 192;
/// `app` bytes.
pub const MAX_APP: usize = 32;
/// `id` total bytes.
pub const MAX_ID: usize = 128;
/// `id` bytes per segment.
pub const MAX_ID_SEGMENT: usize = 32;
/// `id` levels.
pub const MAX_ID_DEPTH: usize = 8;
/// Records per terminal (upper cap).
pub const MAX_RECORDS: usize = 256;
/// Records per terminal a terminal MUST support at minimum.
pub const MIN_RECORDS: usize = 64;
