# Architecture

## Product invariant

The note—not the patch—is Schisma's unit of expression. Tuning is represented
as `f64` frequency in Hz and reaches the oscillator and modal body before MPE
bend is applied. Audio samples remain `f32`; long-lived phase, tuning, and
coefficient calculations use `f64`.

## Current M0 data flow

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

## Realtime contract

Inside an audio callback Schisma must not allocate, lock, perform I/O, log,
sleep, create or destroy threads, or release the last owner of heap storage.
Buffers and event capacity are prepared before streaming. The audit allocator
tags the audio thread and tests that normal processing performs zero
allocations.

## Planned dependency direction

Future milestones will split neutral realtime primitives, parameter metadata,
gesture compilation, module registration, graph validation, schema, and
analysis from the M0 engine. The intended one-way direction is:

```text
rt ← core ← params ← tuning / gesture / dsp / modules
                                  ↓
                               graph ← schema
                                  ↓
                               engine
                                  ↓
                      audio-io / analysis / app
```

No UI, device, or file-format type may become part of the DSP contract. The
engine must stay usable by a standalone host, offline renderer, and a future
CLAP host adapter.

## Plan changes

The compiled-graph milestone distinguishes parameter edits, modulation-program
edits, and topology edits. Topology plans are immutable and swapped only at a
block boundary. Retired plans are returned to the control thread for disposal.
Active node state migrates only when stable node ID and type both match.

