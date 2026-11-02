# OSC 7501 conformance vectors

Language-neutral test data for the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status)
(OSC 7501), revision 0.4. Any implementation, in any language, can load
these JSON files and check itself against them.

| file | what it tests |
|---|---|
| `requirements.json` | every normative statement of the spec, quoted verbatim, with an id |
| `vectors/parse.json` | one OSC body → decoded message, or discard |
| `vectors/scan.json` | a byte stream → passthrough bytes and sequences |
| `vectors/store.json` | a series of reports and events → the terminal's records |
| `SPEC.lock` | the SHA-256 of the spec text the vectors were written against |

## How the vectors trace to the spec

1. `requirements.json` quotes each normative statement. `scripts/check-trace.py`
   fails if a quote no longer appears in the live spec text, so a spec edit
   cannot silently leave requirements stale.
2. Every case lists the requirement ids it `covers`. The checker fails if a
   requirement in a testable scope has no covering case (or a Rust test tagged
   `// spec: ID`) and no written `untested_reason`.
3. Where the spec leaves room, the case names an `interpretation`
   (`INT-n`, documented in `../docs/interpretations.md`). Implementations that
   read the spec differently can skip exactly those cases.
4. `scripts/fetch-spec.py` compares the live spec with `SPEC.lock`.

## Formats

All strings are UTF-8. Control bytes appear as JSON escapes (`\u001b` is ESC,
`\u0007` is BEL).

### Messages (parse.json `expect`, scan.json `sequence`)

```jsonc
{ "report": { "state": "blocked", "id": "eu-west", "kind": "permission",
              "progress": 40, "app": "deploy", "title": "EU West", "msg": "Approve?" } }
{ "clear": {} }                 // state=clear, no id
{ "clear": { "id": "us-east" } }
{ "feature": { "states": ["idle", "done"], "kinds": ["auth"] } }
{ "discard": true }             // nothing from the report may be applied
```

A key absent from `report` must be absent from the parsed report. `title`
and `msg` are the decoded text. `kind` and `progress` appear only where the
state allows them (INT-12). `feature` lists what the body says the terminal
accepts, with the spec defaults filled in for anything not listed.

### parse.json

`{ "name", "covers": [ids], "interpretation"?, "body", "expect" }`. `body` is
the bytes between `ESC ] 7501 ;` and the string terminator.

### scan.json

`{ "name", "covers", "interpretation"?, "stream", "expect": [events] }`.
Events, in order:

```jsonc
{ "passthrough": "bytes that are not OSC 7501" }   // adjacent runs merged
{ "sequence": <message>, "terminator": "st" | "bel" }
{ "discard": true, "terminator": "st" | "bel" }
```

An unterminated sequence at end of stream produces nothing; a partial
`ESC ] 7501` prefix at end of stream is passed through. Results must not
depend on how the stream is split into reads.

### store.json

`{ "name", "covers", "interpretation"?, "capacity"?, "steps", "expect" }`.

Steps run in order on a fresh record set (`capacity` defaults to 256):

```jsonc
{ "report": "<body>" }          // parse; apply only if not discarded
{ "event": "process_exit" | "prompt" | "reset" }   // reset is RIS
{ "config": { "drop_idle": true } }               // also drop idle on lifetime events
```

`expect` holds any of:

- `records`: every record, ordered by id with the root (`"id": null`) first,
  then by byte order. Each has `id`, `state`, the optional keys as in
  `report`, and `effective_app` (the record's or its nearest ancestor's `app`)
  when there is one.
- `count`: the number of records.
- `present` / `absent`: ids (`null` is the root) that must or must not exist.
