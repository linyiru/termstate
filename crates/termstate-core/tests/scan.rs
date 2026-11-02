//! Scanner behaviour, including split-invariance under every chunking.

use proptest::prelude::*;
use termstate_core::scan::{Event, Scanner};
use termstate_core::{Error, Message, State, Terminator};

/// Owned form of an event, so results from different chunkings compare.
#[derive(Debug, Clone, PartialEq)]
enum Ev {
    Bytes(Vec<u8>),
    Seq(Result<String, Error>, Terminator),
}

fn describe(m: &Message<'_>) -> String {
    match m {
        Message::Report(r) => format!(
            "{} id={:?} kind={:?} progress={:?} app={:?} title={:?} msg={:?}",
            r.state,
            r.id.map(|i| i.as_str()),
            r.kind,
            r.progress,
            r.app.map(|a| a.as_str()),
            r.title.map(|t| t.to_string()),
            r.msg.map(|t| t.to_string()),
        ),
        Message::Clear { id } => format!("clear {:?}", id.map(|i| i.as_str())),
        Message::Feature(c) => format!("feature {c:?}"),
    }
}

/// Scans `chunks` in order and merges adjacent passthrough runs.
fn scan_chunks<'a>(chunks: impl IntoIterator<Item = &'a [u8]>) -> Vec<Ev> {
    let mut out: Vec<Ev> = Vec::new();
    let mut push = |e: Event<'_>| match e {
        Event::Passthrough(b) => {
            if let Some(Ev::Bytes(prev)) = out.last_mut() {
                prev.extend_from_slice(b);
            } else {
                out.push(Ev::Bytes(b.to_vec()));
            }
        }
        Event::Sequence { message, terminator } => {
            out.push(Ev::Seq(message.map(|m| describe(&m)), terminator))
        }
    };
    let mut s = Scanner::new();
    for c in chunks {
        s.feed(c, &mut push);
    }
    s.finish(&mut push);
    out
}

fn scan(input: &[u8]) -> Vec<Ev> {
    scan_chunks([input])
}

fn bytes(s: &str) -> Ev {
    Ev::Bytes(s.as_bytes().to_vec())
}

#[test]
fn extracts_and_passes_through() {
    let got = scan(b"hello\x1b]7501;state=working\x1b\\world\x1b]7501;state=done\x07!");
    assert_eq!(got.len(), 5);
    assert_eq!(got[0], bytes("hello"));
    assert!(matches!(&got[1], Ev::Seq(Ok(s), Terminator::St) if s.starts_with("working")));
    assert_eq!(got[2], bytes("world"));
    assert!(matches!(&got[3], Ev::Seq(Ok(s), Terminator::Bel) if s.starts_with("done")));
    assert_eq!(got[4], bytes("!"));
}

#[test]
fn other_sequences_pass_through_untouched() {
    for input in [
        &b"\x1b]0;title\x07"[..],
        b"\x1b]7;file:///tmp\x1b\\",
        b"\x1b]75010;x\x07",
        b"\x1b]750;x\x07",
        b"\x1b[31mred\x1b[0m",
        b"\x1b\x1b]7501",
        b"plain",
    ] {
        assert_eq!(scan(input), vec![Ev::Bytes(input.to_vec())], "{input:?}");
    }
}

#[test]
fn esc_without_backslash_aborts_and_starts_a_new_sequence() {
    let got = scan(b"\x1b]7501;state=idle\x1b]7501;state=done\x07");
    assert_eq!(got.len(), 1);
    assert!(matches!(&got[0], Ev::Seq(Ok(s), _) if s.starts_with("done")));

    let got = scan(b"\x1b]7501;state=idle\x1b[1mbold");
    assert_eq!(got, vec![bytes("\x1b[1mbold")]);
}

#[test]
fn can_and_sub_abort() {
    assert_eq!(scan(b"\x1b]7501;state=idle\x18after"), vec![bytes("after")]);
    assert_eq!(scan(b"\x1b]7501;state=idle\x1aafter"), vec![bytes("after")]);
}

#[test]
fn c0_controls_inside_the_body_are_ignored() {
    let got = scan(b"\x1b]7501;state=wor\nking\x00\x07");
    assert!(matches!(&got[0], Ev::Seq(Ok(s), _) if s.starts_with("working")));
}

#[test]
fn length_limit_counts_osc_through_st() {
    // Body that makes the sequence exactly 4096 bytes with BEL, 4097 with ESC \.
    let pad = 4096 - 7 - 1 - "state=idle:x=".len();
    let body = format!("state=idle:x={}", "a".repeat(pad));
    let bel = format!("\x1b]7501;{body}\x07");
    assert_eq!(bel.len(), 4096);
    assert!(matches!(&scan(bel.as_bytes())[0], Ev::Seq(Ok(_), _)));
    let st = format!("\x1b]7501;{body}\x1b\\");
    assert_eq!(scan(st.as_bytes())[0], Ev::Seq(Err(Error::SequenceTooLong), Terminator::St));

    let huge = format!("\x1b]7501;state=idle:x={}\x07tail", "a".repeat(100_000));
    assert_eq!(
        scan(huge.as_bytes()),
        vec![Ev::Seq(Err(Error::SequenceTooLong), Terminator::Bel), bytes("tail")]
    );
}

#[test]
fn unterminated_sequence_is_dropped_and_partial_prefix_kept() {
    assert_eq!(scan(b"x\x1b]7501;state=idle"), vec![bytes("x")]);
    assert_eq!(scan(b"x\x1b]75"), vec![bytes("x\x1b]75")]);
}

#[test]
fn state_is_parsed() {
    let mut s = Scanner::new();
    let mut seen = None;
    s.feed(b"\x1b]7501;state=blocked:kind=auth\x07", |e| {
        if let Event::Sequence { message: Ok(Message::Report(r)), .. } = e {
            seen = Some((r.state, r.kind));
        }
    });
    assert_eq!(seen, Some((State::Blocked, Some(termstate_core::Kind::Auth))));
    assert!(s.is_ground());
}

fn stream() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just(b"\x1b]7501;state=working:progress=40\x1b\\".to_vec()),
        Just(b"\x1b]7501;state=done:msg=SGk=\x07".to_vec()),
        Just(b"\x1b]7501;?\x07".to_vec()),
        Just(b"\x1b]0;title\x07".to_vec()),
        Just(b"\x1b[2J".to_vec()),
        Just(b"\x1b".to_vec()),
        Just(b"\x1b]".to_vec()),
        Just(b"\x1b]750".to_vec()),
        Just(b"\x18".to_vec()),
        proptest::collection::vec(any::<u8>(), 0..16),
        "[a-z ]{0,8}".prop_map(String::into_bytes),
    ];
    proptest::collection::vec(piece, 0..12).prop_map(|v| v.concat())
}

proptest! {
    /// However the input is split into reads, the scanner reports the same events.
    #[test]
    fn split_invariant(input in stream(), cuts in proptest::collection::vec(any::<prop::sample::Index>(), 0..6)) {
        let whole = scan(&input);
        let mut points: Vec<usize> = cuts.iter().map(|c| c.index(input.len() + 1)).collect();
        points.sort();
        let mut chunks = Vec::new();
        let mut prev = 0;
        for p in points { chunks.push(&input[prev..p]); prev = p; }
        chunks.push(&input[prev..]);
        prop_assert_eq!(scan_chunks(chunks), whole);
    }

    /// Byte-at-a-time feeding matches one-shot feeding.
    #[test]
    fn byte_at_a_time(input in stream()) {
        prop_assert_eq!(scan_chunks(input.chunks(1)), scan(&input));
    }

    /// Stripping never invents bytes: passthrough is a subsequence of the input,
    /// and input without ESC passes through unchanged.
    #[test]
    fn passthrough_is_faithful(input in stream()) {
        let out: Vec<u8> = scan(&input).into_iter().flat_map(|e| match e { Ev::Bytes(b) => b, _ => vec![] }).collect();
        let mut it = input.iter();
        prop_assert!(out.iter().all(|b| it.any(|x| x == b)));
        if !input.contains(&0x1b) { prop_assert_eq!(out, input); }
    }
}
