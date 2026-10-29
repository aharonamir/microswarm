#!/usr/bin/env bash
set -euo pipefail

limit=6000
command -v tokei >/dev/null 2>&1 || {
  echo "tokei is required; install it with: cargo install tokei" >&2
  exit 2
}

count_rust_code() {
  tokei --output json "$@" | python3 -c '
import json, sys
report = json.load(sys.stdin)
print(report.get("Rust", {}).get("code", 0))
'
}

agent=$(count_rust_code crates/ms-agent/src crates/ms-proto/src crates/ms-store/src)
supervisor=$(count_rust_code crates/ms-supervisor/src crates/ms-proto/src crates/ms-store/src)
runtime=$(count_rust_code crates/ms-runtime/src crates/ms-proto/src crates/ms-store/src)
printf 'ms-agent + shared crates: %s / %s Rust code lines\n' "$agent" "$limit"
printf 'ms-supervisor + shared crates: %s / %s Rust code lines\n' "$supervisor" "$limit"
printf 'ms-runtime + shared crates: %s / %s Rust code lines\n' "$runtime" "$limit"

if (( agent > limit || supervisor > limit || runtime > limit )); then
  echo "Rust source line budget exceeded" >&2
  exit 1
fi
