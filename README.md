# termstate

A conformance-tested implementation of **OSC 7501**, the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status):
a terminal escape sequence that lets a program tell the terminal whether it is
idle, working, done, blocked on the user, or failed, and why.

```
ESC ] 7501 ; state=blocked:kind=permission:app=terraform:msg=QXBwbHkgMyB0byBhZGQsIDEgdG8gY2hhbmdlLCAwIHRvIGRlc3Ryb3k/ ESC \
```

Implements spec revision **0.4** (2026-10-10).

## Crates

| crate | for | |
|---|---|---|
| [`termstate-core`](crates/termstate-core) | both sides | `no_std`, no allocation, no dependencies. Report builder and encoder, body parser, streaming scanner that extracts or strips sequences from terminal output, feature-detection query and reply. |
| [`termstate-store`](crates/termstate-store) | terminals, multiplexers | `no_std` + `alloc`. The record set one terminal keeps: replacement, subtree `clear`, `app` inheritance, lifetime events, 64–256 record cap with LRU eviction. |

Program side:

```rust
use termstate_core::{Report, State, Kind};

let report = Report::builder(State::Blocked)
    .kind(Kind::Permission)
    .app("terraform")
    .msg("Apply 3 to add, 1 to change, 0 to destroy?")
    .build()?;            // refuses anything a terminal would discard
print!("{report}");       // ESC ] 7501 ; ... ESC \
```

Terminal or multiplexer side:

```rust
use termstate_core::scan::{Event, Scanner};
use termstate_store::Store;

let mut scanner = Scanner::new();
let mut store = Store::new();
scanner.feed(&pty_output, |event| match event {
    Event::Passthrough(bytes) => forward(bytes),
    Event::Sequence { message: Ok(m), .. } => { store.apply(&m); }
    Event::Sequence { message: Err(_), .. } => {} // discarded, per spec
});
```

## How conformance is verified

The goal is that every behaviour traces to a sentence in the spec, and that
nothing drifts silently. Run everything with `scripts/check.sh`.

1. **Requirement trace.** [`spec-tests/requirements.json`](spec-tests/requirements.json)
   quotes all 54 normative statements of the spec, verbatim, with ids.
   `scripts/check-trace.py` fails if a quote no longer appears in the live
   spec, or if a testable requirement has no covering test.
2. **Spec lock.** `scripts/fetch-spec.py` hashes the live spec and compares it
   with [`spec-tests/SPEC.lock`](spec-tests/SPEC.lock); CI runs it weekly.
3. **Language-neutral vectors.** [`spec-tests/vectors`](spec-tests/vectors):
   145 parse, 21 scan and 19 store cases in plain JSON, each naming the
   requirements it covers. Any implementation in any language can load them
   ([format](spec-tests/README.md)).
4. **Written interpretations.** Where the spec leaves room, the choice is
   documented in [`docs/interpretations.md`](docs/interpretations.md)
   (INT-1…17) and the vectors that depend on it say so.
5. **Differential testing against Ghostty.** `tools/ghostty-diff/run.sh` feeds
   every vector to libghostty-vt's parser (by the spec's author) and compares.
   At ghostty `0b1729e9`: 0 unexpected differences on terminal-side cases.
6. **Properties.** Scanner output does not depend on how input is split into
   reads; the parser never panics; every accepted report re-encodes to itself.
7. **Coverage floor.** `cargo llvm-cov` must report ≥ 95% of lines.

## Status

Pre-release (0.0.x). Planned: a `std` facade with tty output and feature
detection, a `termstate` CLI, a C ABI for other languages, and WASM.

## License

MIT or Apache-2.0, at your option.
