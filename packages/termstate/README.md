# termstate

Terminal program status for JavaScript: the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status)
(OSC 7501), spec revision 0.4.

Currently re-exports [`osc7501`](https://www.npmjs.com/package/osc7501)
(encoder, parser, sanitizer). Terminal-side pieces (stream scanner, record
store) will land here, mirroring the Rust crates in
[termstate](https://github.com/linyiru/termstate).

```js
import { encode } from "termstate";
process.stdout.write(encode({ state: "done", app: "backup", msg: "Photos synced" }));
```

MIT or Apache-2.0.
