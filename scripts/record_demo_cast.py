#!/usr/bin/env python3
"""Record a ~90s paced asciinema of a real warm `make demo` run.

Warm the binary first (`make demo` once). Then:

    python3 scripts/record_demo_cast.py

Output is docs/demo/demo.cast. Content is a live in-memory simulator
run; timing is stretched so the unsafe vs fenced beats are readable.
Does not contact Postgres, Stripe, or any other live system.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CAST = ROOT / "docs/demo/demo.cast"
MARKERS = (
    "1. Unsafe baseline",
    "2. Clared path with the same injected failure",
    "3. Successful simulated settlement",
)


def crlf(text: str) -> str:
    return text.replace("\r\n", "\n").replace("\n", "\r\n")


def split_sections(body: str) -> tuple[str, str, str, str]:
    rest = body
    parts: list[str] = []
    for marker in MARKERS:
        idx = rest.find(marker)
        if idx < 0:
            raise SystemExit(f"demo output missing {marker!r}")
        if idx:
            parts.append(rest[:idx])
        rest = rest[idx:]
    parts.append(rest)
    if len(parts) != 4:
        raise SystemExit("expected preamble plus three sections")
    return parts[0], parts[1], parts[2], parts[3]


def main() -> int:
    proc = subprocess.run(
        ["make", "demo"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        return proc.returncode
    body = proc.stdout if proc.stdout.endswith("\n") else proc.stdout + "\n"
    preamble, s1, s2, s3 = split_sections(body)
    events = [
        [0.4, "o", "$ make demo\r\n"],
        [2.2, "o", "\r\n" + crlf(preamble)],
        [8.0, "o", crlf(s1)],
        [34.0, "o", crlf(s2)],
        [62.0, "o", crlf(s3)],
        [90.0, "o", ""],
    ]
    header = {
        "version": 2,
        "width": 100,
        "height": 30,
        "duration": 90.0,
        "env": {"SHELL": "/bin/zsh", "TERM": "xterm-256color"},
        "title": "Clared unsafe vs fenced (make demo) — in-memory simulator, warm binary",
    }
    CAST.write_text(
        "\n".join(json.dumps(x, ensure_ascii=False) for x in [header, *events]) + "\n",
        encoding="utf-8",
    )
    print(f"wrote {CAST} (90s paced, live make demo output)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
