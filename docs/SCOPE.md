# Scope and milestones

## Thesis

Schisma is an MPE physical-modeling instrument whose per-note gesture response,
body, and tuning are programmable. Everything else must strengthen that idea.

## M0 — present scaffold

- fixed per-voice wavetable/modal/filter topology;
- complete MPE voice/channel semantics and poly-pressure fallback;
- Scala/KBM, EDO, and JI tuning;
- live and offline hosts;
- deterministic same-build rendering, timing histogram, and allocation audit.

Exit gate: the morph earns strong player feedback and 16 demanding voices fit
comfortably inside a 128-frame callback on reference hardware.

## M1 — engine core

Parameter registry, module registry, constrained graph document, compiled
immutable plan, stable IDs, voice-state migration, sub-block modulation, and a
flat per-note gesture program.

## M2 — tuning and controllers

Controller profiles and reconnect behavior, MTS-ESP client precedence and
fallback, click-free continuous retuning, and tuning-parser fuzzing.

## M3 — instrument

Dual wavetables, excitation choices, finalized modal body, per-note drive and
position, global EQ/reverb/limiter, presets, and factory content.

## M4 — professional shell

Multi-resolution full-screen UI, performance macros, preset browser, note
activity monitor, device management, and spectrum/peak/true-peak/stereo
correlation analysis on a non-audio thread.

## M5 — topology editor

A constrained graphical editor for rearranging and inserting typed modules,
with undo, groups, feedback validation, CPU ×voice cost badges, and explicit
per-voice/global boundaries. Modulation remains a parameter relationship, not
an audio cable.

## Deliberately later

CLAP integration is the first host expansion. Free-form modular construction,
WASM modules, voice coupling, advanced synthesis families, MIDI 2.0, surround,
and distributed rendering do not belong in the initial instrument.

