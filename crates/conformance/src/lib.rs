//! Conformance runner: converts termstate values to the JSON shape used by
//! the vectors in `spec-tests/`. See `spec-tests/README.md`.

use serde_json::{Map, Value, json};
use termstate_core::{Capabilities, Message, Terminator};
use termstate_store::Store;

/// The vector JSON for a decoded message.
pub fn message_json(m: &Message<'_>) -> Value {
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
        Message::Clear { id } => match id {
            Some(id) => json!({ "clear": { "id": id.as_str() } }),
            None => json!({ "clear": {} }),
        },
        Message::Feature(c) => caps_json(c),
    }
}

/// The vector JSON for a feature-detection message.
pub fn caps_json(c: &Capabilities) -> Value {
    json!({ "feature": {
        "states": c.states().map(|s| s.as_str()).collect::<Vec<_>>(),
        "kinds": c.kinds().map(|k| k.as_str()).collect::<Vec<_>>(),
    }})
}

/// The vector name of a terminator.
pub fn terminator_json(t: Terminator) -> &'static str {
    match t {
        Terminator::St => "st",
        Terminator::Bel => "bel",
    }
}

/// The vector JSON for every record in a store, root first.
pub fn records_json(store: &Store) -> Value {
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
            if let Some(a) = &r.app {
                o.insert("app".into(), a.as_str().into());
            }
            if let Some(t) = &r.title {
                o.insert("title".into(), t.as_str().into());
            }
            if let Some(t) = &r.msg {
                o.insert("msg".into(), t.as_str().into());
            }
            if let Some(a) = store.effective_app(id) {
                o.insert("effective_app".into(), a.into());
            }
            Value::Object(o)
        })
        .collect()
}
