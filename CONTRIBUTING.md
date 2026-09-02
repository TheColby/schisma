# Contributing

Schisma is in an evidence-gathering M0 phase. Changes should improve the
per-note physical instrument, MPE correctness, tuning correctness, realtime
safety, tests, or reproducible measurement.

Before submitting a change, run:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Audio-thread allocation, blocking synchronization, file or console I/O, and
unbounded work are non-negotiable review failures. New synthesis families,
extension hosts, distributed execution, and unrelated UI infrastructure are
out of scope until their milestone begins.

Public parameter IDs and future node IDs are permanent once a released preset
uses them. Parser and voice-lifecycle changes require regression tests.

