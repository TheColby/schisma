# Schisma M0 sound examples

Each file is a deterministic four-second, 48 kHz stereo float WAV rendered by
`schisma-render`. The WAV files are intentionally ignored by Git; regenerate
them locally with the commands below.

| File | Study |
| --- | --- |
| `01-glass-strike.wav` | Bright, body-dominant high strike with a long decay |
| `02-velvet-bloom.wav` | Oscillator-forward low note opened by MPE pressure |
| `03-tectonic-bend.wav` | Bass body shaped by a wide per-note pitch gesture |
| `04-prismatic-chord.wav` | Five notes with independent pressure and CC74 color |
| `05-microtonal-orbit.wav` | Cascading 19-EDO notes through Scala/KBM mapping |
| `06-pressure-choir.wav` | Five sustained voices breathing independently |
| `07-release-comet.wav` | Short excitation with a long, expressive release |
| `08-rebind-sparks.wav` | One MPE channel reassigned while tails remain alive |
| `09-stereo-cascade.wav` | Register-based stereo motion across eight notes |
| `10-colossus.wav` | Ten-voice, modal-heavy mass with divergent bends |

For any example other than `microtonal-orbit`:

```sh
cargo run --release -p schisma-engine --bin schisma-render -- \
  --example glass-strike \
  --duration 4 \
  --output examples/audio/01-glass-strike.wav
```

For the 19-EDO example, append:

```sh
--scl examples/tunings/19edo.scl --kbm examples/tunings/19edo.kbm
```

List every built-in study with `schisma-render --list-examples`.
