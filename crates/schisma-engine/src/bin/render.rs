use hound::{SampleFormat, WavSpec, WavWriter};
use schisma_engine::rt_audit::{
    audio_allocation_count, reset_audio_allocation_count, AuditAllocator,
};
use schisma_engine::telemetry::CallbackHistogram;
use schisma_engine::{
    default_twelve_tet_tuning, M0Config, M0Engine, MAX_SUPPORTED_SAMPLE_RATE_HZ,
    MIN_SUPPORTED_SAMPLE_RATE_HZ, OUTPUT_CHANNELS,
};
use schisma_midi::{MidiEvent, MidiEventKind, NoteEvent, OfflineMidiReader, TimedMidiEvent};
use schisma_tuning::ScalaTuning;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[global_allocator]
static GLOBAL_ALLOCATOR: AuditAllocator = AuditAllocator;

const OUTPUT_BITS_PER_SAMPLE: u16 = 32;

#[derive(Debug)]
struct Options {
    output: PathBuf,
    midi: Option<PathBuf>,
    example: Option<String>,
    scl: Option<PathBuf>,
    kbm: Option<PathBuf>,
    duration_seconds: f64,
    sample_rate: u32,
    block_size: usize,
    stress: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            output: PathBuf::from("schisma.wav"),
            midi: None,
            example: None,
            scl: None,
            kbm: None,
            duration_seconds: 5.0,
            sample_rate: 48_000,
            block_size: 128,
            stress: false,
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = parse_options()?;
    let tuning = load_tuning(&options)?;
    let sample_rate = f64::from(options.sample_rate);
    let (timeline, base_morph) = match (&options.midi, &options.example) {
        (Some(path), _) => (
            OfflineMidiReader::from_bytes(&std::fs::read(path)?, sample_rate)?,
            0.5,
        ),
        (None, Some(name)) => short_example_timeline(name, sample_rate)?,
        (None, None) if options.stress => (stress_timeline(sample_rate), 0.5),
        (None, None) => (demo_timeline(sample_rate), 0.5),
    };

    let config = M0Config {
        sample_rate,
        base_morph,
        ..M0Config::default()
    };
    let mut engine = M0Engine::new(config, tuning)?;
    let mut block = vec![[0.0_f32; 2]; options.block_size];
    let callback_budget = Duration::from_secs_f64(options.block_size as f64 / sample_rate);
    let total_frames = (options.duration_seconds * sample_rate).round() as u64;
    let mut timing = CallbackHistogram::new();
    let mut peak = 0.0_f32;
    let mut hash = Sha256::new();

    let spec = WavSpec {
        channels: OUTPUT_CHANNELS as u16,
        sample_rate: options.sample_rate,
        bits_per_sample: OUTPUT_BITS_PER_SAMPLE,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(&options.output, spec)?;
    let mut frame = 0_u64;

    reset_audio_allocation_count();
    while frame < total_frames {
        let frames_this_block =
            usize::try_from((total_frames - frame).min(options.block_size as u64))?;
        let events = timeline.block_events(frame, frames_this_block);
        let started = Instant::now();
        engine.process_block(&events, &mut block[..frames_this_block]);
        timing.observe(started.elapsed(), callback_budget);

        for stereo in &block[..frames_this_block] {
            for sample in stereo {
                peak = peak.max(sample.abs());
                hash.update(sample.to_le_bytes());
                writer.write_sample(*sample)?;
            }
        }
        frame += frames_this_block as u64;
    }
    writer.finalize()?;

    println!("rendered: {}", options.output.display());
    println!("frames: {total_frames} @ {} Hz", options.sample_rate);
    println!("format: {OUTPUT_BITS_PER_SAMPLE}-bit float, {OUTPUT_CHANNELS}-channel stereo");
    println!("sample sha256: {:x}", hash.finalize());
    println!("peak: {:.3} dBFS", 20.0 * peak.max(1.0e-12).log10());
    println!(
        "callback budget: p99 <= {:.1}% max {:.1}% across {} blocks",
        timing.percentile_ratio(0.99) * 100.0,
        timing.max_ratio() * 100.0,
        timing.samples()
    );
    println!("audio-thread allocations: {}", audio_allocation_count());
    Ok(())
}

fn parse_options() -> Result<Options, Box<dyn Error>> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-o" | "--output" => options.output = PathBuf::from(next_value(&mut args, &argument)?),
            "--midi" => options.midi = Some(PathBuf::from(next_value(&mut args, &argument)?)),
            "--example" => options.example = Some(next_value(&mut args, &argument)?),
            "--scl" => options.scl = Some(PathBuf::from(next_value(&mut args, &argument)?)),
            "--kbm" => options.kbm = Some(PathBuf::from(next_value(&mut args, &argument)?)),
            "--duration" => options.duration_seconds = next_value(&mut args, &argument)?.parse()?,
            "--sample-rate" => options.sample_rate = next_value(&mut args, &argument)?.parse()?,
            "--block-size" => options.block_size = next_value(&mut args, &argument)?.parse()?,
            "--stress" => options.stress = true,
            "--list-examples" => {
                print_example_list();
                std::process::exit(0);
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            unknown => return Err(format!("unknown option '{unknown}'").into()),
        }
    }
    if !options.duration_seconds.is_finite() || options.duration_seconds <= 0.0 {
        return Err("duration must be finite and greater than zero".into());
    }
    if !(MIN_SUPPORTED_SAMPLE_RATE_HZ..=MAX_SUPPORTED_SAMPLE_RATE_HZ).contains(&options.sample_rate)
    {
        return Err(format!(
            "sample rate must be between {MIN_SUPPORTED_SAMPLE_RATE_HZ} and \
             {MAX_SUPPORTED_SAMPLE_RATE_HZ} Hz"
        )
        .into());
    }
    if options.block_size == 0 {
        return Err("block size must be greater than zero".into());
    }
    if options.kbm.is_some() && options.scl.is_none() {
        return Err("--kbm requires --scl".into());
    }
    let input_count = usize::from(options.midi.is_some())
        + usize::from(options.example.is_some())
        + usize::from(options.stress);
    if input_count > 1 {
        return Err("choose only one of --midi, --example, or --stress".into());
    }
    Ok(options)
}

fn next_value(
    args: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, Box<dyn Error>> {
    args.next()
        .ok_or_else(|| format!("missing value for {option}").into())
}

fn print_help() {
    println!(
        "schisma-render\n\
         \n\
         Usage: schisma-render [options]\n\
         \n\
           -o, --output PATH       Output float WAV (default: schisma.wav)\n\
               --midi PATH         Optional Standard MIDI File\n\
               --example NAME      Render a built-in expressive sound example\n\
               --list-examples     List built-in sound examples\n\
               --scl PATH          Optional Scala scale\n\
               --kbm PATH          Optional Scala keyboard mapping\n\
               --duration SECONDS  Render duration (default: 5)\n\
               --sample-rate HZ    8000..384000 (default: 48000)\n\
               --block-size FRAMES Callback size (default: 128)\n\
               --stress            Render a sustained 16-voice CPU exercise\n\
         \n\
         Output is always stereo 32-bit IEEE float WAV."
    );
}

fn print_example_list() {
    println!(
        "glass-strike\n\
         velvet-bloom\n\
         tectonic-bend\n\
         prismatic-chord\n\
         microtonal-orbit\n\
         pressure-choir\n\
         release-comet\n\
         rebind-sparks\n\
         stereo-cascade\n\
         colossus"
    );
}

fn load_tuning(options: &Options) -> Result<ScalaTuning, Box<dyn Error>> {
    let Some(scl_path) = &options.scl else {
        return Ok(default_twelve_tet_tuning());
    };
    let scl = std::fs::read_to_string(scl_path)?;
    let kbm = options
        .kbm
        .as_ref()
        .map(std::fs::read_to_string)
        .transpose()?;
    ScalaTuning::from_text(&scl, kbm.as_deref(), 440.0).map_err(Into::into)
}

fn demo_timeline(sample_rate: f64) -> OfflineMidiReader {
    let frame = |seconds: f64| (seconds * sample_rate).round() as u64;
    let mut events = Vec::new();

    for (channel, note, timbre) in [(2_u8, 48_u8, 0.22), (3, 55, 0.52), (4, 64, 0.78)] {
        events.push(timed(
            0,
            MidiEventKind::ControlChange {
                channel,
                cc: 74,
                value: timbre,
            },
        ));
        events.push(timed(
            0,
            MidiEventKind::ChannelPressure {
                channel,
                value: 0.18,
            },
        ));
        events.push(timed(
            frame(0.01),
            MidiEventKind::Note(NoteEvent {
                channel,
                note,
                velocity: 0.78,
                is_on: true,
            }),
        ));
        events.push(timed(
            frame(1.0),
            MidiEventKind::ChannelPressure {
                channel,
                value: 0.82,
            },
        ));
        events.push(timed(
            frame(2.0),
            MidiEventKind::ControlChange {
                channel,
                cc: 74,
                value: (timbre + 0.2_f64).min(1.0_f64),
            },
        ));
        events.push(timed(
            frame(3.2),
            MidiEventKind::Note(NoteEvent {
                channel,
                note,
                velocity: 0.35,
                is_on: false,
            }),
        ));
    }
    OfflineMidiReader::new(events)
}

fn stress_timeline(sample_rate: f64) -> OfflineMidiReader {
    let frame = |seconds: f64| (seconds * sample_rate).round() as u64;
    let mut events = Vec::new();
    for channel in 2_u8..=16 {
        let note = 35 + channel;
        events.push(timed(
            0,
            MidiEventKind::ControlChange {
                channel,
                cc: 74,
                value: 0.45 + f64::from(channel - 2) / 40.0,
            },
        ));
        events.push(timed(
            0,
            MidiEventKind::ChannelPressure {
                channel,
                value: 0.65,
            },
        ));
        events.push(timed(
            frame(0.01),
            MidiEventKind::Note(NoteEvent {
                channel,
                note,
                velocity: 0.65,
                is_on: true,
            }),
        ));
    }

    // Reassign one MPE channel while its previous note is releasing. This
    // exercises all sixteen engine slots while validating detached release state.
    events.push(timed(
        frame(0.5),
        MidiEventKind::Note(NoteEvent {
            channel: 2,
            note: 37,
            velocity: 0.1,
            is_on: false,
        }),
    ));
    events.push(timed(
        frame(0.51),
        MidiEventKind::Note(NoteEvent {
            channel: 2,
            note: 72,
            velocity: 0.65,
            is_on: true,
        }),
    ));
    OfflineMidiReader::new(events)
}

fn short_example_timeline(
    name: &str,
    sample_rate: f64,
) -> Result<(OfflineMidiReader, f32), String> {
    let frame = |seconds: f64| (seconds * sample_rate).round() as u64;
    let mut events = Vec::new();
    let morph = match name {
        "glass-strike" => {
            set_timbre(&mut events, 0, 2, 0.98);
            set_pressure(&mut events, 0, 2, 0.92);
            note(&mut events, frame(0.01), 2, 84, 0.96, true);
            set_pressure(&mut events, frame(0.12), 2, 0.28);
            note(&mut events, frame(0.22), 2, 84, 0.02, false);
            0.96
        }
        "velvet-bloom" => {
            set_timbre(&mut events, 0, 2, 0.10);
            set_pressure(&mut events, 0, 2, 0.04);
            note(&mut events, frame(0.01), 2, 45, 0.82, true);
            for (seconds, pressure) in [(0.4, 0.18), (0.9, 0.42), (1.4, 0.72), (1.9, 0.95)] {
                set_pressure(&mut events, frame(seconds), 2, pressure);
            }
            set_timbre(&mut events, frame(1.25), 2, 0.32);
            note(&mut events, frame(2.35), 2, 45, 0.72, false);
            0.06
        }
        "tectonic-bend" => {
            set_timbre(&mut events, 0, 2, 0.72);
            set_pressure(&mut events, 0, 2, 0.74);
            note(&mut events, frame(0.01), 2, 31, 0.92, true);
            for (seconds, bend) in [
                (0.35, 0.03),
                (0.70, 0.08),
                (1.05, 0.145),
                (1.45, -0.045),
                (1.85, 0.02),
                (2.20, 0.0),
            ] {
                pitch_bend(&mut events, frame(seconds), 2, bend);
            }
            note(&mut events, frame(2.45), 2, 31, 0.22, false);
            0.70
        }
        "prismatic-chord" => {
            for (index, (note_number, timbre)) in
                [(48, 0.10), (55, 0.34), (62, 0.58), (67, 0.78), (74, 0.96)]
                    .into_iter()
                    .enumerate()
            {
                let channel = 2 + index as u8;
                set_timbre(&mut events, 0, channel, timbre);
                set_pressure(&mut events, 0, channel, 0.18 + index as f64 * 0.12);
                note(
                    &mut events,
                    frame(0.04 + index as f64 * 0.035),
                    channel,
                    note_number,
                    0.72,
                    true,
                );
                set_pressure(
                    &mut events,
                    frame(1.15),
                    channel,
                    0.90 - index as f64 * 0.08,
                );
                note(&mut events, frame(2.30), channel, note_number, 0.28, false);
            }
            0.52
        }
        "microtonal-orbit" => {
            for (index, note_number) in [55, 58, 61, 64, 67, 70, 73, 76].into_iter().enumerate() {
                let channel = 2 + index as u8;
                let start = 0.08 + index as f64 * 0.24;
                set_timbre(
                    &mut events,
                    frame(start),
                    channel,
                    0.25 + index as f64 * 0.08,
                );
                set_pressure(&mut events, frame(start), channel, 0.55);
                note(&mut events, frame(start), channel, note_number, 0.66, true);
                note(
                    &mut events,
                    frame(start + 0.72),
                    channel,
                    note_number,
                    0.58,
                    false,
                );
            }
            0.62
        }
        "pressure-choir" => {
            for (index, note_number) in [48, 52, 55, 59, 64].into_iter().enumerate() {
                let channel = 2 + index as u8;
                set_timbre(&mut events, 0, channel, 0.22 + index as f64 * 0.10);
                set_pressure(&mut events, 0, channel, 0.03);
                note(&mut events, frame(0.02), channel, note_number, 0.62, true);
                for (seconds, pressure) in [(0.55, 0.28), (1.15, 0.82), (1.75, 0.35), (2.20, 0.94)]
                {
                    set_pressure(
                        &mut events,
                        frame(seconds + index as f64 * 0.015),
                        channel,
                        (pressure - index as f64 * 0.025).max(0.0),
                    );
                }
                note(&mut events, frame(2.55), channel, note_number, 0.48, false);
            }
            0.38
        }
        "release-comet" => {
            set_timbre(&mut events, 0, 2, 0.88);
            set_pressure(&mut events, 0, 2, 0.78);
            note(&mut events, frame(0.01), 2, 76, 0.98, true);
            pitch_bend(&mut events, frame(0.12), 2, 0.04);
            note(&mut events, frame(0.24), 2, 76, 0.0, false);
            set_timbre(&mut events, frame(0.45), 2, 0.42);
            pitch_bend(&mut events, frame(0.70), 2, -0.035);
            0.90
        }
        "rebind-sparks" => {
            set_timbre(&mut events, 0, 2, 0.92);
            set_pressure(&mut events, 0, 2, 0.70);
            note(&mut events, frame(0.01), 2, 60, 0.86, true);
            note(&mut events, frame(0.38), 2, 60, 0.05, false);
            set_timbre(&mut events, frame(0.42), 2, 0.18);
            set_pressure(&mut events, frame(0.42), 2, 0.40);
            note(&mut events, frame(0.43), 2, 72, 0.80, true);
            pitch_bend(&mut events, frame(0.72), 2, 0.055);
            note(&mut events, frame(1.02), 2, 72, 0.12, false);
            set_timbre(&mut events, frame(1.08), 2, 0.70);
            set_pressure(&mut events, frame(1.08), 2, 0.88);
            note(&mut events, frame(1.09), 2, 79, 0.76, true);
            note(&mut events, frame(1.72), 2, 79, 0.32, false);
            0.74
        }
        "stereo-cascade" => {
            for (index, note_number) in [36, 43, 50, 57, 64, 71, 78, 85].into_iter().enumerate() {
                let channel = 2 + index as u8;
                let start = 0.06 + index as f64 * 0.25;
                set_timbre(
                    &mut events,
                    frame(start),
                    channel,
                    0.15 + index as f64 * 0.11,
                );
                set_pressure(
                    &mut events,
                    frame(start),
                    channel,
                    0.35 + index as f64 * 0.07,
                );
                note(&mut events, frame(start), channel, note_number, 0.74, true);
                note(
                    &mut events,
                    frame(start + 0.48),
                    channel,
                    note_number,
                    0.36,
                    false,
                );
            }
            0.58
        }
        "colossus" => {
            for (index, note_number) in [29, 36, 41, 48, 53, 60, 65, 72, 77, 84]
                .into_iter()
                .enumerate()
            {
                let channel = 2 + index as u8;
                set_timbre(&mut events, 0, channel, 0.34 + (index % 4) as f64 * 0.18);
                set_pressure(&mut events, 0, channel, 0.52 + (index % 3) as f64 * 0.13);
                note(
                    &mut events,
                    frame(0.02 + index as f64 * 0.018),
                    channel,
                    note_number,
                    0.72,
                    true,
                );
                pitch_bend(
                    &mut events,
                    frame(1.10 + index as f64 * 0.012),
                    channel,
                    (index as f64 - 4.5) * 0.004,
                );
                set_pressure(&mut events, frame(1.55), channel, 0.92);
                note(&mut events, frame(2.35), channel, note_number, 0.18, false);
            }
            0.82
        }
        unknown => {
            return Err(format!(
                "unknown example '{unknown}'; use --list-examples to see valid names"
            ));
        }
    };

    Ok((OfflineMidiReader::new(events), morph))
}

fn note(
    events: &mut Vec<TimedMidiEvent>,
    frame: u64,
    channel: u8,
    note: u8,
    velocity: f64,
    is_on: bool,
) {
    events.push(timed(
        frame,
        MidiEventKind::Note(NoteEvent {
            channel,
            note,
            velocity,
            is_on,
        }),
    ));
}

fn set_pressure(events: &mut Vec<TimedMidiEvent>, frame: u64, channel: u8, value: f64) {
    events.push(timed(
        frame,
        MidiEventKind::ChannelPressure { channel, value },
    ));
}

fn set_timbre(events: &mut Vec<TimedMidiEvent>, frame: u64, channel: u8, value: f64) {
    events.push(timed(
        frame,
        MidiEventKind::ControlChange {
            channel,
            cc: 74,
            value,
        },
    ));
}

fn pitch_bend(events: &mut Vec<TimedMidiEvent>, frame: u64, channel: u8, value: f64) {
    events.push(timed(frame, MidiEventKind::PitchBend { channel, value }));
}

fn timed(frame: u64, kind: MidiEventKind) -> TimedMidiEvent {
    TimedMidiEvent {
        frame,
        event: MidiEvent {
            frame_offset: 0,
            kind,
        },
    }
}
