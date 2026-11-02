//! The terminal side of OSC 7501: the set of records one terminal holds.
//!
//! [`Store`] applies decoded [`Message`]s from `termstate-core` following the
//! "Records and ids", "States" (lifetime) and "Limits" sections of the
//! Program Status Protocol, revision 0.4:
//!
//! - a report replaces its record completely;
//! - `clear` removes the addressed record and every record beneath it;
//! - a record without `app` inherits it from its nearest ancestor;
//! - process exit and a new shell prompt drop `working` and `blocked`
//!   (and, by policy, `idle`); `done` and `error` survive;
//! - at most 256 records, least recently updated evicted first;
//! - a full reset (RIS) removes everything.
//!
//! Presentation (notifications, icons, rate limiting) is left to the caller.
//! Every mutating method returns the [`Change`]s it made, in order.
#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use termstate_core::limits::{MAX_RECORDS, MIN_RECORDS};
use termstate_core::{Kind, Message, Report, State};

/// What one record holds: exactly the keys of the last report for its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The state.
    pub state: State,
    /// What a blocked program needs (only with `blocked`).
    pub kind: Option<Kind>,
    /// 0..=100 (only with `working` or `blocked`); `None` is indeterminate.
    pub progress: Option<u8>,
    /// The `app` the report carried. See [`Store::effective_app`] for the
    /// inherited value.
    pub app: Option<String>,
    /// Decoded `title`.
    pub title: Option<String>,
    /// Decoded `msg`.
    pub msg: Option<String>,
}

impl Record {
    /// Converts a validated report into an owned record.
    pub fn from_report(r: &Report<'_>) -> Self {
        Record {
            state: r.state,
            kind: r.kind,
            progress: r.progress,
            app: r.app.map(|a| a.as_str().into()),
            title: r.title.map(|t| t.to_string()),
            msg: r.msg.map(|t| t.to_string()),
        }
    }
}

/// One change to the store. Ids are `None` for the root record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// The record was created or replaced.
    Set(Option<String>),
    /// The record was removed by `clear`, a lifetime event, or a reset.
    Removed(Option<String>),
    /// The record was removed to make room under the record cap.
    Evicted(Option<String>),
}

#[derive(Clone, Debug)]
struct Entry {
    record: Record,
    touched: u64,
}

/// The records of one terminal. See the [crate docs](crate).
#[derive(Clone, Debug)]
pub struct Store {
    // The root record is stored under "", which sorts first and is an
    // ancestor of every id.
    records: BTreeMap<String, Entry>,
    capacity: usize,
    clock: u64,
    drop_idle: bool,
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

fn key(id: Option<&str>) -> &str {
    id.unwrap_or("")
}

fn id_of(key: &str) -> Option<String> {
    (!key.is_empty()).then(|| key.into())
}

/// Whether `key` is `ancestor` or beneath it.
fn within(ancestor: &str, key: &str) -> bool {
    ancestor.is_empty()
        || key == ancestor
        || (key.len() > ancestor.len()
            && key.starts_with(ancestor)
            && key.as_bytes()[ancestor.len()] == b'/')
}

impl Store {
    /// A store with the spec's full cap of 256 records.
    pub fn new() -> Self {
        Self::with_capacity(MAX_RECORDS)
    }

    /// A store holding at most `capacity` records, clamped to 64..=256
    /// (terminals MAY choose a lower limit but MUST support at least 64).
    pub fn with_capacity(capacity: usize) -> Self {
        Store {
            records: BTreeMap::new(),
            capacity: capacity.clamp(MIN_RECORDS, MAX_RECORDS),
            clock: 0,
            drop_idle: false,
        }
    }

    /// Also drop `idle` records on process exit and new prompts (the spec
    /// allows either). Off by default.
    pub fn set_drop_idle_on_lifetime_events(&mut self, on: bool) {
        self.drop_idle = on;
    }

    /// The record cap.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The record for `id` (`None` is the root record).
    pub fn get(&self, id: Option<&str>) -> Option<&Record> {
        self.records.get(key(id)).map(|e| &e.record)
    }

    /// All records, ordered by id with the root first.
    pub fn iter(&self) -> impl Iterator<Item = (Option<&str>, &Record)> {
        self.records
            .iter()
            .map(|(k, e)| ((!k.is_empty()).then_some(k.as_str()), &e.record))
    }

    /// The `app` of the record, or of its nearest ancestor record that has
    /// one. Ancestors do not need to exist. `None` if no record on the path
    /// carries an app.
    pub fn effective_app(&self, id: Option<&str>) -> Option<&str> {
        let mut k = key(id);
        loop {
            if let Some(app) = self.records.get(k).and_then(|e| e.record.app.as_deref()) {
                return Some(app);
            }
            if k.is_empty() {
                return None;
            }
            k = k.rfind('/').map_or("", |i| &k[..i]);
        }
    }

    /// Applies one decoded message. Feature-detection messages change nothing.
    pub fn apply(&mut self, message: &Message<'_>) -> Vec<Change> {
        match message {
            Message::Report(r) => self.set(r),
            Message::Clear { id } => self.clear(id.map(|i| i.as_str())),
            Message::Feature(_) => Vec::new(),
        }
    }

    /// Replaces the addressed record with `report`.
    pub fn set(&mut self, report: &Report<'_>) -> Vec<Change> {
        let mut changes = Vec::new();
        let k = key(report.id.map(|i| i.as_str()));
        self.clock += 1;
        let entry = Entry {
            record: Record::from_report(report),
            touched: self.clock,
        };
        if let Some(existing) = self.records.get_mut(k) {
            *existing = entry;
        } else {
            if self.records.len() >= self.capacity {
                let oldest = self
                    .records
                    .iter()
                    .min_by_key(|(_, e)| e.touched)
                    .map(|(k, _)| k.clone());
                if let Some(oldest) = oldest {
                    self.records.remove(&oldest);
                    changes.push(Change::Evicted(id_of(&oldest)));
                }
            }
            self.records.insert(k.to_string(), entry);
        }
        changes.push(Change::Set(id_of(k)));
        changes
    }

    /// Removes the record `id` and every record beneath it, or every record
    /// when `id` is `None`.
    pub fn clear(&mut self, id: Option<&str>) -> Vec<Change> {
        let ancestor = key(id);
        self.remove_where(|k, _| within(ancestor, k))
    }

    /// The process attached to the terminal exited: drops `working` and
    /// `blocked` records (and `idle` ones if so configured).
    pub fn process_exited(&mut self) -> Vec<Change> {
        self.lifetime_event()
    }

    /// A new shell prompt began (OSC 133 `A`): same effect as
    /// [`process_exited`](Self::process_exited).
    pub fn prompt_started(&mut self) -> Vec<Change> {
        self.lifetime_event()
    }

    /// A full reset (RIS): removes every record. A soft reset (DECSTR) must
    /// not call this.
    pub fn reset(&mut self) -> Vec<Change> {
        self.clear(None)
    }

    fn lifetime_event(&mut self) -> Vec<Change> {
        let drop_idle = self.drop_idle;
        self.remove_where(|_, r| r.state.is_transient() || (drop_idle && r.state == State::Idle))
    }

    fn remove_where(&mut self, mut pred: impl FnMut(&str, &Record) -> bool) -> Vec<Change> {
        let doomed: Vec<String> = self
            .records
            .iter()
            .filter(|(k, e)| pred(k, &e.record))
            .map(|(k, _)| k.clone())
            .collect();
        doomed
            .into_iter()
            .map(|k| {
                self.records.remove(&k);
                Change::Removed(id_of(&k))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use termstate_core::parse_body;

    fn apply(s: &mut Store, body: &str) -> Vec<Change> {
        s.apply(&parse_body(body.as_bytes()).unwrap())
    }

    #[test]
    fn within_matches_segments_only() {
        assert!(within("", "a"));
        assert!(within("a", "a"));
        assert!(within("a", "a/b"));
        assert!(!within("a", "ab"));
        assert!(!within("a/b", "a"));
    }

    #[test]
    fn eviction_spares_updates_and_takes_least_recent() {
        let mut s = Store::with_capacity(0);
        assert_eq!(s.capacity(), MIN_RECORDS);
        for i in 0..MIN_RECORDS {
            apply(&mut s, &alloc::format!("state=idle:id=r{i}"));
        }
        // Touch r0 so r1 becomes the least recently updated.
        assert_eq!(
            apply(&mut s, "state=done:id=r0"),
            [Change::Set(Some("r0".into()))]
        );
        assert_eq!(
            apply(&mut s, "state=idle:id=new"),
            [
                Change::Evicted(Some("r1".into())),
                Change::Set(Some("new".into()))
            ]
        );
        assert_eq!(s.len(), MIN_RECORDS);
    }
}
