#!/usr/bin/env bash
set -euo pipefail

CARGO_BIN="${CARGO:-cargo}"
NODE_BIN="${NODE:-node}"

if ! command -v "$CARGO_BIN" >/dev/null 2>&1; then
  if [ -x "$HOME/.cargo/bin/cargo" ]; then
    CARGO_BIN="$HOME/.cargo/bin/cargo"
  elif command -v cargo.exe >/dev/null 2>&1; then
    CARGO_BIN="cargo.exe"
  fi
fi

if ! command -v "$NODE_BIN" >/dev/null 2>&1; then
  if command -v node.exe >/dev/null 2>&1; then
    NODE_BIN="node.exe"
  fi
fi

"$CARGO_BIN" fmt --all -- --check
"$CARGO_BIN" build --all-targets --locked
"$CARGO_BIN" test --locked
"$CARGO_BIN" clippy --all-targets --all-features --locked -- -D warnings
"$NODE_BIN" --test "tests/node/*.test.js"
