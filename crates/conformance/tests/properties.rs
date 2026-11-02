//! Properties that must hold for any input, not just the vectors.

use proptest::prelude::*;
use termstate_core::{Message, Report, parse_body};

/// Bodies built from protocol-shaped pieces, so random input reaches deep paths.
fn body() -> impl Strategy<Value = Vec<u8>> {
    let pair = prop_oneof![
        prop::sample::select(vec![
            "idle", "working", "done", "blocked", "error", "clear", "paused", ""
        ])
        .prop_map(|s| format!("state={s}")),
        "[a-z0-9/._+-]{0,40}".prop_map(|s| format!("id={s}")),
        prop::sample::select(vec!["permission", "question", "auth", "x"])
            .prop_map(|s| format!("kind={s}")),
        "[0-9+-]{0,4}".prop_map(|s| format!("progress={s}")),
        "[A-Za-z0-9_.+,/=-]{0,40}".prop_map(|s| format!("app={s}")),
        "[A-Za-z0-9+/=]{0,64}".prop_map(|s| format!("msg={s}")),
        "[A-Za-z0-9+/=]{0,64}".prop_map(|s| format!("title={s}")),
        "[ -~]{0,20}",
    ];
    prop_oneof![
        prop::collection::vec(pair, 0..8).prop_map(|v| v.join(":").into_bytes()),
        prop::collection::vec(any::<u8>(), 0..200),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// The parser never panics, and anything it accepts the encoder can
    /// reproduce, and that reproduction parses to the same message.
    #[test]
    fn accepted_reports_round_trip(body in body()) {
        if let Ok(Message::Report(r)) = parse_body(&body) {
            let (title, msg) = (r.title.map(|t| t.to_string()), r.msg.map(|t| t.to_string()));
            let mut b = Report::builder(r.state);
            if let Some(id) = r.id { b = b.id(id.as_str()); }
            if let Some(k) = r.kind { b = b.kind(k); }
            if let Some(p) = r.progress { b = b.progress(p); }
            if let Some(a) = r.app { b = b.app(a.as_str()); }
            if let Some(t) = &title { b = b.title(t); }
            if let Some(m) = &msg { b = b.msg(m); }
            let wire = b.build().expect("accepted report must be encodable").to_string();
            let again = parse_body(&wire.as_bytes()[7..wire.len() - 2]).expect("re-encoded report must parse");
            prop_assert_eq!(again, Message::Report(r));
        }
    }

    /// Text the builder accepts survives the trip through base64 unchanged.
    #[test]
    fn text_round_trips(text in "\\PC{0,200}") {
        if let Ok(r) = Report::builder(termstate_core::State::Idle).msg(&text).build() {
            let wire = r.to_string();
            let Ok(Message::Report(back)) = parse_body(&wire.as_bytes()[7..wire.len() - 2]) else {
                panic!("built report did not parse: {wire:?}");
            };
            prop_assert_eq!(back.msg.unwrap().to_string(), text);
        }
    }
}
