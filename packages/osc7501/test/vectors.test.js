// Runs the shared conformance vectors (spec-tests/vectors/parse.json).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { encode, encodeClear, parseBody, query, sanitize, Osc7501Error } from "../index.js";

const vectors = JSON.parse(
  readFileSync(new URL("../../../spec-tests/vectors/parse.json", import.meta.url), "utf8"),
);

/** Our message shape to the vector shape. */
function toVector(m) {
  switch (m.type) {
    case "discard":
      return { discard: true };
    case "clear":
      return { clear: m.id === undefined ? {} : { id: m.id } };
    case "feature":
      return { feature: { states: m.states, kinds: m.kinds } };
    default: {
      const { type, ...report } = m;
      return { report };
    }
  }
}

for (const c of vectors.cases) {
  test(`parse: ${c.name}`, () => {
    const got = parseBody(c.body);
    assert.deepEqual(toVector(got), c.expect);
    if (got.type === "report") {
      // Round trip: what the parser accepts, the encoder reproduces.
      const { type, ...report } = got;
      const wire = encode(report);
      assert.deepEqual(parseBody(wire.slice(7, -2)), got);
    }
  });
}

test("encode: spec example bytes", () => {
  assert.equal(
    encode({ state: "done", app: "brew", msg: "Upgraded 12 packages" }),
    "\x1b]7501;state=done:app=brew:msg=VXBncmFkZWQgMTIgcGFja2FnZXM=\x1b\\",
  );
  assert.equal(
    encode({ state: "blocked", id: "eu-west", kind: "permission", progress: 100, app: "deploy", title: "EU West", msg: "Approve?" }, { terminator: "bel" }),
    "\x1b]7501;state=blocked:id=eu-west:kind=permission:progress=100:app=deploy:title=RVUgV2VzdA==:msg=QXBwcm92ZT8=\x07",
  );
});

test("encode: drops what the state ignores and empty text", () => {
  assert.equal(encode({ state: "done", kind: "auth", progress: 5, msg: "" }), "\x1b]7501;state=done\x1b\\");
});

test("encode: refuses what a terminal would discard", () => {
  for (const [r, code] of [
    [{ state: "paused" }, "unknown-state"],
    [{ state: "working", progress: 101 }, "invalid-progress"],
    [{ state: "working", progress: 4.5 }, "invalid-progress"],
    [{ state: "idle", msg: "a\nb" }, "control-character"],
    [{ state: "idle", title: "\u009b" }, "control-character"],
    [{ state: "idle", app: "has space" }, "invalid-app"],
    [{ state: "idle", id: "a//b" }, "invalid-id"],
    [{ state: "idle", msg: "x".repeat(2049) }, "too-long"],
  ]) {
    assert.throws(() => encode(r), (e) => e instanceof Osc7501Error && e.code === code, JSON.stringify(r));
  }
});

test("encode: largest legal report is under 3300 bytes", () => {
  const wire = encode({
    state: "blocked",
    id: "a234567890123456789012345678901/b/c/d/e/f/g/h",
    kind: "permission",
    progress: 100,
    app: "a".repeat(32),
    title: "y".repeat(192),
    msg: "x".repeat(2048),
  });
  assert.ok(wire.length < 3300);
});

test("clear and query", () => {
  assert.equal(encodeClear("us-east"), "\x1b]7501;state=clear:id=us-east\x1b\\");
  assert.equal(encodeClear(undefined, { terminator: "bel" }), "\x1b]7501;state=clear\x07");
  assert.equal(query(), "\x1b]7501;?\x1b\\\x1b[c");
});

test("sanitize", () => {
  assert.equal(sanitize("a\nb\tc\u0085d"), "a b c d");
  assert.equal(sanitize("héllo wörld", 8), "héll…");
  assert.equal(sanitize("abc", 3), "abc");
});

test("utf-8 text round trips", () => {
  const msg = "完成 ✓ — 12 個套件 🎉";
  assert.equal(parseBody(encode({ state: "done", msg }).slice(7, -2)).msg, msg);
});
