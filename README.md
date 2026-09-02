# Schisma

Schisma is an open-source, controller-first MPE instrument in which every note
behaves like a small physical instrument. Its first vertical slice combines a
wavetable exciter, an energy-coupled modal body, per-note expression, and
microtonal tuning that reaches both the oscillator and resonator.

This repository is intentionally the focused M0 foundation, not a collection
of speculative product surfaces. It already provides:

- allocation-free, 16-voice stereo rendering with an audited audio callback;
- MPE lower/upper zones, master/member expression, channel rebinding during
  release, RPN pitch-bend ranges, and poly-pressure fallback;
- a fixed wavetable → energy morph → 16-mode resonator → TPT low-pass chain;
- per-note pitch, pressure, CC74 timbre, velocity, release velocity, stereo
  position, and release-tail handling;
- Scala `.scl` plus complete `.kbm` mapping, EDO, and rational JI tuning;
- a lock-free realtime MIDI queue with note-off-preserving overflow behavior;
- live CPAL audio/MIDI operation and deterministic offline WAV rendering;
- callback-time telemetry and an audio-thread allocation audit.

## Quick start

Build and test everything:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Render the built-in MPE performance:

```sh
cargo run --release -p schisma-engine --bin schisma-render -- \
  --output /tmp/schisma.wav \
  --duration 8 \
  --stress
```

Render a Standard MIDI File, optionally with Scala tuning:

```sh
cargo run --release -p schisma-engine --bin schisma-render -- \
  --midi performance.mid \
  --scl examples/tunings/19edo.scl \
  --kbm examples/tunings/19edo.kbm \
  --output /tmp/schisma-midi.wav
```

List audio and MIDI devices, then launch the live instrument:

```sh
cargo run -p schisma-engine --bin schisma-live -- --list
cargo run --release -p schisma-engine --bin schisma-live -- \
  --midi "Your MPE Controller"
```

The live binary selects the first MIDI input and the system audio output when
names are omitted. Use `--help` for all options.

## Repository map

- `schisma-engine`: voice lifecycle, MPE binding, fixed M0 DSP chain, render and
  live binaries, realtime audit, and callback telemetry.
- `schisma-midi`: normalized MIDI/MPE events, SMF replay, byte routing, and
  realtime lock-free ingest.
- `schisma-tuning`: Scala/KBM, EDO, JI, and composable frequency transforms.
- `schisma-audio-io`: the small host-agnostic CPAL output adapter.
- `docs`: architecture, product boundary, milestone map, and realtime rules.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before extending the engine.

## Status

M0 engineering scaffold. The next gate is player validation of the morph on
multiple MPE controllers. The graph compiler, gesture language, full-screen
shell, analysis meters, presets, MTS-ESP, and topology canvas remain explicit
later milestones rather than placeholder implementations.

## License

MIT. See [LICENSE](LICENSE).
