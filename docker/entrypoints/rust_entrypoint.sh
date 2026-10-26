#!/bin/sh
set -e

echo "=== Chatwoot Rust Production Entrypoint ==="
echo "Target Host: ${HOST:-0.0.0.0}, Port: ${PORT:-3000}"

# Eksekusi binary Chatwoot Rust
exec /usr/local/bin/chatwoot-backend "$@"
