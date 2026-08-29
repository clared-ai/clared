# Demo recording

`demo.cast` is an [asciinema v2](https://asciinema.org/docs/formats) recording of a real
`make demo` run against the in-memory simulator (no external systems contacted).

Replay it with:

```bash
asciinema play demo.cast
```

Record a fresh one from the repository root with:

```bash
asciinema rec -c "make demo" demo.cast
```

The SHA-256 evidence digest and Ed25519 signature differ on every run by design:
they cover the session's unique nonce, timestamps, and simulated action set.
