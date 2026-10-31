#!/bin/sh
while IFS= read -r line; do
    id=$(printf '%s' "$line" | sed -n 's/.*"id":[[:space:]]*\([0-9][0-9]*\).*/\1/p')
    [ -n "$id" ] || continue
    case "$line" in
        *'"method":"initialize"'*)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"weather-fixture","version":"1"}}}\n' "$id"
            ;;
        *'"method":"tools/list"'*)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"weather","description":"Return fixture weather","inputSchema":{"type":"object","properties":{}}}]}}\n' "$id"
            ;;
        *'"method":"tools/call"'*)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"27 C and clear"}]}}\n' "$id"
            ;;
    esac
done
