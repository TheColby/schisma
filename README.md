# Schisma

Schisma is an open-source, standalone, controller-first MPE instrument in which
every note behaves like a small physical instrument. Version 0.1 combines a
wavetable exciter, an energy-coupled modal body, per-note expression,
microtonal tuning, a constrained topology editor, professional metering, and
native GPU batch processing in one scalable desktop interface.

The current vertical slice provides:

- allocation-free, 16-voice stereo rendering with an audited audio callback;
- stereo 32-bit float synthesis and WAV output from 8 kHz through 384 kHz;
- MPE lower/upper zones, master/member expression, channel rebinding during
  release, RPN pitch-bend ranges, and poly-pressure fallback;
- a fixed wavetable → energy morph → 16-mode resonator → TPT low-pass chain;
- per-note pitch, pressure, CC74 timbre, velocity, release velocity, stereo
  position, release-tail handling, and a pronounced key-specific morph contour;
- Scala `.scl` plus complete `.kbm` mapping, EDO, and rational JI tuning;
- a lock-free realtime MIDI queue with note-off-preserving overflow behavior;
- live CPAL audio/MIDI operation and deterministic offline WAV rendering;
- callback-time telemetry and an audio-thread allocation audit;
- a resizable dark desktop shell with global performance controls, audio/GPU
  settings, a 16-voice activity monitor, and an on-screen performance surface;
- a typed graphical topology document with draggable modules, validated cables,
  per-voice/global boundaries, stable node IDs, JSON export, and cycle checks;
- a colossal live 3D spectral-waterfall stage plus a square Lissajous M/S
  vectorscope, dual-channel time-domain oscilloscope, three-second phase
  correlometer, stereo width/balance/orbit/anti-phase/crest diagnostics,
  peak/RMS dBFS, and momentary loudness on a worker thread;
- real Metal and CUDA compute kernels with runtime discovery, self-tests, and
  explicit CPU fallback. Metal is native on macOS; CUDA targets NVIDIA systems.

## Install

### Homebrew (macOS)

Until the first tagged binary release, the formula builds the latest `main`
branch from source:

```sh
brew tap TheColby/schisma https://github.com/TheColby/schisma
brew install --HEAD TheColby/schisma/schisma
schisma
```

The formula installs the standalone interface plus `schisma-render`,
`schisma-live`, and `schisma-gpu-info`. Refresh a head build with
`brew upgrade --fetch-HEAD TheColby/schisma/schisma`.

### Debian and Ubuntu (`apt-get`)

Download the `schisma-debian` artifact from the latest
[Packages workflow](https://github.com/TheColby/schisma/actions/workflows/packages.yml),
unzip it, and install the native package with dependency resolution:

```sh
sudo apt-get update
sudo apt-get install ./schisma_0.1.0_amd64.deb
schisma
```

To build the `.deb` locally, first install Rust 1.92 or newer, then run:

```sh
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config dpkg-dev \
  libasound2-dev libudev-dev libx11-dev libxi-dev libgl1-mesa-dev \
  libwayland-dev libxkbcommon-dev libvulkan-dev
./scripts/package_debian.sh
sudo apt-get install ./dist/schisma_0.1.0_$(dpkg --print-architecture).deb
```

The package adds Schisma to the desktop application menu and installs all four
binaries in `/usr/bin`. More detail is available in
[docs/INSTALL.md](docs/INSTALL.md).

## Quick start

Build and test everything:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Launch the standalone interface:

```sh
cargo run --release -p schisma-app --bin schisma
```

Schisma starts the system audio output and first available MIDI input. The
on-screen keys work without a controller; MPE input adds independent pitch,
pressure, and CC74 timbre per note. Audio and MIDI/MPE devices can both be
selected in the Audio & GPU settings window.

The default instrument and all ten built-in sound studies start with a 46%
key-morph spread. An asymmetric pitch-class contour gives adjacent keys
different material character, while register adds a slower large-scale shift;
velocity, pressure, pitch bend, and CC74 remain fully per-note and expressive.
The engine exposes the spread independently, including zero for the original
global-morph response.

On macOS, build a standalone app bundle with:

```sh
./scripts/package_macos.sh
open dist/Schisma.app
```

Render the built-in MPE performance:

```sh
cargo run --release -p schisma-engine --bin schisma-render -- \
  --output /tmp/schisma.wav \
  --duration 8 \
  --stress
```

Render at the maximum supported resolution:

```sh
cargo run --release -p schisma-engine --bin schisma-render -- \
  --example colossus \
  --sample-rate 384000 \
  --duration 2 \
  --output /tmp/schisma-384k-f32.wav
```

Live output uses the same stereo `f32` signal path. The selected hardware
device must advertise the requested rate; offline rendering supports the full
8–384 kHz range independently of audio hardware.

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

Inspect GPU availability and run a compute self-test:

```sh
cargo run -p schisma-gpu --bin schisma-gpu-info
```

The default build includes Metal plus CUDA 12 runtime support. CUDA 11 and 13
build variants and runtime requirements are documented in
[docs/GPU.md](docs/GPU.md).

## Repository map

- `schisma-app`: standalone resizable GUI, performance surface, topology canvas,
  runtime controls, settings, and metering.
- `schisma-engine`: voice lifecycle, MPE binding, fixed v0.1 DSP chain, render and
  live binaries, realtime audit, and callback telemetry.
- `schisma-analysis`: FFT spectrum, peak/RMS dBFS, loudness, and correlation.
- `schisma-gpu`: Metal, CUDA, and CPU batch-processing backends.
- `schisma-graph`: serializable typed graph document and structural validation.
- `schisma-params`: stable parameter IDs, metadata, normalization, and registry.
- `schisma-midi`: normalized MIDI/MPE events, SMF replay, byte routing, and
  realtime lock-free ingest.
- `schisma-tuning`: Scala/KBM, EDO, JI, and composable frequency transforms.
- `schisma-audio-io`: the small host-agnostic CPAL output adapter.
- `docs`: architecture, GPU configuration, scope, and realtime rules.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before extending the engine.

## v0.1 boundary

The standalone instrument, graph editor, analysis system, and GPU batch paths
are implemented. The editable graph is a validated patch document in v0.1;
audio still runs through the proven fixed wavetable → morph → modal → filter
voice topology. Compiling arbitrary edited graphs into realtime DSP, full
BS.1770 gating/true-peak metering, presets, undo, and MTS-ESP remain subsequent
milestones. See [docs/SCOPE.md](docs/SCOPE.md) for the exact boundary.

## License

MIT. See [LICENSE](LICENSE).
