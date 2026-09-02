use ringbuf::{traits::*, HeapCons, HeapProd, HeapRb};
use schisma_analysis::{AnalysisSnapshot, Analyzer};
use schisma_audio_io::{HardwareConfig, HardwareHost, HardwareStream};
use schisma_engine::rt_audit::audio_allocation_count;
use schisma_engine::{default_twelve_tet_tuning, M0Config, M0Engine, OUTPUT_CHANNELS};
use schisma_gpu::{Accelerator, BackendKind};
use schisma_midi::realtime::{RealtimeMidiHost, RealtimeMidiQueue};
use schisma_midi::{MidiEvent, MidiEventKind, NoteEvent};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const COMMAND_CAPACITY: usize = 4096;
const MIDI_CAPACITY: usize = 8192;
const ANALYSIS_CAPACITY: usize = 32_768;
const ANALYSIS_FFT_SIZE: usize = 4096;

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub device_name: Option<String>,
    pub midi_input: Option<String>,
    pub sample_rate: u32,
    pub block_size: usize,
    pub gpu_backend: BackendKind,
}

#[derive(Debug, Clone)]
pub struct RuntimeSnapshot {
    pub device_name: String,
    pub sample_rate: u32,
    pub block_size: usize,
    pub active_voices: usize,
    pub callback_load: f32,
    pub dropped_commands: u64,
    pub audio_allocations: usize,
    pub voices: [VoiceMeter; 16],
    pub analysis: AnalysisSnapshot,
    pub gpu_requested: BackendKind,
    pub gpu_active: BackendKind,
    pub gpu_device: String,
    pub gpu_detail: String,
    pub midi_input: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VoiceMeter {
    pub active: bool,
    pub note: u8,
    pub pressure: f32,
    pub timbre: f32,
    pub released: bool,
}

enum AudioCommand {
    Midi(MidiEvent),
    SetMorph(f32),
    SetMaster(f32),
    Panic,
}

struct Telemetry {
    active_voices: AtomicUsize,
    callback_load_bits: AtomicU32,
    dropped_commands: AtomicU64,
    voices: [AtomicU32; 16],
}

impl Telemetry {
    fn new() -> Self {
        Self {
            active_voices: AtomicUsize::new(0),
            callback_load_bits: AtomicU32::new(0),
            dropped_commands: AtomicU64::new(0),
            voices: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}

#[derive(Clone)]
struct AnalysisState {
    snapshot: AnalysisSnapshot,
    requested: BackendKind,
    active: BackendKind,
    device: String,
    detail: String,
}

struct AnalysisWorker {
    shared: Arc<Mutex<AnalysisState>>,
    requested: Arc<AtomicU8>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AnalysisWorker {
    fn start(
        mut samples: HeapCons<[f32; 2]>,
        sample_rate: u32,
        initial_backend: BackendKind,
    ) -> Self {
        let shared = Arc::new(Mutex::new(AnalysisState {
            snapshot: AnalysisSnapshot::silence(ANALYSIS_FFT_SIZE / 2 + 1),
            requested: initial_backend,
            active: BackendKind::Cpu,
            device: "Initializing".into(),
            detail: "GPU probe in progress".into(),
        }));
        let requested = Arc::new(AtomicU8::new(backend_to_u8(initial_backend)));
        let stop = Arc::new(AtomicBool::new(false));
        let shared_thread = Arc::clone(&shared);
        let requested_thread = Arc::clone(&requested);
        let stop_thread = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("schisma-analysis".into())
            .spawn(move || {
                let mut current_kind = initial_backend;
                let mut accelerator = Accelerator::new(initial_backend);
                let mut analyzer = Analyzer::new(sample_rate as f32, ANALYSIS_FFT_SIZE);
                let mut window = VecDeque::with_capacity(ANALYSIS_FFT_SIZE);
                publish_gpu_state(&shared_thread, &accelerator);

                while !stop_thread.load(Ordering::Relaxed) {
                    let requested_kind = backend_from_u8(requested_thread.load(Ordering::Relaxed));
                    if requested_kind != current_kind {
                        accelerator = Accelerator::new(requested_kind);
                        current_kind = requested_kind;
                        publish_gpu_state(&shared_thread, &accelerator);
                    }

                    while let Some(frame) = samples.try_pop() {
                        if window.len() == ANALYSIS_FFT_SIZE {
                            window.pop_front();
                        }
                        window.push_back(frame);
                    }

                    if window.len() == ANALYSIS_FFT_SIZE {
                        let source_frames: Vec<_> = window.iter().copied().collect();
                        let mut flat = Vec::with_capacity(ANALYSIS_FFT_SIZE * 2);
                        for frame in &source_frames {
                            flat.extend_from_slice(frame);
                        }
                        let mut conditioned = vec![0.0_f32; flat.len()];
                        let result = accelerator.condition(&flat, &mut conditioned, 1.0, 1.0);
                        let frames: Vec<[f32; 2]> = if result.is_ok() {
                            conditioned.as_chunks::<2>().0.to_vec()
                        } else {
                            source_frames
                        };
                        let snapshot = analyzer.analyze(&frames);
                        if let Ok(mut state) = shared_thread.lock() {
                            state.snapshot = snapshot;
                            if let Err(error) = result {
                                state.detail = error.to_string();
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            })
            .expect("analysis worker thread must start");
        Self {
            shared,
            requested,
            stop,
            thread: Some(thread),
        }
    }

    fn set_backend(&self, backend: BackendKind) {
        self.requested
            .store(backend_to_u8(backend), Ordering::Relaxed);
    }

    fn state(&self) -> AnalysisState {
        self.shared
            .lock()
            .map(|state| state.clone())
            .unwrap_or_else(|_| AnalysisState {
                snapshot: AnalysisSnapshot::silence(ANALYSIS_FFT_SIZE / 2 + 1),
                requested: BackendKind::Cpu,
                active: BackendKind::Cpu,
                device: "Analysis unavailable".into(),
                detail: "analysis state lock was poisoned".into(),
            })
    }
}

impl Drop for AnalysisWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct AudioRuntime {
    stream: Option<HardwareStream>,
    _midi_host: RealtimeMidiHost,
    commands: HeapProd<AudioCommand>,
    telemetry: Arc<Telemetry>,
    analysis: AnalysisWorker,
    device_name: String,
    sample_rate: u32,
    block_size: usize,
    midi_input: String,
}

impl AudioRuntime {
    pub fn start(config: AudioConfig) -> Result<Self, String> {
        let telemetry = Arc::new(Telemetry::new());
        let command_ring = HeapRb::<AudioCommand>::new(COMMAND_CAPACITY);
        let (commands, mut command_consumer) = command_ring.split();
        let analysis_ring = HeapRb::<[f32; 2]>::new(ANALYSIS_CAPACITY);
        let (mut analysis_producer, analysis_consumer) = analysis_ring.split();
        let analysis =
            AnalysisWorker::start(analysis_consumer, config.sample_rate, config.gpu_backend);

        let mut midi_host = RealtimeMidiHost::new();
        let (midi_input, mut midi_queue): (String, Option<RealtimeMidiQueue>) =
            match midi_host.start_ring(config.midi_input.as_deref(), MIDI_CAPACITY) {
                Ok((name, queue)) => (name, Some(queue)),
                Err(error) => (format!("No MIDI input ({error})"), None),
            };

        let mut engine = M0Engine::new(
            M0Config {
                sample_rate: f64::from(config.sample_rate),
                ..M0Config::default()
            },
            default_twelve_tet_tuning(),
        )
        .map_err(|error| error.to_string())?;
        let telemetry_callback = Arc::clone(&telemetry);
        let block_size = config.block_size;
        let sample_rate = config.sample_rate;
        let mut scratch = vec![[0.0_f32; 2]; block_size];
        let mut events = Vec::<MidiEvent>::with_capacity(MIDI_CAPACITY + COMMAND_CAPACITY);
        let hardware = HardwareHost::new(HardwareConfig {
            device_name: config.device_name,
            n_channels: OUTPUT_CHANNELS,
            sample_rate: f64::from(sample_rate),
            block_size,
            ..HardwareConfig::default()
        });
        let stream = hardware
            .open(move |interleaved| {
                let started = Instant::now();
                events.clear();
                while let Some(command) = command_consumer.try_pop() {
                    match command {
                        AudioCommand::Midi(mut event) => {
                            event.frame_offset = 0;
                            if events.len() < events.capacity() {
                                events.push(event);
                            }
                        }
                        AudioCommand::SetMorph(value) => engine.set_base_morph(value),
                        AudioCommand::SetMaster(value) => engine.set_master_gain(value),
                        AudioCommand::Panic => engine.all_notes_off(),
                    }
                }
                if let Some(queue) = &mut midi_queue {
                    while let Some(mut event) = queue.try_pop() {
                        event.frame_offset = 0;
                        if events.len() < events.capacity() {
                            events.push(event);
                        }
                    }
                    if queue.take_emergency_all_notes_off() {
                        engine.all_notes_off();
                    }
                }

                let mut first_chunk = true;
                for chunk in interleaved.chunks_mut(block_size * OUTPUT_CHANNELS) {
                    let frames = chunk.len() / OUTPUT_CHANNELS;
                    if frames == 0 {
                        continue;
                    }
                    let block_events = if first_chunk { events.as_slice() } else { &[] };
                    engine.process_block(block_events, &mut scratch[..frames]);
                    for (target, stereo) in chunk
                        .as_chunks_mut::<OUTPUT_CHANNELS>()
                        .0
                        .iter_mut()
                        .zip(&scratch[..frames])
                    {
                        target[0] = stereo[0];
                        target[1] = stereo[1];
                        let _ = analysis_producer.try_push(*stereo);
                    }
                    first_chunk = false;
                }

                telemetry_callback
                    .active_voices
                    .store(engine.active_voice_count(), Ordering::Relaxed);
                for (index, voice) in engine.voices().iter().enumerate().take(16) {
                    telemetry_callback.voices[index].store(pack_voice(*voice), Ordering::Relaxed);
                }
                let frames = interleaved.len() / OUTPUT_CHANNELS;
                let budget = frames as f32 / sample_rate as f32;
                let load = (started.elapsed().as_secs_f32() / budget.max(1.0e-6)).min(9.99);
                telemetry_callback
                    .callback_load_bits
                    .store(load.to_bits(), Ordering::Relaxed);
            })
            .map_err(|error| error.to_string())?;
        let device_name = stream.device_name().to_owned();

        Ok(Self {
            stream: Some(stream),
            _midi_host: midi_host,
            commands,
            telemetry,
            analysis,
            device_name,
            sample_rate,
            block_size,
            midi_input,
        })
    }

    pub fn set_gpu_backend(&self, backend: BackendKind) {
        self.analysis.set_backend(backend);
    }

    pub fn set_morph(&mut self, value: f32) {
        self.push(AudioCommand::SetMorph(value));
    }

    pub fn set_master(&mut self, value: f32) {
        self.push(AudioCommand::SetMaster(value));
    }

    pub fn note_on(&mut self, channel: u8, note: u8, velocity: f64, pressure: f64, timbre: f64) {
        self.push(AudioCommand::Midi(MidiEvent {
            frame_offset: 0,
            kind: MidiEventKind::ControlChange {
                channel,
                cc: 74,
                value: timbre,
            },
        }));
        self.push(AudioCommand::Midi(MidiEvent {
            frame_offset: 0,
            kind: MidiEventKind::ChannelPressure {
                channel,
                value: pressure,
            },
        }));
        self.push(AudioCommand::Midi(MidiEvent {
            frame_offset: 0,
            kind: MidiEventKind::Note(NoteEvent {
                channel,
                note,
                velocity,
                is_on: true,
            }),
        }));
    }

    pub fn note_off(&mut self, channel: u8, note: u8, release_velocity: f64) {
        self.push(AudioCommand::Midi(MidiEvent {
            frame_offset: 0,
            kind: MidiEventKind::Note(NoteEvent {
                channel,
                note,
                velocity: release_velocity,
                is_on: false,
            }),
        }));
    }

    pub fn panic(&mut self) {
        self.push(AudioCommand::Panic);
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        let analysis = self.analysis.state();
        RuntimeSnapshot {
            device_name: self.device_name.clone(),
            sample_rate: self.sample_rate,
            block_size: self.block_size,
            active_voices: self.telemetry.active_voices.load(Ordering::Relaxed),
            callback_load: f32::from_bits(
                self.telemetry.callback_load_bits.load(Ordering::Relaxed),
            ),
            dropped_commands: self.telemetry.dropped_commands.load(Ordering::Relaxed),
            audio_allocations: audio_allocation_count(),
            voices: std::array::from_fn(|index| {
                unpack_voice(self.telemetry.voices[index].load(Ordering::Relaxed))
            }),
            analysis: analysis.snapshot,
            gpu_requested: analysis.requested,
            gpu_active: analysis.active,
            gpu_device: analysis.device,
            gpu_detail: analysis.detail,
            midi_input: self.midi_input.clone(),
        }
    }

    fn push(&mut self, command: AudioCommand) {
        if self.commands.try_push(command).is_err() {
            self.telemetry
                .dropped_commands
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Drop for AudioRuntime {
    fn drop(&mut self) {
        self.stream.take();
    }
}

fn publish_gpu_state(shared: &Mutex<AnalysisState>, accelerator: &Accelerator) {
    if let Ok(mut state) = shared.lock() {
        state.requested = accelerator.requested();
        state.active = accelerator.active();
        state.device = accelerator.device_name().into();
        state.detail = accelerator
            .fallback_reason()
            .unwrap_or("native compute active")
            .into();
    }
}

fn backend_to_u8(backend: BackendKind) -> u8 {
    match backend {
        BackendKind::Auto => 0,
        BackendKind::Cpu => 1,
        BackendKind::Metal => 2,
        BackendKind::Cuda => 3,
    }
}

fn backend_from_u8(value: u8) -> BackendKind {
    match value {
        1 => BackendKind::Cpu,
        2 => BackendKind::Metal,
        3 => BackendKind::Cuda,
        _ => BackendKind::Auto,
    }
}

fn pack_voice(voice: schisma_engine::VoiceState) -> u32 {
    if !voice.is_active() {
        return 0;
    }
    let pressure = (voice.expression.pressure.clamp(0.0, 1.0) * 255.0).round() as u32;
    let timbre = (voice.expression.timbre.clamp(0.0, 1.0) * 255.0).round() as u32;
    let released = u32::from(voice.phase == schisma_engine::VoicePhase::Released);
    (1 << 31) | u32::from(voice.note) | (pressure << 8) | (timbre << 16) | (released << 24)
}

fn unpack_voice(value: u32) -> VoiceMeter {
    VoiceMeter {
        active: value & (1 << 31) != 0,
        note: (value & 0xff) as u8,
        pressure: ((value >> 8) & 0xff) as f32 / 255.0,
        timbre: ((value >> 16) & 0xff) as f32 / 255.0,
        released: value & (1 << 24) != 0,
    }
}
