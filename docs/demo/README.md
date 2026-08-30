# Demo recording

`demo.cast` is an [asciinema v2](https://asciinema.org/docs/formats) recording of a real
`make demo` run against the in-memory simulator (no external systems contacted).
The binary is warmed first; the cast is paced to ~90 seconds so the three beats
(unsafe baseline, fenced abort, simulated settlement) are readable. Evidence
digest and signature differ on every recapture.

Replay:

```bash
brew install asciinema   # one-time
asciinema play docs/demo/demo.cast
```

Recapture from a warm binary (repository root):

```bash
make demo                         # warm compile; discard this run
python3 scripts/record_demo_cast.py
```

Do not record a cold compile. Do not put Postgres or Stripe in this recording.
