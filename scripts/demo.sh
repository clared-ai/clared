#!/usr/bin/env bash
set -euo pipefail

demo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
demo_secret="0123456789abcdef0123456789abcdef"
demo_log="$(mktemp -t clared-demo.XXXXXX)"
server_pid=""

cleanup() {
    if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -f "$demo_log"
}
trap cleanup EXIT INT TERM

command -v cargo >/dev/null 2>&1 || {
    echo "Clared demo requires the Rust toolchain: https://rustup.rs" >&2
    exit 1
}

if command -v uv >/dev/null 2>&1; then
    (cd "$demo_root/clared-python" && uv sync --locked --quiet)
    demo_python="$demo_root/clared-python/.venv/bin/python"
else
    command -v python3 >/dev/null 2>&1 || {
        echo "Clared demo requires Python 3.10 or newer." >&2
        exit 1
    }
    demo_venv="$demo_root/.venv"
    if [[ ! -x "$demo_venv/bin/python" ]]; then
        python3 -m venv "$demo_venv"
    fi
    "$demo_venv/bin/python" -m pip install --quiet -e "$demo_root/clared-python"
    demo_python="$demo_venv/bin/python"
fi

(cd "$demo_root/clared-core" && cargo build --quiet)
CLARED_DELEGATION_SECRET="$demo_secret" \
    "$demo_root/clared-core/target/debug/clared-guard" >"$demo_log" 2>&1 &
server_pid=$!

if ! "$demo_python" -c '
import socket
import time

for _ in range(100):
    try:
        socket.create_connection(("127.0.0.1", 4000), timeout=0.1).close()
        break
    except OSError:
        time.sleep(0.1)
else:
    raise SystemExit("Clared reference server did not start")
'; then
    echo "Reference server log:" >&2
    sed -n '1,120p' "$demo_log" >&2
    exit 1
fi

CLARED_DELEGATION_SECRET="$demo_secret" \
    "$demo_python" "$demo_root/examples/fault_injection_demo.py"
