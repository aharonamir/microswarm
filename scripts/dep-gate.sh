#!/usr/bin/env bash
set -euo pipefail

python3 - <<'PY'
import pathlib
import sys
import tomllib

lock = tomllib.loads(pathlib.Path("Cargo.lock").read_text())
packages = sorted({package["name"] for package in lock.get("package", [])})

# Tokio is the sole permitted async runtime. These packages provide other
# async runtimes other than Tokio. Generic future executors are not runtimes.
other_runtimes = {
    "actix-rt", "async-executor", "async-global-executor", "async-std",
    "embassy-executor", "glommio", "monoio",
    "pollster", "smol", "tokio-uring",
}
# rustls is the only permitted TLS stack. Certificate stores and crypto
# providers are not TLS stacks and are intentionally not listed here.
other_tls = {
    "boring", "boring-sys", "curl-sys", "hyper-tls", "native-tls",
    "openssl", "openssl-sys", "s2n-tls", "schannel", "security-framework",
    "tokio-native-tls",
}

violations = []
for package in packages:
    if package in other_runtimes:
        violations.append(f"second async runtime/executor: {package}")
    if package in other_tls:
        violations.append(f"non-rustls TLS stack: {package}")

if violations:
    print("Dependency policy violations:", file=sys.stderr)
    for violation in violations:
        print(f"  - {violation}", file=sys.stderr)
    raise SystemExit(1)

print(f"Dependency gate passed ({len(packages)} locked packages checked)")
PY
