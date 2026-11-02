#!/usr/bin/env python3
"""Fetch the Program Status Protocol spec and normalize it to plain text.

Writes .spec-cache/program-status.txt (git-ignored; the spec is not ours to
redistribute) and compares its SHA-256 with spec-tests/SPEC.lock, so a silent
upstream edit is caught. Use --update to accept a new revision after
reviewing the diff.

Usage: scripts/fetch-spec.py [--update]
"""
import hashlib
import html
import json
import pathlib
import re
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCK = ROOT / "spec-tests" / "SPEC.lock"
CACHE = ROOT / ".spec-cache" / "program-status.txt"
URL = "https://www.superlogical.com/rex/docs/build/program-status"


def normalize(page: str) -> str:
    s = re.sub(r"(?s)<(script|style).*?</\1>", "", page)
    s = re.sub(r"<(br|/p|/li|/h\d|/tr|/pre|/div)[^>]*>", "\n", s)
    s = re.sub(r"</t[dh]>", " | ", s)
    s = re.sub(r"<[^>]+>", "", s)
    s = html.unescape(s)
    # Keep only the spec itself, not the site navigation around it.
    start = s.index("Program Status Protocol (OSC 7501)")
    s = s[start:]
    return re.sub(r"\s+", " ", s).strip() + "\n"


def main() -> int:
    req = urllib.request.Request(URL, headers={"User-Agent": "termstate-spec-check"})
    with urllib.request.urlopen(req, timeout=30) as r:
        text = normalize(r.read().decode("utf-8"))
    CACHE.parent.mkdir(exist_ok=True)
    CACHE.write_text(text)
    digest = hashlib.sha256(text.encode()).hexdigest()
    rev = re.findall(r"(\d{4}-\d{2}-\d{2}) \| (\d+\.\d+) \|", text)
    latest = rev[0] if rev else ("?", "?")
    current = {"url": URL, "revision": latest[1], "revision_date": latest[0], "sha256": digest}

    if "--update" in sys.argv:
        LOCK.write_text(json.dumps(current, indent=2) + "\n")
        print(f"locked revision {latest[1]} ({latest[0]}) sha256 {digest}")
        return 0
    locked = json.loads(LOCK.read_text())
    if locked["sha256"] != digest:
        print(f"SPEC CHANGED: locked rev {locked['revision']} sha {locked['sha256'][:12]}, "
              f"live rev {latest[1]} sha {digest[:12]}. Review {CACHE}, then --update.")
        return 1
    print(f"spec unchanged: revision {latest[1]} ({latest[0]})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
