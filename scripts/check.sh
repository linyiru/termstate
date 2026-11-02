#!/usr/bin/env bash
# Everything CI runs, in one place. Usage: scripts/check.sh [--offline]
#   --offline  skip fetching the live spec (trace check uses the cached copy)
set -euo pipefail
cd "$(dirname "$0")/.."

# Prefer the rustup toolchain so llvm-cov's tools match the compiler.
if command -v rustup >/dev/null; then
  TC="$(dirname "$(rustup which rustc)")"
  export PATH="$TC:$PATH" RUSTC="$TC/rustc"
fi

step() { printf '\n== %s\n' "$*"; }

step fmt;      cargo fmt --all --check
step clippy;   cargo clippy -q --workspace --all-targets --all-features -- -D warnings
step test;     cargo test -q --workspace --all-features
step doc;      RUSTDOCFLAGS="-D warnings" cargo doc -q --workspace --no-deps --all-features
step no_std
if rustup target list --installed 2>/dev/null | grep -q thumbv7em-none-eabihf; then
  cargo build -q -p termstate-core --no-default-features --target thumbv7em-none-eabihf
  cargo build -q -p termstate-core -p termstate-store --target thumbv7em-none-eabihf
else
  echo "skipped: rustup target add thumbv7em-none-eabihf"
fi
step "js (packages/osc7501)"
if command -v node >/dev/null; then
  (cd packages/osc7501 && node --test test/*.test.js >/dev/null && echo "ok")
else
  echo "skipped: node not found"
fi
step spec
if [[ "${1:-}" != "--offline" ]]; then python3 -I scripts/fetch-spec.py; fi
python3 -I scripts/check-trace.py
step coverage
if cargo llvm-cov --version >/dev/null 2>&1; then
  cargo llvm-cov -q --workspace --all-features --summary-only --fail-under-lines 95 | tail -1
else
  echo "skipped: cargo install cargo-llvm-cov"
fi
echo; echo "all checks passed"
