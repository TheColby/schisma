use crate::audio::{AudioConfig, AudioRuntime, RuntimeSnapshot};
use crate::theme;
use eframe::egui::{self, Align, Color32, FontId, Id, Layout, Pos2, Rect, Sense, Stroke, Vec2};
use schisma_analysis::AnalysisSnapshot;
use schisma_audio_io::HardwareHost;
use schisma_gpu::{BackendKind, BackendStatus};
use schisma_graph::{
    default_instrument_graph, Connection, GraphDocument, GraphNode, NodeId, NodeKind, NodeScope,
    PortKind,
};
use schisma_midi::realtime::RealtimeMidiHost;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

const VERSION: &str = "v0.1.0";
const WATERFALL_BINS: usize = 112;
const WATERFALL_FRAMES: usize = 64;
const WATERFALL_CAPTURE_INTERVAL: Duration = Duration::from_millis(42);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainView {
    Perform,
    Topology,
}

pub struct SchismaApp {
    runtime: Option<AudioRuntime>,
    runtime_error: Option<String>,
    last_snapshot: Option<RuntimeSnapshot>,
    silence: AnalysisSnapshot,
    graph: GraphDocument,
    selected_node: Option<NodeId>,
    pending_connection: Option<NodeId>,
    waterfall_history: VecDeque<Vec<f32>>,
    last_waterfall_capture: Instant,
    canvas_pan: Vec2,
    canvas_zoom: f32,
    main_view: MainView,
    morph: f32,
    master: f32,
    pressure: f32,
    timbre: f32,
    sample_rate: u32,
    block_size: usize,
    devices: Vec<String>,
    selected_device: Option<String>,
    midi_devices: Vec<String>,
    selected_midi: Option<String>,
    gpu_backend: BackendKind,
    gpu_statuses: Vec<BackendStatus>,
    held_notes: BTreeMap<u8, u8>,
    show_about: bool,
    show_settings: bool,
    status_message: String,
}

impl SchismaApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        theme::install(&context.egui_ctx);
        let devices = HardwareHost::list_devices().unwrap_or_default();
        let midi_devices = RealtimeMidiHost::list_input_ports().unwrap_or_default();
        let gpu_statuses = schisma_gpu::discover();
        let sample_rate = 48_000;
        let block_size = 128;
        let gpu_backend = BackendKind::Auto;
        let mut app = Self {
            runtime: None,
            runtime_error: None,
            last_snapshot: None,
            silence: AnalysisSnapshot::silence(2049),
            graph: default_instrument_graph(),
            selected_node: None,
            pending_connection: None,
            waterfall_history: VecDeque::with_capacity(WATERFALL_FRAMES),
            last_waterfall_capture: Instant::now() - WATERFALL_CAPTURE_INTERVAL,
            canvas_pan: egui::vec2(30.0, 70.0),
            canvas_zoom: 0.82,
            main_view: MainView::Perform,
            morph: 0.5,
            master: 1.0,
            pressure: 0.65,
            timbre: 0.55,
            sample_rate,
            block_size,
            devices,
            selected_device: None,
            midi_devices,
            selected_midi: None,
            gpu_backend,
            gpu_statuses,
            held_notes: BTreeMap::new(),
            show_about: false,
            show_settings: false,
            status_message: "Initializing audio engine".into(),
        };
        app.restart_audio();
        app
    }

    fn restart_audio(&mut self) {
        self.release_all_notes();
        self.runtime.take();
        let config = AudioConfig {
            device_name: self.selected_device.clone(),
            midi_input: self.selected_midi.clone(),
            sample_rate: self.sample_rate,
            block_size: self.block_size,
            gpu_backend: self.gpu_backend,
        };
        match AudioRuntime::start(config) {
            Ok(mut runtime) => {
                runtime.set_morph(self.morph);
                runtime.set_master(self.master);
                self.runtime_error = None;
                self.status_message = "Audio engine online".into();
                self.runtime = Some(runtime);
            }
            Err(error) => {
                self.status_message = "Audio engine offline".into();
                self.runtime_error = Some(error);
            }
        }
    }

    fn release_all_notes(&mut self) {
        if let Some(runtime) = &mut self.runtime {
            for (note, channel) in std::mem::take(&mut self.held_notes) {
                runtime.note_off(channel, note, 0.4);
            }
            runtime.panic();
        } else {
            self.held_notes.clear();
        }
    }

    fn refresh_snapshot(&mut self) {
        if let Some(runtime) = &self.runtime {
            let snapshot = runtime.snapshot();
            if self.last_waterfall_capture.elapsed() >= WATERFALL_CAPTURE_INTERVAL {
                self.waterfall_history
                    .push_back(resample_spectrum(&snapshot.analysis.spectrum_db));
                while self.waterfall_history.len() > WATERFALL_FRAMES {
                    self.waterfall_history.pop_front();
                }
                self.last_waterfall_capture = Instant::now();
            }
            self.last_snapshot = Some(snapshot);
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu_bar")
            .exact_size(38.0)
            .frame(egui::Frame::new().fill(theme::BG))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("SCHISMA")
                            .strong()
                            .size(16.0)
                            .color(theme::TEXT),
                    );
                    ui.label(egui::RichText::new(VERSION).small().color(theme::MUTED));
                    ui.separator();
                    egui::MenuBar::new().ui(ui, |ui| {
                        ui.menu_button("File", |ui| {
                            if ui.button("New topology").clicked() {
                                self.graph = default_instrument_graph();
                                self.selected_node = None;
                                self.status_message = "Default topology restored".into();
                                ui.close();
                            }
                            if ui.button("Save patch JSON").clicked() {
                                match serde_json::to_string_pretty(&self.graph).and_then(|json| {
                                    std::fs::write("schisma-patch.json", json)
                                        .map_err(serde_json::Error::io)
                                }) {
                                    Ok(()) => {
                                        self.status_message = "Saved schisma-patch.json".into()
                                    }
                                    Err(error) => self.status_message = error.to_string(),
                                }
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Quit").clicked() {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                        ui.menu_button("Engine", |ui| {
                            if ui.button("Restart audio").clicked() {
                                self.restart_audio();
                                ui.close();
                            }
                            if ui.button("All notes off").clicked() {
                                self.release_all_notes();
                                ui.close();
                            }
                        });
                        ui.menu_button("View", |ui| {
                            ui.selectable_value(&mut self.main_view, MainView::Perform, "Perform");
                            ui.selectable_value(
                                &mut self.main_view,
                                MainView::Topology,
                                "Topology",
                            );
                            ui.checkbox(&mut self.show_settings, "Audio & GPU settings");
                        });
                        ui.menu_button("Help", |ui| {
                            if ui.button("About Schisma").clicked() {
                                self.show_about = true;
                                ui.close();
                            }
                        });
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(8.0);
                        let (label, color) = if self.runtime.is_some() {
                            ("ENGINE ONLINE", theme::CYAN)
                        } else {
                            ("ENGINE OFFLINE", theme::RED)
                        };
                        status_pill(ui, label, color);
                        if let Some(snapshot) = &self.last_snapshot {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} voices  ·  {:.1}% DSP",
                                    snapshot.active_voices,
                                    snapshot.callback_load * 100.0
                                ))
                                .monospace()
                                .color(theme::MUTED),
                            );
                        }
                    });
                });
            });
    }

    fn global_strip(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("global_strip")
            .exact_size(128.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(14, 10)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("PER-NOTE PHYSICAL INSTRUMENT")
                                    .small()
                                    .strong()
                                    .color(theme::VIOLET),
                            );
                            ui.label(
                                egui::RichText::new("Each gesture reshapes the body")
                                    .size(18.0)
                                    .color(theme::TEXT),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if tab_button(ui, "PERFORM", self.main_view == MainView::Perform)
                                    .clicked()
                                {
                                    self.main_view = MainView::Perform;
                                }
                                if tab_button(ui, "TOPOLOGY", self.main_view == MainView::Topology)
                                    .clicked()
                                {
                                    self.main_view = MainView::Topology;
                                }
                            });
                        });
                        ui.separator();
                        let morph_changed = knob(ui, "MORPH", &mut self.morph, theme::VIOLET);
                        let master_changed = knob(ui, "MASTER", &mut self.master, theme::CYAN);
                        knob(ui, "PRESSURE", &mut self.pressure, theme::AMBER);
                        knob(ui, "TIMBRE", &mut self.timbre, theme::VIOLET);
                        if morph_changed {
                            if let Some(runtime) = &mut self.runtime {
                                runtime.set_morph(self.morph);
                            }
                        }
                        if master_changed {
                            if let Some(runtime) = &mut self.runtime {
                                runtime.set_master(self.master);
                            }
                        }
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("COMPUTE").small().color(theme::MUTED));
                            egui::ComboBox::from_id_salt("gpu_backend")
                                .selected_text(self.gpu_backend.label())
                                .width(120.0)
                                .show_ui(ui, |ui| {
                                    for backend in BackendKind::ALL {
                                        ui.selectable_value(
                                            &mut self.gpu_backend,
                                            backend,
                                            backend.label(),
                                        );
                                    }
                                });
                            if ui.button("Apply GPU").clicked() {
                                if let Some(runtime) = &self.runtime {
                                    runtime.set_gpu_backend(self.gpu_backend);
                                }
                                self.status_message =
                                    format!("Requested {} compute", self.gpu_backend.label());
                            }
                            if let Some(snapshot) = &self.last_snapshot {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} · {}",
                                        snapshot.gpu_active.label(),
                                        snapshot.gpu_device
                                    ))
                                    .small()
                                    .color(theme::CYAN),
                                );
                            }
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("AUDIO").small().color(theme::MUTED));
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} Hz / {}f",
                                    self.sample_rate, self.block_size
                                ))
                                .monospace(),
                            );
                            if ui.button("Configure").clicked() {
                                self.show_settings = true;
                            }
                            if ui
                                .add(
                                    egui::Button::new("PANIC").fill(theme::RED.gamma_multiply(0.7)),
                                )
                                .clicked()
                            {
                                self.release_all_notes();
                            }
                        });
                    });
                });
            });
    }

    fn left_palette(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("module_palette")
            .resizable(true)
            .default_size(210.0)
            .size_range(170.0..=300.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("MODULES")
                        .strong()
                        .small()
                        .color(theme::MUTED),
                );
                ui.add_space(6.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    module_group(
                        ui,
                        "SOURCES",
                        &[NodeKind::Wavetable, NodeKind::Exciter],
                        self,
                    );
                    module_group(ui, "BODY", &[NodeKind::Morph, NodeKind::ModalBody], self);
                    module_group(
                        ui,
                        "PER-NOTE FX",
                        &[NodeKind::Filter, NodeKind::Drive, NodeKind::Delay],
                        self,
                    );
                    module_group(
                        ui,
                        "GLOBAL",
                        &[
                            NodeKind::Reverb,
                            NodeKind::Equalizer,
                            NodeKind::Limiter,
                            NodeKind::Output,
                        ],
                        self,
                    );
                });
            });
    }

    fn right_inspector(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(270.0)
            .size_range(220.0..=360.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("INSPECTOR")
                        .strong()
                        .small()
                        .color(theme::MUTED),
                );
                ui.add_space(8.0);
                let selected = self
                    .selected_node
                    .and_then(|id| self.graph.nodes.iter_mut().find(|node| node.id == id));
                if let Some(node) = selected {
                    ui.heading(&node.name);
                    ui.label(
                        egui::RichText::new(format!("{:?} · {:?}", node.kind, node.scope))
                            .small()
                            .color(theme::VIOLET),
                    );
                    ui.separator();
                    ui.label("Display name");
                    ui.text_edit_singleline(&mut node.name);
                    ui.label("Canvas position");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut node.position[0]).speed(1.0));
                        ui.add(egui::DragValue::new(&mut node.position[1]).speed(1.0));
                    });
                    ui.separator();
                    ui.label(egui::RichText::new("COST").small().color(theme::MUTED));
                    let multiplier = if node.scope == NodeScope::PerVoice {
                        "×16 voices"
                    } else {
                        "×1 global"
                    };
                    ui.label(
                        egui::RichText::new(multiplier)
                            .monospace()
                            .color(theme::AMBER),
                    );
                    ui.add_space(12.0);
                    if ui
                        .add(
                            egui::Button::new("Delete module").fill(theme::RED.gamma_multiply(0.3)),
                        )
                        .clicked()
                    {
                        let id = node.id;
                        self.graph.nodes.retain(|candidate| candidate.id != id);
                        self.graph
                            .connections
                            .retain(|edge| edge.from_node != id && edge.to_node != id);
                        self.selected_node = None;
                    }
                } else {
                    ui.heading(&self.graph.name);
                    ui.label(format!(
                        "{} modules · {} cables",
                        self.graph.nodes.len(),
                        self.graph.connections.len()
                    ));
                    ui.separator();
                    if let Some(snapshot) = &self.last_snapshot {
                        labeled_value(ui, "Audio", &snapshot.device_name);
                        labeled_value(ui, "MIDI", &snapshot.midi_input);
                        labeled_value(
                            ui,
                            "Format",
                            &format!(
                                "{} Hz · {}f · stereo f32",
                                snapshot.sample_rate, snapshot.block_size
                            ),
                        );
                        labeled_value(
                            ui,
                            "GPU",
                            &format!(
                                "{} > {} / {}",
                                snapshot.gpu_requested.label(),
                                snapshot.gpu_active.label(),
                                snapshot.gpu_device
                            ),
                        );
                        labeled_value(
                            ui,
                            "RT audit",
                            &format!("{} callback allocations", snapshot.audio_allocations),
                        );
                        labeled_value(
                            ui,
                            "Queue",
                            &format!("{} dropped commands", snapshot.dropped_commands),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(&snapshot.gpu_detail)
                                .small()
                                .color(theme::MUTED),
                        );
                    }
                    if let Some(error) = &self.runtime_error {
                        ui.separator();
                        ui.label(egui::RichText::new(error).color(theme::RED));
                    }
                }
            });
    }

    fn center(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG))
            .show(ui, |ui| match self.main_view {
                MainView::Perform => self.performance_view(ui),
                MainView::Topology => self.topology_view(ui),
            });
    }

    fn performance_view(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        ui.allocate_ui_with_layout(available, Layout::top_down(Align::Center), |ui| {
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new("THE NOTE IS THE INSTRUMENT")
                    .size(12.0)
                    .strong()
                    .color(theme::VIOLET),
            );
            ui.label(
                egui::RichText::new("Wavetable energy becomes a resonant body")
                    .size(24.0)
                    .color(theme::TEXT),
            );
            ui.add_space(12.0);
            let reserved_below = if self.last_snapshot.is_some() {
                184.0
            } else {
                136.0
            };
            let stage_height = (ui.available_height() - reserved_below).max(120.0);
            let (waterfall_rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), stage_height),
                Sense::hover(),
            );
            waterfall_spectral_background(ui, waterfall_rect, &self.waterfall_history);
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("CLICK KEYS TO LATCH · MPE INPUT REMAINS LIVE")
                    .small()
                    .color(theme::MUTED),
            );
            self.keyboard(ui);
            ui.add_space(14.0);
            if let Some(snapshot) = &self.last_snapshot {
                voice_monitor(ui, snapshot);
            }
        });
    }

    fn keyboard(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::horizontal()
            .id_salt("performance_keyboard")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for note in 48_u8..=83 {
                        let black = matches!(note % 12, 1 | 3 | 6 | 8 | 10);
                        let active = self.held_notes.contains_key(&note);
                        let fill = if active {
                            if black {
                                theme::VIOLET
                            } else {
                                theme::CYAN
                            }
                        } else if black {
                            Color32::from_rgb(24, 26, 32)
                        } else {
                            Color32::from_rgb(204, 211, 220)
                        };
                        let text = if black { theme::TEXT } else { theme::BG };
                        let height = if black { 72.0 } else { 102.0 };
                        let response = ui.add_sized(
                            [28.0, height],
                            egui::Button::new(
                                egui::RichText::new(note_name(note)).small().color(text),
                            )
                            .fill(fill),
                        );
                        if response.clicked() {
                            if let Some(channel) = self.held_notes.remove(&note) {
                                if let Some(runtime) = &mut self.runtime {
                                    runtime.note_off(channel, note, 0.35);
                                }
                            } else {
                                let channel = 2 + (self.held_notes.len() as u8 % 15);
                                self.held_notes.insert(note, channel);
                                if let Some(runtime) = &mut self.runtime {
                                    runtime.note_on(
                                        channel,
                                        note,
                                        0.78,
                                        f64::from(self.pressure),
                                        f64::from(self.timbre),
                                    );
                                }
                            }
                        }
                    }
                });
            });
    }

    fn topology_view(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::BG);

        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.canvas_zoom = (self.canvas_zoom * (1.0 + scroll * 0.0015)).clamp(0.35, 1.8);
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            self.canvas_pan += ui.input(|input| input.pointer.delta());
        }
        draw_grid(&painter, rect, self.canvas_pan, self.canvas_zoom);

        let mut node_rects = BTreeMap::new();
        for node in &self.graph.nodes {
            node_rects.insert(node.id, self.node_rect(rect, node));
        }
        for edge in &self.graph.connections {
            if let (Some(source), Some(target)) = (
                node_rects.get(&edge.from_node),
                node_rects.get(&edge.to_node),
            ) {
                draw_cable(
                    &painter,
                    egui::pos2(source.right(), source.center().y),
                    egui::pos2(target.left(), target.center().y),
                    theme::CYAN.gamma_multiply(0.72),
                );
            }
        }

        let mut clicked_output = None;
        let mut clicked_input = None;
        let pointer_delta = ui.input(|input| input.pointer.delta());
        for node in &mut self.graph.nodes {
            let node_rect = node_rects[&node.id];
            let id = Id::new(("graph_node", node.id.0));
            let node_response = ui.interact(node_rect, id, Sense::click_and_drag());
            if node_response.clicked() {
                self.selected_node = Some(node.id);
            }
            if node_response.dragged() {
                node.position[0] += pointer_delta.x / self.canvas_zoom;
                node.position[1] += pointer_delta.y / self.canvas_zoom;
            }
            draw_node(
                &painter,
                node,
                node_rect,
                self.selected_node == Some(node.id),
            );

            if !node.inputs.is_empty() {
                let port = Rect::from_center_size(
                    egui::pos2(node_rect.left(), node_rect.center().y),
                    egui::vec2(18.0, 18.0),
                );
                if ui
                    .interact(port, Id::new(("input", node.id.0)), Sense::click())
                    .clicked()
                {
                    clicked_input = Some(node.id);
                }
            }
            if !node.outputs.is_empty() {
                let port = Rect::from_center_size(
                    egui::pos2(node_rect.right(), node_rect.center().y),
                    egui::vec2(18.0, 18.0),
                );
                if ui
                    .interact(port, Id::new(("output", node.id.0)), Sense::click())
                    .clicked()
                {
                    clicked_output = Some(node.id);
                }
            }
        }
        if let Some(source) = clicked_output {
            self.pending_connection = Some(source);
            self.status_message = "Select a destination input".into();
        }
        if let (Some(source), Some(target)) = (self.pending_connection, clicked_input) {
            self.connect(source, target);
            self.pending_connection = None;
        }
        if let Some(source) = self.pending_connection {
            if let (Some(source_rect), Some(pointer)) = (
                node_rects.get(&source),
                ui.input(|input| input.pointer.hover_pos()),
            ) {
                draw_cable(
                    &painter,
                    egui::pos2(source_rect.right(), source_rect.center().y),
                    pointer,
                    theme::AMBER,
                );
            }
        }

        painter.text(
            rect.left_top() + egui::vec2(18.0, 16.0),
            egui::Align2::LEFT_TOP,
            "TOPOLOGY  ·  wheel zoom  ·  middle-drag pan  ·  drag modules  ·  click ports to cable",
            FontId::monospace(11.0),
            theme::MUTED,
        );
    }

    fn node_rect(&self, canvas: Rect, node: &GraphNode) -> Rect {
        let origin = canvas.left_top() + self.canvas_pan;
        let position = origin + egui::vec2(node.position[0], node.position[1]) * self.canvas_zoom;
        Rect::from_min_size(position, egui::vec2(184.0, 88.0) * self.canvas_zoom)
    }

    fn connect(&mut self, source: NodeId, target: NodeId) {
        if source == target {
            self.status_message = "A module cannot cable directly to itself".into();
            return;
        }
        let mut candidate = self.graph.clone();
        candidate.connections.push(Connection {
            from_node: source,
            from_port: 0,
            to_node: target,
            to_port: 0,
        });
        match candidate.validate() {
            Ok(()) => {
                self.graph = candidate;
                self.status_message = "Cable connected".into();
            }
            Err(error) => self.status_message = error.to_string(),
        }
    }

    fn add_node(&mut self, kind: NodeKind) {
        let id = NodeId(
            self.graph
                .nodes
                .iter()
                .map(|node| node.id.0)
                .max()
                .unwrap_or(0)
                + 1,
        );
        let (scope, inputs, outputs) = node_ports(&kind);
        let node = GraphNode {
            id,
            name: node_label(&kind).into(),
            kind,
            scope,
            position: [320.0 + id.0 as f32 * 9.0, 260.0 + id.0 as f32 * 5.0],
            inputs,
            outputs,
            parameters: BTreeMap::new(),
        };
        self.graph.nodes.push(node);
        self.selected_node = Some(id);
        self.main_view = MainView::Topology;
        self.status_message = "Module added; connect its ports in the canvas".into();
    }

    fn bottom_analysis(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("analysis")
            .resizable(true)
            .default_size(225.0)
            .size_range(150.0..=360.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ui, |ui| {
                let analysis = self
                    .last_snapshot
                    .as_ref()
                    .map(|snapshot| &snapshot.analysis)
                    .unwrap_or(&self.silence);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("ANALYSIS")
                            .strong()
                            .small()
                            .color(theme::MUTED),
                    );
                    ui.label(
                        egui::RichText::new(&self.status_message)
                            .small()
                            .color(theme::TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "LUFS-M  {:>6.1}    CORR  {:+.2}",
                                analysis.momentary_lufs, analysis.stereo_correlation
                            ))
                            .monospace()
                            .color(theme::CYAN),
                        );
                    });
                });
                ui.add_space(4.0);
                let visual_height = ui.available_height().max(72.0);
                ui.horizontal(|ui| {
                    let meter_width = 52.0;
                    let spectrum_width =
                        (ui.available_width() - meter_width * 3.0 - 34.0).max(260.0);
                    spectrum(ui, analysis, egui::vec2(spectrum_width, visual_height));
                    level_meter(
                        ui,
                        "L",
                        analysis.peak_dbfs[0],
                        analysis.rms_dbfs[0],
                        visual_height,
                    );
                    level_meter(
                        ui,
                        "R",
                        analysis.peak_dbfs[1],
                        analysis.rms_dbfs[1],
                        visual_height,
                    );
                    correlation_meter(ui, analysis.stereo_correlation, visual_height);
                });
            });
    }

    fn settings_window(&mut self, context: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = self.show_settings;
        egui::Window::new("Audio & GPU")
            .open(&mut open)
            .resizable(true)
            .default_width(470.0)
            .show(context, |ui| {
                ui.heading("Realtime audio");
                egui::ComboBox::from_label("Output device")
                    .selected_text(self.selected_device.as_deref().unwrap_or("System default"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_device, None, "System default");
                        for device in &self.devices {
                            ui.selectable_value(
                                &mut self.selected_device,
                                Some(device.clone()),
                                device,
                            );
                        }
                    });
                egui::ComboBox::from_label("MIDI / MPE input")
                    .selected_text(self.selected_midi.as_deref().unwrap_or("First available"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_midi, None, "First available");
                        for device in &self.midi_devices {
                            ui.selectable_value(
                                &mut self.selected_midi,
                                Some(device.clone()),
                                device,
                            );
                        }
                    });
                egui::ComboBox::from_label("Sample rate")
                    .selected_text(format!("{} Hz", self.sample_rate))
                    .show_ui(ui, |ui| {
                        for rate in [44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000] {
                            ui.selectable_value(&mut self.sample_rate, rate, format!("{rate} Hz"));
                        }
                    });
                egui::ComboBox::from_label("Block size")
                    .selected_text(format!("{} frames", self.block_size))
                    .show_ui(ui, |ui| {
                        for size in [32, 64, 128, 256, 512, 1024] {
                            ui.selectable_value(&mut self.block_size, size, format!("{size}"));
                        }
                    });
                if ui.button("Restart audio with these settings").clicked() {
                    self.restart_audio();
                }
                ui.separator();
                ui.heading("Compute backends");
                for status in &self.gpu_statuses {
                    ui.horizontal(|ui| {
                        let color = if status.available { theme::CYAN } else { theme::MUTED };
                        status_pill(
                            ui,
                            if status.available { "READY" } else { "ABSENT" },
                            color,
                        );
                        ui.strong(status.kind.label());
                        ui.label(&status.device_name);
                    });
                    ui.label(egui::RichText::new(&status.detail).small().color(theme::MUTED));
                }
                ui.label(
                    egui::RichText::new(
                        "GPU batches run on the analysis/offline path; the realtime callback never waits for a GPU.",
                    )
                    .small()
                    .color(theme::AMBER),
                );
            });
        self.show_settings = open;
    }

    fn about_window(&mut self, context: &egui::Context) {
        if !self.show_about {
            return;
        }
        egui::Window::new("About Schisma")
            .open(&mut self.show_about)
            .resizable(false)
            .show(context, |ui| {
                ui.heading(format!("Schisma {VERSION}"));
                ui.label("An open-source MPE instrument where every note is a programmable physical body.");
                ui.add_space(8.0);
                ui.label("Stereo f32 · 8–384 kHz · Scala/KBM · Metal · CUDA");
                ui.label(egui::RichText::new("MIT licensed").small().color(theme::MUTED));
            });
    }
}

impl eframe::App for SchismaApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_snapshot();
        context.request_repaint_after(Duration::from_millis(16));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.menu_bar(ui);
        self.global_strip(ui);
        self.bottom_analysis(ui);
        self.left_palette(ui);
        self.right_inspector(ui);
        self.center(ui);
        let context = ui.ctx().clone();
        self.settings_window(&context);
        self.about_window(&context);
    }
}

fn module_group(ui: &mut egui::Ui, label: &str, modules: &[NodeKind], app: &mut SchismaApp) {
    ui.label(egui::RichText::new(label).small().color(theme::VIOLET));
    for kind in modules {
        let response = ui.add_sized(
            [ui.available_width(), 34.0],
            egui::Button::new(node_label(kind)).fill(theme::PANEL_RAISED),
        );
        if response.clicked() {
            app.add_node(kind.clone());
        }
    }
    ui.add_space(10.0);
}

fn node_ports(kind: &NodeKind) -> (NodeScope, Vec<PortKind>, Vec<PortKind>) {
    match kind {
        NodeKind::Wavetable | NodeKind::Exciter => {
            (NodeScope::PerVoice, vec![], vec![PortKind::AudioMono])
        }
        NodeKind::Morph | NodeKind::ModalBody | NodeKind::Drive | NodeKind::Delay => (
            NodeScope::PerVoice,
            vec![PortKind::AudioMono],
            vec![PortKind::AudioMono],
        ),
        NodeKind::Filter => (
            NodeScope::PerVoice,
            vec![PortKind::AudioMono],
            vec![PortKind::AudioStereo],
        ),
        NodeKind::VoiceBus => (
            NodeScope::Global,
            vec![PortKind::AudioStereo],
            vec![PortKind::AudioStereo],
        ),
        NodeKind::Reverb | NodeKind::Equalizer | NodeKind::Limiter => (
            NodeScope::Global,
            vec![PortKind::AudioStereo],
            vec![PortKind::AudioStereo],
        ),
        NodeKind::Output => (NodeScope::Global, vec![PortKind::AudioStereo], vec![]),
    }
}

fn node_label(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Wavetable => "Wavetable",
        NodeKind::Exciter => "Noise / Impulse",
        NodeKind::Morph => "Energy Morph",
        NodeKind::ModalBody => "Modal Body",
        NodeKind::Filter => "TPT Filter",
        NodeKind::Drive => "Per-note Drive",
        NodeKind::Delay => "Feedback Delay",
        NodeKind::VoiceBus => "Voice Bus",
        NodeKind::Reverb => "Global Reverb",
        NodeKind::Equalizer => "Global EQ",
        NodeKind::Limiter => "Safety Limiter",
        NodeKind::Output => "Stereo Output",
    }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, pan: Vec2, zoom: f32) {
    let spacing = 34.0 * zoom;
    let mut x = rect.left() + pan.x.rem_euclid(spacing);
    while x < rect.right() {
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(1.0, theme::GRID),
        );
        x += spacing;
    }
    let mut y = rect.top() + pan.y.rem_euclid(spacing);
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, theme::GRID),
        );
        y += spacing;
    }
}

fn draw_node(painter: &egui::Painter, node: &GraphNode, rect: Rect, selected: bool) {
    let accent = if node.scope == NodeScope::PerVoice {
        theme::VIOLET
    } else {
        theme::CYAN
    };
    painter.rect_filled(rect, 9.0, theme::PANEL_RAISED);
    painter.rect_stroke(
        rect,
        9.0,
        Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected { accent } else { theme::GRID },
        ),
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(
        Rect::from_min_size(rect.min, egui::vec2(5.0, rect.height())),
        3.0,
        accent,
    );
    painter.text(
        rect.left_top() + egui::vec2(16.0, 15.0),
        egui::Align2::LEFT_TOP,
        &node.name,
        FontId::proportional(14.0),
        theme::TEXT,
    );
    painter.text(
        rect.left_bottom() + egui::vec2(16.0, -15.0),
        egui::Align2::LEFT_BOTTOM,
        if node.scope == NodeScope::PerVoice {
            "PER NOTE ×16"
        } else {
            "GLOBAL ×1"
        },
        FontId::monospace(9.0),
        theme::MUTED,
    );
    if !node.inputs.is_empty() {
        painter.circle_filled(egui::pos2(rect.left(), rect.center().y), 6.0, accent);
    }
    if !node.outputs.is_empty() {
        painter.circle_filled(egui::pos2(rect.right(), rect.center().y), 6.0, accent);
    }
}

fn draw_cable(painter: &egui::Painter, start: Pos2, end: Pos2, color: Color32) {
    let distance = (end.x - start.x).abs().max(60.0) * 0.45;
    let mut points = Vec::with_capacity(25);
    for step in 0..=24 {
        let t = step as f32 / 24.0;
        let one = 1.0 - t;
        let c1 = start + egui::vec2(distance, 0.0);
        let c2 = end - egui::vec2(distance, 0.0);
        let point = start.to_vec2() * one.powi(3)
            + c1.to_vec2() * (3.0 * one.powi(2) * t)
            + c2.to_vec2() * (3.0 * one * t * t)
            + end.to_vec2() * t.powi(3);
        points.push(point.to_pos2());
    }
    painter.add(egui::Shape::line(points, Stroke::new(2.0, color)));
}

fn knob(ui: &mut egui::Ui, label: &str, value: &mut f32, color: Color32) -> bool {
    let before = *value;
    ui.vertical(|ui| {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(66.0, 66.0), Sense::drag());
        if response.dragged() {
            *value = (*value - ui.input(|input| input.pointer.delta().y) * 0.006).clamp(0.0, 1.0);
        }
        let painter = ui.painter();
        let center = rect.center();
        painter.circle_filled(center, 27.0, theme::PANEL_RAISED);
        painter.circle_stroke(center, 27.0, Stroke::new(2.0, theme::GRID));
        let angle = std::f32::consts::PI * (0.75 + *value * 1.5);
        let tip = center + egui::vec2(angle.cos(), angle.sin()) * 21.0;
        painter.line_segment([center, tip], Stroke::new(3.0, color));
        painter.circle_filled(center, 4.0, color);
        ui.label(
            egui::RichText::new(format!("{label} {:>3}%", (*value * 100.0).round()))
                .monospace()
                .small()
                .color(theme::MUTED),
        );
    });
    (*value - before).abs() > f32::EPSILON
}

fn resample_spectrum(spectrum: &[f32]) -> Vec<f32> {
    if spectrum.len() < 2 {
        return vec![-120.0; WATERFALL_BINS];
    }
    (0..WATERFALL_BINS)
        .map(|index| {
            let normalized = index as f32 / (WATERFALL_BINS - 1) as f32;
            let source = normalized.powi(2) * (spectrum.len() - 1) as f32;
            let lower = source.floor() as usize;
            let upper = (lower + 1).min(spectrum.len() - 1);
            let fraction = source - lower as f32;
            egui::lerp(spectrum[lower]..=spectrum[upper], fraction)
        })
        .collect()
}

fn waterfall_spectral_background(ui: &egui::Ui, stage_rect: Rect, history: &VecDeque<Vec<f32>>) {
    let rect = stage_rect.shrink2(egui::vec2(10.0, 8.0));
    if rect.width() < 240.0 || rect.height() < 96.0 {
        return;
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 7.0, theme::BG.gamma_multiply(0.96));
    painter.rect_stroke(
        rect,
        7.0,
        Stroke::new(1.0, theme::GRID.gamma_multiply(0.75)),
        egui::StrokeKind::Inside,
    );
    let horizon_y = rect.top() + rect.height() * 0.13;
    let front_y = rect.bottom() - 18.0;
    let center_x = rect.center().x;
    let horizon_half_width = rect.width() * 0.09;
    let front_half_width = rect.width() * 0.57;
    let project = |depth: f32, horizontal: f32, lift: f32| {
        let perspective = depth.powf(1.28);
        let half_width = egui::lerp(horizon_half_width..=front_half_width, perspective);
        egui::pos2(
            center_x + horizontal * half_width,
            egui::lerp(horizon_y..=front_y, perspective) - lift,
        )
    };

    let faint_grid = Color32::from_rgba_unmultiplied(
        theme::VIOLET.r(),
        theme::VIOLET.g(),
        theme::VIOLET.b(),
        25,
    );
    for division in 0..=16 {
        let horizontal = -1.0 + 2.0 * division as f32 / 16.0;
        painter.line_segment(
            [project(0.0, horizontal, 0.0), project(1.0, horizontal, 0.0)],
            Stroke::new(0.8, faint_grid),
        );
    }
    for layer in 0..=14 {
        let depth = (layer as f32 / 14.0).powf(1.15);
        painter.line_segment(
            [project(depth, -1.0, 0.0), project(depth, 1.0, 0.0)],
            Stroke::new(0.8, faint_grid),
        );
    }
    let rail =
        Color32::from_rgba_unmultiplied(theme::CYAN.r(), theme::CYAN.g(), theme::CYAN.b(), 46);
    painter.line_segment(
        [project(0.0, -1.0, 0.0), project(1.0, -1.0, 0.0)],
        Stroke::new(1.3, rail),
    );
    painter.line_segment(
        [project(0.0, 1.0, 0.0), project(1.0, 1.0, 0.0)],
        Stroke::new(1.3, rail),
    );

    for (index, spectrum) in history.iter().enumerate() {
        if spectrum.len() < 2 {
            continue;
        }
        let age = (history.len() - 1 - index) as f32;
        let depth = (1.0 - age / (WATERFALL_FRAMES - 1) as f32).clamp(0.0, 1.0);
        let perspective = depth.powf(1.28);
        let maximum_lift = egui::lerp(12.0..=rect.height() * 0.29, perspective);
        let points: Vec<_> = spectrum
            .iter()
            .enumerate()
            .map(|(bin, db)| {
                let horizontal = -1.0 + 2.0 * bin as f32 / (spectrum.len() - 1) as f32;
                let energy = ((*db + 105.0) / 105.0).clamp(0.0, 1.0).powf(0.68);
                project(depth, horizontal, energy * maximum_lift)
            })
            .collect();
        let base = theme::VIOLET.lerp_to_gamma(theme::CYAN, perspective);
        let alpha = egui::lerp(24.0..=145.0, perspective) as u8;
        let color = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
        painter.add(egui::Shape::line(
            points.clone(),
            Stroke::new(0.65 + perspective * 1.25, color),
        ));

        if index + 1 == history.len() {
            let glow = Color32::from_rgba_unmultiplied(
                theme::CYAN.r(),
                theme::CYAN.g(),
                theme::CYAN.b(),
                26,
            );
            painter.add(egui::Shape::line(points.clone(), Stroke::new(7.0, glow)));
            painter.add(egui::Shape::line(
                points,
                Stroke::new(2.2, theme::CYAN.gamma_multiply(0.88)),
            ));
        }
    }

    painter.text(
        rect.right_top() + egui::vec2(-12.0, 12.0),
        egui::Align2::RIGHT_TOP,
        "LIVE 3D WATERFALL  /  4096 FFT  /  2.7 s",
        FontId::monospace(9.0),
        Color32::from_rgba_unmultiplied(theme::CYAN.r(), theme::CYAN.g(), theme::CYAN.b(), 94),
    );
}

fn voice_monitor(ui: &mut egui::Ui, snapshot: &RuntimeSnapshot) {
    egui::ScrollArea::horizontal()
        .id_salt("voice_monitor")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (index, voice) in snapshot.voices.iter().enumerate() {
                    let (fill, text) = if voice.active {
                        (
                            theme::VIOLET.lerp_to_gamma(theme::CYAN, voice.timbre),
                            format!("{:02} {}", index + 1, note_name(voice.note)),
                        )
                    } else {
                        (theme::PANEL_RAISED, format!("{:02} -", index + 1))
                    };
                    let response = ui.add_sized(
                        [72.0, 34.0],
                        egui::Button::new(egui::RichText::new(text).monospace().small()).fill(fill),
                    );
                    if voice.active {
                        response.on_hover_text(format!(
                            "Pressure {:.0}% · Timbre {:.0}%{}",
                            voice.pressure * 100.0,
                            voice.timbre * 100.0,
                            if voice.released { " · release" } else { "" }
                        ));
                    }
                }
            });
        });
}

fn spectrum(ui: &mut egui::Ui, analysis: &AnalysisSnapshot, size: Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 5.0, theme::BG);
    for division in 1..5 {
        let y = egui::lerp(rect.y_range(), division as f32 / 5.0);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, theme::GRID),
        );
    }
    if analysis.spectrum_db.len() > 1 {
        let points: Vec<_> = analysis
            .spectrum_db
            .iter()
            .enumerate()
            .map(|(index, db)| {
                let normalized = index as f32 / (analysis.spectrum_db.len() - 1) as f32;
                let x = rect.left() + normalized.sqrt() * rect.width();
                let y = rect.bottom() - ((*db + 100.0) / 100.0).clamp(0.0, 1.0) * rect.height();
                egui::pos2(x, y)
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(1.8, theme::CYAN)));
    }
    painter.text(
        rect.left_top() + egui::vec2(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        "SPECTRUM  20 Hz — NYQUIST",
        FontId::monospace(9.0),
        theme::MUTED,
    );
}

fn level_meter(ui: &mut egui::Ui, label: &str, peak: f32, rms: f32, height: f32) {
    let (outer, _) = ui.allocate_exact_size(egui::vec2(34.0, height), Sense::hover());
    let rect = Rect::from_min_max(outer.min, egui::pos2(outer.right(), outer.bottom() - 22.0));
    ui.painter().rect_filled(rect, 4.0, theme::BG);
    let peak_t = ((peak + 72.0) / 72.0).clamp(0.0, 1.0);
    let rms_t = ((rms + 72.0) / 72.0).clamp(0.0, 1.0);
    let rms_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 5.0, rect.bottom() - rect.height() * rms_t),
        egui::pos2(rect.right() - 5.0, rect.bottom()),
    );
    ui.painter().rect_filled(rms_rect, 2.0, theme::CYAN);
    let peak_y = rect.bottom() - rect.height() * peak_t;
    ui.painter().line_segment(
        [
            egui::pos2(rect.left() + 3.0, peak_y),
            egui::pos2(rect.right() - 3.0, peak_y),
        ],
        Stroke::new(
            2.0,
            if peak > -1.0 {
                theme::RED
            } else {
                theme::AMBER
            },
        ),
    );
    ui.painter().text(
        egui::pos2(outer.center().x, outer.bottom()),
        egui::Align2::CENTER_BOTTOM,
        label,
        FontId::monospace(11.0),
        theme::MUTED,
    );
}

fn correlation_meter(ui: &mut egui::Ui, correlation: f32, height: f32) {
    let (outer, _) = ui.allocate_exact_size(egui::vec2(58.0, height), Sense::hover());
    let rect = Rect::from_center_size(
        egui::pos2(outer.center().x, outer.center().y),
        egui::vec2(58.0, 18.0),
    );
    ui.painter().text(
        egui::pos2(outer.center().x, outer.top()),
        egui::Align2::CENTER_TOP,
        "CORR",
        FontId::monospace(9.0),
        theme::MUTED,
    );
    ui.painter().rect_filled(rect, 4.0, theme::BG);
    let center = rect.center().x;
    let x = center + correlation.clamp(-1.0, 1.0) * rect.width() * 0.48;
    ui.painter().line_segment(
        [
            egui::pos2(center, rect.top()),
            egui::pos2(center, rect.bottom()),
        ],
        Stroke::new(1.0, theme::GRID),
    );
    ui.painter().circle_filled(
        egui::pos2(x, rect.center().y),
        5.0,
        if correlation < 0.0 {
            theme::RED
        } else {
            theme::CYAN
        },
    );
    ui.painter().text(
        egui::pos2(outer.center().x, outer.bottom()),
        egui::Align2::CENTER_BOTTOM,
        format!("{correlation:+.2}"),
        FontId::monospace(11.0),
        theme::TEXT,
    );
}

fn status_pill(ui: &mut egui::Ui, label: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .corner_radius(99.0)
        .inner_margin(egui::Margin::symmetric(9, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).strong().small().color(color));
        });
}

fn tab_button(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).strong().small()).fill(if selected {
            theme::VIOLET.gamma_multiply(0.55)
        } else {
            theme::PANEL_RAISED
        }),
    )
}

fn labeled_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).small().color(theme::MUTED));
    ui.label(value);
    ui.add_space(5.0);
}

fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!(
        "{}{}",
        NAMES[usize::from(note % 12)],
        i16::from(note) / 12 - 1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waterfall_resampling_preserves_range_and_endpoints() {
        let resampled = resample_spectrum(&[-120.0, -60.0, 0.0]);
        assert_eq!(resampled.len(), WATERFALL_BINS);
        assert_eq!(resampled[0], -120.0);
        assert_eq!(resampled[WATERFALL_BINS - 1], 0.0);
        assert!(resampled.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn waterfall_resampling_handles_missing_fft_data() {
        assert_eq!(resample_spectrum(&[]), vec![-120.0; WATERFALL_BINS]);
    }
}
