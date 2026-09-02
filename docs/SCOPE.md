# Scope and milestones

## Thesis

Schisma is an MPE physical-modeling instrument whose per-note gesture response,
body, and tuning are programmable. Everything else must strengthen that idea.

## v0.1 — implemented vertical slice

- fixed per-voice wavetable/modal/filter topology;
- complete MPE voice/channel semantics and poly-pressure fallback;
- Scala/KBM, EDO, and JI tuning;
- live and offline hosts;
- deterministic same-build rendering, timing histogram, and allocation audit;
- standalone multi-resolution GUI with global controls, selectable audio/MPE
  devices, and a performance surface;
- typed topology-document editor with structural validation and JSON export;
- a 64-frame perspective spectral waterfall, spectrum, stereo peak/RMS dBFS,
  loudness, and correlation analysis;
- native Metal and CUDA batch kernels with discovery, self-test, and CPU fallback;
- stereo IEEE-754 `f32` processing and float WAV output at 8–384 kHz.

The graph editor is intentionally document-level in v0.1. Its edits are not yet
compiled into the fixed realtime DSP chain. The loudness meter is a useful
momentary estimate, not yet a fully gated BS.1770/EBU R128 implementation.

## v0.2 — compiled engine core

Parameter registry, module registry, constrained graph document, compiled
immutable plan, stable IDs, voice-state migration, sub-block modulation, and a
flat per-note gesture program.

## v0.3 — tuning and controllers

Controller profiles and reconnect behavior, MTS-ESP client precedence and
fallback, click-free continuous retuning, and tuning-parser fuzzing.

## v0.4 — instrument

Dual wavetables, excitation choices, finalized modal body, per-note drive and
position, global EQ/reverb/limiter, presets, and factory content.

## v0.5 — production shell

Preset browser, undo/redo, accessibility polish, full BS.1770/EBU R128 gating,
true-peak oversampling, controller mapping, and factory content.

## v0.6 — topology execution

Compile the constrained graphical document into immutable realtime plans, add
state migration, groups, modulation relationships, and calibrated CPU/GPU cost
badges. Modulation remains a parameter relationship, not an audio cable.

## Deliberately later

CLAP integration is the first host expansion. Free-form modular construction,
WASM modules, voice coupling, advanced synthesis families, MIDI 2.0, surround,
and distributed rendering do not belong in the initial instrument.
