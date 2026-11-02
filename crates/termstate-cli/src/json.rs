//! JSON output, in the same shape as the conformance vectors in spec-tests/.

use serde_json::{Map, Value, json};
use termstate_core::detect::Outcome;
use termstate_core::{Error, Message, Terminator};
use termstate_store::Store;

fn term(t: Terminator) -> &'static str {
    match t {
        Terminator::St => "st",
        Terminator::Bel => "bel",
    }
}

pub fn message(m: &Message<'_>) -> Value {
    match m {
        Message::Report(r) => {
            let mut o = Map::new();
            o.insert("state".into(), r.state.as_str().into());
            if let Some(id) = r.id {
                o.insert("id".into(), id.as_str().into());
            }
            if let Some(k) = r.kind {
                o.insert("kind".into(), k.as_str().into());
            }
            if let Some(p) = r.progress {
                o.insert("progress".into(), p.into());
            }
            if let Some(a) = r.app {
                o.insert("app".into(), a.as_str().into());
            }
            if let Some(t) = r.title {
                o.insert("title".into(), t.to_string().into());
            }
            if let Some(t) = r.msg {
                o.insert("msg".into(), t.to_string().into());
            }
            json!({ "report": o })
        }
        Message::Clear { id: Some(id) } => json!({ "clear": { "id": id.as_str() } }),
        Message::Clear { id: None } => json!({ "clear": {} }),
        Message::Feature(c) => json!({ "feature": {
            "states": c.states().map(|s| s.as_str()).collect::<Vec<_>>(),
            "kinds": c.kinds().map(|k| k.as_str()).collect::<Vec<_>>(),
        }}),
    }
}

pub fn sequence(m: &Result<Message<'_>, Error>, t: Terminator) -> Value {
    match m {
        Ok(m) => json!({ "sequence": message(m), "terminator": term(t) }),
        Err(e) => json!({ "discard": true, "reason": e.to_string(), "terminator": term(t) }),
    }
}

pub fn records(store: &Store) -> Value {
    store
        .iter()
        .map(|(id, r)| {
            let mut o = Map::new();
            o.insert("id".into(), id.map_or(Value::Null, Value::from));
            o.insert("state".into(), r.state.as_str().into());
            if let Some(k) = r.kind {
                o.insert("kind".into(), k.as_str().into());
            }
            if let Some(p) = r.progress {
                o.insert("progress".into(), p.into());
            }
            for (k, v) in [("app", &r.app), ("title", &r.title), ("msg", &r.msg)] {
                if let Some(v) = v {
                    o.insert(k.into(), v.as_str().into());
                }
            }
            if let Some(a) = store.effective_app(id) {
                o.insert("effective_app".into(), a.into());
            }
            Value::Object(o)
        })
        .collect()
}

#[cfg_attr(not(unix), allow(dead_code))]
pub fn outcome(o: &Outcome) -> Value {
    match o {
        Outcome::Supported(c) => json!({
            "supported": true,
            "states": c.states().map(|s| s.as_str()).collect::<Vec<_>>(),
            "kinds": c.kinds().map(|k| k.as_str()).collect::<Vec<_>>(),
        }),
        Outcome::Unsupported => json!({ "supported": false }),
    }
}
