use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(10, 12, 17);
pub const PANEL: Color32 = Color32::from_rgb(17, 20, 27);
pub const PANEL_RAISED: Color32 = Color32::from_rgb(24, 29, 38);
pub const GRID: Color32 = Color32::from_rgb(31, 37, 48);
pub const TEXT: Color32 = Color32::from_rgb(224, 230, 239);
pub const MUTED: Color32 = Color32::from_rgb(123, 135, 153);
pub const CYAN: Color32 = Color32::from_rgb(48, 219, 207);
pub const VIOLET: Color32 = Color32::from_rgb(142, 103, 255);
pub const AMBER: Color32 = Color32::from_rgb(244, 177, 72);
pub const RED: Color32 = Color32::from_rgb(244, 84, 95);

pub fn install(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals.dark_mode = true;
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BG;
    style.visuals.faint_bg_color = PANEL_RAISED;
    style.visuals.widgets.inactive.bg_fill = PANEL_RAISED;
    style.visuals.widgets.inactive.weak_bg_fill = PANEL_RAISED;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, MUTED);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(31, 38, 50);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(37, 45, 59);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.5, CYAN);
    style.visuals.selection.bg_fill = VIOLET.gamma_multiply(0.45);
    style.visuals.selection.stroke = Stroke::new(1.0, VIOLET);
    style.visuals.window_corner_radius = CornerRadius::same(10);
    style.visuals.menu_corner_radius = CornerRadius::same(8);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(22.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();
    ctx.set_style_of(egui::Theme::Dark, style);
}
