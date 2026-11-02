#!/usr/bin/env python3
"""Check the spec-to-test trace.

1. Every quote in spec-tests/requirements.json appears verbatim (modulo
   whitespace) in the cached spec text, so requirements cannot drift from
   the spec. Run scripts/fetch-spec.py first.
2. Every vector's `covers` names known requirement ids, and every
   `interpretation` names one documented in docs/interpretations.md.
3. Every requirement in a testable scope is covered by at least one vector
   or Rust test tagged `// spec: ID`; the rest carry an untested_reason.

Exit status is non-zero on any failure. Prints a coverage matrix with -v.
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SPEC = ROOT / ".spec-cache" / "program-status.txt"
TESTABLE = {"parse", "scan", "encode", "store", "detect"}


def ws(s: str) -> str:
    return re.sub(r"\s+", " ", s).strip()


def main() -> int:
    verbose = "-v" in sys.argv
    errors = []
    reqs = json.loads((ROOT / "spec-tests" / "requirements.json").read_text())["requirements"]
    ids = {r["id"] for r in reqs}
    if len(ids) != len(reqs):
        errors.append("duplicate requirement ids")

    if not SPEC.exists():
        errors.append(f"{SPEC} missing: run scripts/fetch-spec.py")
    else:
        spec = ws(SPEC.read_text())
        for r in reqs:
            if ws(r["quote"]) not in spec:
                errors.append(f"{r['id']}: quote not found in spec: {r['quote']!r}")

    interp_doc = (ROOT / "docs" / "interpretations.md").read_text()
    interps = set(re.findall(r"^## (INT-\d+)", interp_doc, re.M))

    covered: dict[str, list[str]] = {i: [] for i in ids}
    for path in sorted((ROOT / "spec-tests" / "vectors").glob("*.json")):
        doc = json.loads(path.read_text())
        names = set()
        for case in doc["cases"]:
            where = f"{path.name}:{case['name']}"
            if case["name"] in names:
                errors.append(f"{where}: duplicate case name")
            names.add(case["name"])
            if not case.get("covers") and not case.get("interpretation"):
                errors.append(f"{where}: covers nothing")
            for c in case.get("covers", []):
                if c not in ids:
                    errors.append(f"{where}: unknown requirement {c}")
                else:
                    covered[c].append(where)
            i = case.get("interpretation")
            if i and i not in interps:
                errors.append(f"{where}: {i} not documented in docs/interpretations.md")

    for path in sorted((ROOT / "crates").rglob("*.rs")):
        for m in re.finditer(r"//\s*spec:\s*([A-Z0-9, -]+)", path.read_text()):
            for c in re.split(r"[,\s]+", m.group(1).strip()):
                if c in covered:
                    covered[c].append(str(path.relative_to(ROOT)))
                elif c:
                    errors.append(f"{path.relative_to(ROOT)}: unknown requirement {c}")

    for r in reqs:
        n = len(covered[r["id"]])
        if r["scope"] in TESTABLE and n == 0 and "untested_reason" not in r:
            errors.append(f"{r['id']} ({r['scope']}): no covering test")
        if verbose:
            mark = f"{n:3d}" if n else ("n/a" if "untested_reason" in r else "  -")
            print(f"{mark}  {r['id']:<20} {r['scope']:<12} {r.get('untested_reason', '')}")

    testable = [r for r in reqs if r["scope"] in TESTABLE]
    done = sum(1 for r in testable if covered[r["id"]])
    print(f"requirements: {len(reqs)} total, {len(testable)} testable, {done} covered; "
          f"interpretations documented: {len(interps)}")
    for e in errors:
        print("ERROR:", e)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
