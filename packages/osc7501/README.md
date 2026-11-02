# osc7501

Encode and parse **OSC 7501**, the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status)
(spec revision 0.4): a terminal escape sequence that lets a program say
whether it is idle, working, done, blocked on the user, or failed.

No dependencies. ESM, Node 18+ and browsers.

```js
import { encode, parseBody, sanitize } from "osc7501";

// Program side: tell the terminal you are waiting for approval.
process.stdout.write(
  encode({ state: "blocked", kind: "permission", app: "deploy", msg: sanitize(userText) }),
);

// Terminal side: parse the body between "ESC ] 7501 ;" and the terminator.
parseBody("state=working:app=cargo:progress=40");
// → { type: "report", state: "working", app: "cargo", progress: 40 }
parseBody("state=working:msg=@@@not-base64");
// → { type: "report", state: "working" }   (malformed pair skipped)
parseBody("state=working:msg=S");
// → { type: "discard", reason: "invalid-base64" }
```

`encode` throws `Osc7501Error` for anything a conforming terminal would
discard, so a report it returns is always accepted.

## Conformance

This package and the Rust crates in
[termstate](https://github.com/linyiru/termstate) pass the same
language-neutral conformance vectors, which trace every normative sentence
of the spec and are checked against Ghostty's implementation. Where the spec
leaves room, the reading is documented in
[interpretations.md](https://github.com/linyiru/termstate/blob/main/docs/interpretations.md).

## License

MIT or Apache-2.0.
