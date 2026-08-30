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

port_in_use() {
    if command -v python3 >/dev/null 2>&1; then
        python3 -c 'import socket, sys
s = socket.socket()
s.settimeout(0.2)
r = s.connect_ex(("127.0.0.1", 4000))
s.close()
sys.exit(0 if r == 0 else 1)'
    else
        bash -c 'echo >/dev/tcp/127.0.0.1/4000' >/dev/null 2>&1
    fi
}

if port_in_use; then
    echo "Clared demo needs TCP port 4000 free. Something is already listening there." >&2
    exit 1
fi

if command -v uv >/dev/null 2>&1; then
    if ! (cd "$demo_root/clared-python" && uv sync --locked --quiet); then
        echo "Clared demo: uv sync failed. Need Python 3.10+ and a valid clared-python/uv.lock." >&2
        exit 1
    fi
    demo_python="$demo_root/clared-python/.venv/bin/python"
    if [[ ! -x "$demo_python" ]]; then
        echo "Clared demo: uv sync did not create $demo_python" >&2
        exit 1
    fi
else
    command -v python3 >/dev/null 2>&1 || {
        echo "Clared demo requires Python 3.10 or newer." >&2
        exit 1
    }
    if ! python3 -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)'; then
        echo "Clared demo requires Python 3.10 or newer (found $(python3 --version 2>&1))." >&2
        exit 1
    fi
    demo_venv="$demo_root/.venv"
    if [[ ! -x "$demo_venv/bin/python" ]]; then
        if ! python3 -m venv "$demo_venv"; then
            echo "Clared demo: failed to create a virtualenv at $demo_venv" >&2
            exit 1
        fi
    fi
    if ! "$demo_venv/bin/python" -m pip install --quiet -e "$demo_root/clared-python"; then
        echo "Clared demo: pip install of clared-python failed." >&2
        exit 1
    fi
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
