#!/usr/bin/env bash
set -euo pipefail

python3 - <<'PY'
import re
import subprocess
import sys

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

targets = ("x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl")
violations = []
checked = set()
for target in targets:
    tree = subprocess.run(
        [
            "cargo", "tree", "--locked", "--workspace", "--edges", "normal,build",
            "--prefix", "none", "--target", target,
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    packages = {match.group(1) for match in re.finditer(r"^([^\s]+) v[^\s]+", tree, re.MULTILINE)}
    checked.update(packages)
    for package in sorted(packages):
        if package in other_runtimes:
            violations.append(f"{target}: second async runtime/executor: {package}")
        if package in other_tls:
            violations.append(f"{target}: non-rustls TLS stack: {package}")

if violations:
    print("Dependency policy violations:", file=sys.stderr)
    for violation in violations:
        print(f"  - {violation}", file=sys.stderr)
    raise SystemExit(1)

print(f"Dependency gate passed ({len(checked)} target-reachable packages checked across {len(targets)} targets)")
PY
