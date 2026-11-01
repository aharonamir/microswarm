#!/usr/bin/env bash
set -euo pipefail

web=crates/ms-console/web
if rg -n -i 'innerHTML|outerHTML|insertAdjacentHTML|document\.write|\beval\s*\(|new\s+Function|setTimeout\s*\(\s*["'"'"']|setInterval\s*\(\s*["'"'"']' "$web"; then
  echo "forbidden executable DOM or string-timer sink in $web" >&2
  exit 1
fi
if rg -n '<script[^>]*>[^<[:space:]]|<style[^>]*>|\sstyle\s*=' "$web/index.html" 2>/dev/null; then
  echo "inline script or style in $web/index.html" >&2
  exit 1
fi
