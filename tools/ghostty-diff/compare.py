#!/usr/bin/env python3
"""Compare Ghostty's parse results (TSDIFF lines) with spec-tests/vectors/parse.json.

Usage: compare.py parse.json ghostty.log
Exit status 1 if any terminal-side case differs other than the known ones.
"""
import json, re, sys
cases = json.load(open(sys.argv[1]))["cases"]
got = {}
for line in open(sys.argv[2]):
    m = re.match(r"TSDIFF (\d+) (.*)", line.strip())
    if m: got[int(m.group(1))] = m.group(2)
def h(v):
    if v == "-": return None
    if v.endswith("~"): return ""
    return bytes.fromhex(v).decode()
def conv(s):
    if s == "discard": return {"discard": True}
    if s == "query": return "query"
    f = dict(kv.split("=", 1) for kv in s.split()[1:])
    st = f["state"]
    if st == "clear":
        return {"clear": ({"id": h(f["id"])} if f["id"] != "-" else {})}
    r = {"state": st}
    if f["id"] != "-": r["id"] = h(f["id"])
    if f["kind"] != "-": r["kind"] = f["kind"]
    if f["progress"] != "-": r["progress"] = int(f["progress"])
    for k in ("app", "title", "msg"):
        if f[k] != "-": r[k] = h(f[k])
    return {"report": r}
diffs = 0
# Known, documented differences (see docs/interpretations.md).
KNOWN = {"query with whitespace"}  # INT-11: Ghostty accepts exactly "?"
skipped = 0
for i, c in enumerate(cases):
    if c.get("direction") == "reply":  # Ghostty is terminal-side only
        skipped += 1
        continue
    g = conv(got[i]); e = c["expect"]
    if g == "query" and "feature" in e: continue
    if c["name"] in KNOWN:
        print(f"  known: {c['name']} [{c.get('interpretation', '')}]")
        continue
    if g != e:
        diffs += 1
        short = lambda x: json.dumps(x, ensure_ascii=False)[:110]
        print(f"- {c['name']}  [{c.get('interpretation','')}]\n    ours:    {short(e)}\n    ghostty: {short(g)}")
print(f"{len(cases)} cases, {skipped} reply-only skipped, {len(KNOWN)} known, {diffs} unexpected differences")
sys.exit(1 if diffs else 0)
