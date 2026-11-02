//! Feature detection on the controlling terminal.

use std::fs::File;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use rustix::termios::{OptionalActions, SpecialCodeIndex, Termios, tcgetattr, tcsetattr};
use termstate_core::detect::{Detector, Outcome, QUERY};

/// Restores the terminal mode on drop, even on error or panic.
struct RawMode<'a> {
    tty: &'a File,
    saved: Termios,
}

impl Drop for RawMode<'_> {
    fn drop(&mut self) {
        let _ = tcsetattr(self.tty, OptionalActions::Now, &self.saved);
    }
}

/// Sends the query plus `CSI c` and reads replies until DA1 or `timeout`.
pub fn detect(timeout: Duration) -> std::io::Result<Outcome> {
    let tty = File::options().read(true).write(true).open("/dev/tty")?;
    let saved = tcgetattr(&tty)?;
    let mut raw = saved.clone();
    raw.make_raw();
    // Reads return after at most 100 ms so the deadline is honoured.
    raw.special_codes[SpecialCodeIndex::VMIN] = 0;
    raw.special_codes[SpecialCodeIndex::VTIME] = 1;
    tcsetattr(&tty, OptionalActions::Now, &raw)?;
    let _guard = RawMode { tty: &tty, saved };

    (&tty).write_all(QUERY)?;
    (&tty).flush()?;

    let mut detector = Detector::new();
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 512];
    while Instant::now() < deadline {
        let n = (&tty).read(&mut buf)?;
        if let Some(outcome) = detector.feed(&buf[..n]) {
            return Ok(outcome);
        }
    }
    Ok(detector.finish())
}
