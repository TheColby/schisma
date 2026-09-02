# Architecture

## Product invariant

The note—not the patch—is Schisma's unit of expression. Tuning is represented
as `f64` frequency in Hz and reaches the oscillator and modal body before MPE
bend is applied. Audio samples remain `f32`; long-lived phase, tuning, and
coefficient calculations use `f64`.

The public audio-format contract is two-channel stereo IEEE-754 `f32` at sample
rates from 8 kHz through 384 kHz. Offline WAV output preserves that path as
32-bit float. Live hardware output is also `f32`, subject to the selected
device supporting the requested sample rate.

## Current v0.1 data flow

```text
MIDI bytes / SMF
      ↓
normalized timestamped events
      ↓
MPE channel state + voice binding
      ↓
wavetable exciter → energy-coupled modal body → TPT filter → stereo bus
      ↓
CPAL output or deterministic float WAV
```

The audio callback owns the engine. Realtime MIDI enters through a bounded
single-producer/single-consumer queue. Queue overflow may discard replaceable
expression but must preserve note-off semantics through an emergency
all-notes-off flag.

The desktop runtime also copies a bounded stereo tap into a separate analysis
queue. An analysis worker computes the FFT and meters and may condition that
batch through Metal or CUDA. GPU submission and readback never occur in the
realtime callback; if a requested backend is absent, the worker reports the
reason and uses the CPU reference path.

## Realtime contract

Inside an audio callback Schisma must not allocate, lock, perform I/O, log,
sleep, create or destroy threads, or release the last owner of heap storage.
Buffers and event capacity are prepared before streaming. The audit allocator
tags the audio thread and tests that normal processing performs zero
allocations.

## Dependency direction

Version 0.1 splits parameter metadata, graph validation, analysis, GPU compute,
device hosting, and the desktop app into separate crates. The intended one-way
direction remains:

```text
rt ← core ← params ← tuning / gesture / dsp / modules
                                  ↓
                               graph ← schema
                                  ↓
                               engine
                                  ↓
                audio-io / analysis / gpu / app
```

No UI, device, or file-format type may become part of the DSP contract. The
engine must stay usable by a standalone host, offline renderer, and a future
CLAP host adapter.

## Plan changes

The compiled-graph milestone distinguishes parameter edits, modulation-program
edits, and topology edits. Topology plans are immutable and swapped only at a
block boundary. Retired plans are returned to the control thread for disposal.
Active node state migrates only when stable node ID and type both match.

The v0.1 topology editor serializes and validates this future plan format, but
does not yet replace the fixed realtime engine. That boundary keeps an edited
document from implying realtime behavior the engine has not compiled.
