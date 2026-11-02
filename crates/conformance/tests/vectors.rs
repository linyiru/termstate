//! Runs every case in spec-tests/vectors against termstate-core and
//! termstate-store. A failure prints the case name and both values.

use std::path::PathBuf;

use serde_json::{Value, json};
use termstate_conformance::{message_json, records_json, terminator_json};
use termstate_core::scan::{Event, Scanner};
use termstate_core::{Message, Report, parse_body};
use termstate_store::Store;

fn load(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec-tests/vectors").join(name);
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    doc["cases"].as_array().unwrap().clone()
}

fn check(failures: &mut Vec<String>, case: &Value, got: &Value, want: &Value) {
    if got != want {
        failures.push(format!("{}\n    want {want}\n    got  {got}", case["name"]));
    }
}

fn finish(file: &str, total: usize, failures: Vec<String>) {
    assert!(failures.is_empty(), "{} failed checks across {total} {file} cases:\n  {}", failures.len(), failures.join("\n  "));
}

fn parse_json(body: &[u8]) -> Value {
    match parse_body(body) {
        Ok(m) => message_json(&m),
        Err(_) => json!({ "discard": true }),
    }
}

/// Re-encodes a parsed report through the program-side builder.
fn reencode(r: &Report<'_>) -> String {
    let (title, msg) = (r.title.map(|t| t.to_string()), r.msg.map(|t| t.to_string()));
    let mut b = Report::builder(r.state);
    if let Some(id) = r.id { b = b.id(id.as_str()); }
    if let Some(k) = r.kind { b = b.kind(k); }
    if let Some(p) = r.progress { b = b.progress(p); }
    if let Some(a) = r.app { b = b.app(a.as_str()); }
    if let Some(t) = &title { b = b.title(t); }
    if let Some(m) = &msg { b = b.msg(m); }
    b.build().expect("a parsed report must be encodable").to_string()
}

#[test]
fn parse_vectors() {
    let cases = load("parse.json");
    let mut failures = Vec::new();
    for case in &cases {
        let body = case["body"].as_str().unwrap().as_bytes();
        let got = parse_json(body);
        check(&mut failures, case, &got, &case["expect"]);

        // Round trip: what the parser accepts, the encoder reproduces.
        if let Ok(Message::Report(r)) = parse_body(body) {
            let wire = reencode(&r);
            let inner = &wire.as_bytes()[7..wire.len() - 2];
            check(&mut failures, case, &parse_json(inner), &case["expect"]);
        }
    }
    finish("parse", cases.len(), failures);
}

#[test]
fn scan_vectors() {
    let cases = load("scan.json");
    let mut failures = Vec::new();
    for case in &cases {
        let stream = case["stream"].as_str().unwrap().as_bytes();
        // Whole input and byte-at-a-time must agree with the vector.
        for chunk in [stream.len().max(1), 1] {
            let mut events: Vec<Value> = Vec::new();
            let mut pass: Vec<u8> = Vec::new();
            let mut on = |e: Event<'_>| match e {
                Event::Passthrough(b) => pass.extend_from_slice(b),
                Event::Sequence { message, terminator } => {
                    if !pass.is_empty() {
                        events.push(json!({ "passthrough": String::from_utf8(std::mem::take(&mut pass)).unwrap() }));
                    }
                    let t = terminator_json(terminator);
                    events.push(match message {
                        Ok(m) => json!({ "sequence": message_json(&m), "terminator": t }),
                        Err(_) => json!({ "discard": true, "terminator": t }),
                    });
                }
            };
            let mut s = Scanner::new();
            for c in stream.chunks(chunk) {
                s.feed(c, &mut on);
            }
            s.finish(&mut on);
            if !pass.is_empty() {
                events.push(json!({ "passthrough": String::from_utf8(pass).unwrap() }));
            }
            check(&mut failures, case, &Value::Array(events), &case["expect"]);
        }
    }
    finish("scan", cases.len(), failures);
}

#[test]
fn store_vectors() {
    let cases = load("store.json");
    let mut failures = Vec::new();
    for case in &cases {
        let mut store = case["capacity"].as_u64().map_or_else(Store::new, |c| Store::with_capacity(c as usize));
        for step in case["steps"].as_array().unwrap() {
            if let Some(body) = step["report"].as_str() {
                if let Ok(m) = parse_body(body.as_bytes()) {
                    store.apply(&m);
                }
            } else if let Some(e) = step["event"].as_str() {
                match e {
                    "process_exit" => store.process_exited(),
                    "prompt" => store.prompt_started(),
                    "reset" => store.reset(),
                    other => panic!("unknown event {other}"),
                };
            } else if let Some(cfg) = step.get("config") {
                if let Some(on) = cfg["drop_idle"].as_bool() {
                    store.set_drop_idle_on_lifetime_events(on);
                }
            } else {
                panic!("unknown step {step}");
            }
        }
        let want = &case["expect"];
        let records = records_json(&store);
        if want.get("records").is_some() {
            check(&mut failures, case, &records, &want["records"]);
        }
        if let Some(n) = want["count"].as_u64() {
            check(&mut failures, case, &json!(store.len()), &json!(n));
        }
        let ids = |key: &str| want[key].as_array().cloned().unwrap_or_default();
        for id in ids("present") {
            if store.get(id.as_str()).is_none() {
                failures.push(format!("{}: {id} should be present", case["name"]));
            }
        }
        for id in ids("absent") {
            if store.get(id.as_str()).is_some() {
                failures.push(format!("{}: {id} should be absent", case["name"]));
            }
        }
    }
    finish("store", cases.len(), failures);
}
