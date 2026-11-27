//! GUI G-Menu (eframe/egui) — tema escuro AOC, sidebar, color picker visual.
//! Não bloqueia a UI: I2C via `WorkerHandle` em background.

use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::mpsc::TryRecvError;

use aoc_crosshair::render_crosshair;
use aoc_gmenu::backend::{detect_aoc, I2cBackend};
use aoc_gmenu::config::{
    crosshair_pidfile, load_config, parse_rgba_hex, save_config, AppConfig, CrosshairShape,
};
use aoc_gmenu::features::{find_feature, format_value, ValueKind};
use aoc_gmenu::logutil::FRIENDLY_READ_FAIL;
use aoc_gmenu::profile::{apply_profile, get_profile};
use aoc_gmenu::worker::{WorkerCmd, WorkerEvent, WorkerHandle};
use eframe::egui::{self, Color32, ColorImage, RichText, Sense, TextureHandle, Vec2};

// Palette observada no G-Menu Electron (`_re/asar/css`).
const BG_WINDOW: Color32 = Color32::from_rgb(0x10, 0x11, 0x13);
const BG_SIDEBAR: Color32 = Color32::from_rgb(0x16, 0x18, 0x1a);
const BG_CARD: Color32 = Color32::from_rgb(0x1f, 0x22, 0x25);
const BG_CARD_ALT: Color32 = Color32::from_rgb(0x28, 0x29, 0x2b);
const ACCENT: Color32 = Color32::from_rgb(0xc9, 0x1b, 0x1b);
const ACCENT_HOT: Color32 = Color32::from_rgb(0xd1, 0x05, 0x0c);
const TEXT: Color32 = Color32::from_rgb(0xe7, 0xe7, 0xe7);
const TEXT_BRIGHT: Color32 = Color32::from_rgb(0xfc, 0xfc, 0xfc);
const MUTED: Color32 = Color32::from_rgb(0x5d, 0x5f, 0x64);
const SIDEBAR_W: f32 = 168.0;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AOC G-Menu")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "AOC G-Menu",
        options,
        Box::new(|cc| Ok(Box::new(GMenuApp::new(cc)))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Imagem,
    Jogo,
    LowBlue,
    Entrada,
    Mira,
    Perfis,
}

struct GMenuApp {
    tab: Tab,
    status: String,
    bus: Option<u32>,
    model: Option<String>,
    values: HashMap<String, u16>,
    maxima: HashMap<String, u16>,
    worker: Option<WorkerHandle>,
    config: AppConfig,
    crosshair_child: Option<Child>,
    error: Option<String>,
    error_tech: Option<String>,
    preview_tex: Option<TextureHandle>,
    preview_key: String,
}

impl GMenuApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);
        let config = load_config().unwrap_or_default();
        let mut app = Self {
            tab: Tab::Imagem,
            status: "Detectando monitor…".into(),
            bus: None,
            model: None,
            values: HashMap::new(),
            maxima: HashMap::new(),
            worker: None,
            config,
            crosshair_child: None,
            error: None,
            error_tech: None,
            preview_tex: None,
            preview_key: String::new(),
        };
        app.start_worker();
        app
    }

    fn start_worker(&mut self) {
        match detect_aoc(self.config.preferred_bus) {
            Ok(mon) => {
                self.bus = Some(mon.bus);
                self.model = mon.edid.model_name.clone();
                self.status = format!(
                    "{}  ·  bus {}  ·  {}",
                    mon.edid.model_name.as_deref().unwrap_or("Monitor"),
                    mon.bus,
                    mon.connector
                );
                let wh = WorkerHandle::spawn_i2c(mon);
                let _ = wh.cmd_tx.send(WorkerCmd::Reload);
                self.worker = Some(wh);
                self.error = None;
                self.error_tech = None;
            }
            Err(e) => {
                self.status = "Sem acesso ao barramento DDC".into();
                self.error = Some(FRIENDLY_READ_FAIL.into());
                self.error_tech = Some(e.to_string());
                self.worker = None;
            }
        }
    }

    fn poll_worker(&mut self) {
        let Some(w) = &self.worker else { return };
        loop {
            match w.event_rx.try_recv() {
                Ok(WorkerEvent::Status {
                    bus,
                    model,
                    connector,
                    error,
                }) => {
                    if let Some(b) = bus {
                        self.bus = Some(b);
                    }
                    if model.is_some() {
                        self.model = model;
                    }
                    if let Some(e) = error {
                        self.error = Some(FRIENDLY_READ_FAIL.into());
                        self.error_tech = Some(e);
                    } else if let (Some(b), Some(c)) = (bus, connector) {
                        self.status = format!(
                            "{}  ·  bus {b}  ·  {c}",
                            self.model.as_deref().unwrap_or("Monitor")
                        );
                    }
                }
                Ok(WorkerEvent::Values(map)) => {
                    for (id, fv) in map {
                        if fv.error.is_none() {
                            self.values.insert(id.clone(), fv.current);
                            self.maxima.insert(id, fv.maximum);
                        } else if let Some(err) = fv.error {
                            self.error = Some(format!("{id}: {err}"));
                            self.error_tech = Some(err);
                        }
                    }
                }
                Ok(WorkerEvent::Written { feature_id, value }) => {
                    self.values.insert(feature_id, value);
                }
                Ok(WorkerEvent::Error(e)) => {
                    self.error_tech = Some(e.clone());
                    self.error = Some(if e.contains("Não consegui") || e.contains("não suportado") {
                        e
                    } else {
                        FRIENDLY_READ_FAIL.into()
                    });
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.error = Some(FRIENDLY_READ_FAIL.into());
                    self.error_tech = Some("worker DDC desconectado".into());
                    self.worker = None;
                    break;
                }
            }
        }
    }

    fn set_now(&self, id: &str, value: u16) {
        if let Some(w) = &self.worker {
            let _ = w.cmd_tx.send(WorkerCmd::SetNow {
                feature_id: id.into(),
                value,
            });
        }
    }

    fn set_debounced(&self, id: &str, value: u16) {
        if let Some(w) = &self.worker {
            let _ = w.cmd_tx.send(WorkerCmd::SetDebounced {
                feature_id: id.into(),
                value,
            });
        }
    }

    fn reload(&self) {
        if let Some(w) = &self.worker {
            let _ = w.cmd_tx.send(WorkerCmd::Reload);
        }
    }

    fn persist_config(&mut self) {
        if let Err(e) = save_config(&self.config) {
            self.error = Some(e);
        }
    }

    fn signal_crosshair_reload(&self) {
        if let Ok(pid_str) = std::fs::read_to_string(crosshair_pidfile()) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                let _ = Command::new("kill")
                    .args(["-USR1", &pid.to_string()])
                    .status();
            }
        }
    }

    fn start_crosshair(&mut self) {
        self.stop_crosshair();
        self.config.crosshair.enabled = true;
        self.persist_config();
        let bin = find_bin("aoc-crosshair");
        match Command::new(bin).spawn() {
            Ok(child) => {
                let pid = child.id();
                let _ = std::fs::write(crosshair_pidfile(), format!("{pid}\n"));
                self.crosshair_child = Some(child);
            }
            Err(e) => {
                self.error = Some(format!("falha ao iniciar mira: {e}"));
            }
        }
    }

    fn stop_crosshair(&mut self) {
        if let Some(mut child) = self.crosshair_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Ok(pid_str) = std::fs::read_to_string(crosshair_pidfile()) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
        }
        let _ = std::fs::remove_file(crosshair_pidfile());
        self.config.crosshair.enabled = false;
        self.persist_config();
    }

    fn apply_profile_named(&mut self, name: &str) {
        let Ok(p) = get_profile(&self.config, name).cloned() else {
            self.error = Some(format!("perfil desconhecido: {name}"));
            return;
        };
        let Some(bus) = self.bus else {
            self.error = Some(FRIENDLY_READ_FAIL.into());
            return;
        };
        let mut backend = I2cBackend::new(bus);
        match apply_profile(&mut backend, &p) {
            Ok(_) => {
                if let Some(en) = p.crosshair_enabled {
                    if en {
                        self.start_crosshair();
                    } else {
                        self.stop_crosshair();
                    }
                }
                self.reload();
            }
            Err(e) => {
                self.error_tech = Some(e.to_string());
                self.error = Some(FRIENDLY_READ_FAIL.into());
            }
        }
    }

    fn ensure_preview(&mut self, ctx: &egui::Context) {
        let key = format!(
            "{:?}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.config.crosshair.shape,
            self.config.crosshair.size,
            self.config.crosshair.thickness,
            self.config.crosshair.gap,
            self.config.crosshair.opacity,
            self.config.crosshair.color,
            self.config.crosshair.outline,
            self.config.crosshair.outline_color,
            self.config.crosshair.outline_thickness
        );
        if self.preview_key == key && self.preview_tex.is_some() {
            return;
        }
        let pm = render_crosshair(&self.config.crosshair);
        let w = pm.width() as usize;
        let h = pm.height() as usize;
        let mut pixels = Vec::with_capacity(w * h);
        for px in pm.pixels() {
            pixels.push(Color32::from_rgba_premultiplied(
                px.red(),
                px.green(),
                px.blue(),
                px.alpha(),
            ));
        }
        let image = ColorImage { size: [w, h], pixels };
        let tex = ctx.load_texture("crosshair-preview", image, Default::default());
        self.preview_tex = Some(tex);
        self.preview_key = key;
    }
}

fn find_bin(name: &str) -> std::path::PathBuf {
    if let Ok(p) = std::env::var(format!(
        "AOC_{}_BIN",
        name.to_ascii_uppercase().replace('-', "_")
    )) {
        return p.into();
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join(name);
            if cand.exists() {
                return cand;
            }
        }
    }
    name.into()
}

fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals.dark_mode = true;
    style.visuals.panel_fill = BG_WINDOW;
    style.visuals.window_fill = BG_WINDOW;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.widgets.noninteractive.bg_fill = BG_CARD;
    style.visuals.widgets.inactive.bg_fill = BG_CARD_ALT;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x37, 0x39, 0x40);
    style.visuals.widgets.active.bg_fill = ACCENT;
    style.visuals.widgets.open.bg_fill = BG_CARD_ALT;
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.widgets.inactive.fg_stroke.color = TEXT;
    style.visuals.widgets.hovered.fg_stroke.color = TEXT_BRIGHT;
    style.visuals.extreme_bg_color = BG_SIDEBAR;
    style.visuals.faint_bg_color = BG_CARD;
    style.spacing.item_spacing = Vec2::new(10.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(22.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(15.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

fn card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(BG_CARD)
        .rounding(10.0)
        .inner_margin(egui::Margin::same(16.0))
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(0x2b, 0x2d, 0x32)))
        .show(ui, |ui| {
            ui.label(
                RichText::new(title)
                    .strong()
                    .color(TEXT_BRIGHT)
                    .size(18.0),
            );
            ui.add_space(10.0);
            add(ui);
        });
}

fn styled_slider(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>) -> egui::Response {
    ui.spacing_mut().slider_width = 220.0;
    ui.add(
        egui::Slider::new(value, range)
            .integer()
            .trailing_fill(true)
            .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.55 }),
    )
}

fn feature_widget(app: &mut GMenuApp, ui: &mut egui::Ui, id: &str) {
    let Some(f) = find_feature(id) else { return };
    if !f.confirmed {
        return;
    }
    let cur = app.values.get(id).copied().unwrap_or(0);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(160.0, 28.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(RichText::new(f.label).color(TEXT).size(15.0));
            },
        );
        match f.kind {
            ValueKind::Range { min, max } => {
                let hi = app.maxima.get(id).copied().unwrap_or(max).min(max);
                let mut v = cur as f32;
                let resp = styled_slider(ui, &mut v, min as f32..=hi as f32);
                if resp.changed() {
                    let nv = v as u16;
                    app.values.insert(id.into(), nv);
                    app.set_debounced(id, nv);
                }
            }
            ValueKind::Named => {
                let label = format_value(f, cur);
                egui::ComboBox::from_id_salt(id)
                    .selected_text(RichText::new(label).color(TEXT_BRIGHT))
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        let mut seen = Vec::new();
                        for nv in f.values {
                            if seen.contains(&nv.code) {
                                continue;
                            }
                            seen.push(nv.code);
                            if ui
                                .selectable_label(cur == nv.code, nv.name)
                                .clicked()
                            {
                                app.values.insert(id.into(), nv.code);
                                app.set_now(id, nv.code);
                            }
                        }
                    });
            }
        }
    });
    ui.add_space(4.0);
}

fn paint_sidebar_icon(ui: &mut egui::Ui, tab: Tab, active: bool) {
    let (rect, _resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
    let color = if active { ACCENT } else { MUTED };
    let stroke = egui::Stroke::new(1.6_f32, color);
    let painter = ui.painter();
    let c = rect.center();
    let r = rect.width() * 0.38;
    match tab {
        Tab::Imagem => {
            // sun / brightness
            painter.circle_stroke(c, r * 0.45, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let dir = Vec2::new(a.cos(), a.sin());
                let i0 = c + dir * (r * 0.6);
                let i1 = c + dir * r;
                painter.line_segment([i0, i1], stroke);
            }
        }
        Tab::Jogo => {
            // gamepad-ish
            painter.rect_stroke(
                egui::Rect::from_center_size(c, Vec2::new(r * 1.6, r * 1.0)),
                3.0,
                stroke,
            );
            painter.circle_filled(c - Vec2::new(r * 0.45, 0.0), 1.8, color);
            painter.circle_filled(c + Vec2::new(r * 0.45, -r * 0.15), 1.5, color);
        }
        Tab::LowBlue => {
            painter.circle_stroke(c, r * 0.7, stroke);
            painter.line_segment(
                [c - Vec2::new(r * 0.35, 0.0), c + Vec2::new(r * 0.35, 0.0)],
                stroke,
            );
        }
        Tab::Entrada => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, Vec2::new(r * 1.3, r * 0.9)),
                2.0,
                stroke,
            );
            painter.line_segment(
                [c + Vec2::new(r * 0.65, 0.0), c + Vec2::new(r * 1.05, 0.0)],
                stroke,
            );
        }
        Tab::Mira => {
            painter.line_segment([c - Vec2::new(r, 0.0), c + Vec2::new(r, 0.0)], stroke);
            painter.line_segment([c - Vec2::new(0.0, r), c + Vec2::new(0.0, r)], stroke);
            painter.circle_stroke(c, r * 0.35, stroke);
        }
        Tab::Perfis => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c - Vec2::new(2.0, 2.0), Vec2::splat(r * 1.1)),
                1.5,
                stroke,
            );
            painter.rect_stroke(
                egui::Rect::from_center_size(c + Vec2::new(2.0, 2.0), Vec2::splat(r * 1.1)),
                1.5,
                stroke,
            );
        }
    }
}

fn hex_to_color32(hex: &str) -> Color32 {
    let (r, g, b, a) = parse_rgba_hex(hex).unwrap_or((255, 0, 0, 255));
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn color32_to_hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}{:02X}", c.r(), c.g(), c.b(), c.a())
}

const SWATCHES: &[(&str, &str)] = &[
    ("Vermelho", "#FF0000FF"),
    ("Verde", "#00FF00FF"),
    ("Ciano", "#00FFFFFF"),
    ("Amarelo", "#FFFF00FF"),
    ("Magenta", "#FF00FFFF"),
    ("Branco", "#FFFFFFFF"),
    ("Laranja", "#FF8800FF"),
];

impl eframe::App for GMenuApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        egui::SidePanel::left("sidebar")
            .exact_width(SIDEBAR_W)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(BG_SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(12.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.label(RichText::new("AOC").color(ACCENT).size(13.0).strong());
                ui.label(RichText::new("G-Menu").color(TEXT_BRIGHT).size(22.0).strong());
                ui.add_space(18.0);

                for (tab, label) in [
                    (Tab::Imagem, "Imagem"),
                    (Tab::Jogo, "Jogo"),
                    (Tab::LowBlue, "Low Blue"),
                    (Tab::Entrada, "Entrada"),
                    (Tab::Mira, "Mira"),
                    (Tab::Perfis, "Perfis"),
                ] {
                    let active = self.tab == tab;
                    let fill = if active {
                        Color32::from_rgba_unmultiplied(0xc9, 0x1b, 0x1b, 40)
                    } else {
                        Color32::TRANSPARENT
                    };
                    let frame = egui::Frame::none()
                        .fill(fill)
                        .rounding(8.0)
                        .inner_margin(egui::Margin::symmetric(10.0, 8.0));
                    let resp = frame
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                paint_sidebar_icon(ui, tab, active);
                                ui.add_space(8.0);
                                let color = if active { TEXT_BRIGHT } else { TEXT };
                                ui.label(RichText::new(label).color(color).size(15.0));
                            });
                        })
                        .response
                        .interact(Sense::click());
                    if active {
                        let rect = resp.rect;
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(rect.left() - 8.0, rect.top() + 4.0),
                                Vec2::new(3.0, rect.height() - 8.0),
                            ),
                            1.5,
                            ACCENT,
                        );
                    }
                    if resp.clicked() {
                        self.tab = tab;
                    }
                    ui.add_space(4.0);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Recarregar").color(TEXT_BRIGHT))
                                .fill(ACCENT)
                                .rounding(6.0)
                                .min_size(Vec2::new(SIDEBAR_W - 24.0, 32.0)),
                        )
                        .clicked()
                    {
                        self.error = None;
                        self.error_tech = None;
                        if self.worker.is_none() {
                            self.start_worker();
                        } else {
                            self.reload();
                        }
                    }
                });
            });

        egui::TopBottomPanel::bottom("status")
            .exact_height(36.0)
            .frame(
                egui::Frame::none()
                    .fill(BG_SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(16.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.status).color(MUTED).size(13.0));
                    if let Some(err) = &self.error {
                        ui.separator();
                        let label = ui.colored_label(ACCENT_HOT, err);
                        if let Some(tech) = &self.error_tech {
                            label.on_hover_text(tech);
                        }
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG_WINDOW)
                    .inner_margin(egui::Margin::same(20.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    Tab::Imagem => {
                        card(ui, "Imagem", |ui| {
                            for id in [
                                "brightness",
                                "contrast",
                                "color_temp",
                                "gamma",
                                "dcr",
                                "eco_mode",
                            ] {
                                feature_widget(self, ui, id);
                            }
                        });
                    }
                    Tab::Jogo => {
                        card(ui, "Jogo", |ui| {
                            for id in [
                                "overdrive",
                                "low_input_lag",
                                "game_color",
                                "shadow_control",
                                "frame_counter",
                                "dial_point",
                            ] {
                                feature_widget(self, ui, id);
                            }
                        });
                    }
                    Tab::LowBlue => {
                        card(ui, "Low Blue Mode", |ui| {
                            feature_widget(self, ui, "low_blue_mode");
                        });
                    }
                    Tab::Entrada => {
                        card(ui, "Entrada / Volume", |ui| {
                            feature_widget(self, ui, "input_source");
                            feature_widget(self, ui, "volume");
                        });
                    }
                    Tab::Mira => {
                        self.ui_mira(ui, ctx);
                    }
                    Tab::Perfis => {
                        self.ui_perfis(ui);
                    }
                });
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(w) = &self.worker {
            let _ = w.cmd_tx.send(WorkerCmd::FlushDebounce);
            let _ = w.cmd_tx.send(WorkerCmd::Shutdown);
        }
    }
}

impl GMenuApp {
    fn ui_mira(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.ensure_preview(ctx);

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                card(ui, "Mira de software", |ui| {
                    let mut en = self.config.crosshair.enabled;
                    if ui
                        .checkbox(
                            &mut en,
                            RichText::new("Ativar overlay").color(TEXT),
                        )
                        .changed()
                    {
                        if en {
                            self.start_crosshair();
                        } else {
                            self.stop_crosshair();
                        }
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Output");
                        if ui
                            .text_edit_singleline(&mut self.config.crosshair.output)
                            .changed()
                        {
                            // só persiste no Salvar
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Forma");
                        egui::ComboBox::from_id_salt("shape")
                            .selected_text(self.config.crosshair.shape.as_str())
                            .show_ui(ui, |ui| {
                                for s in [
                                    CrosshairShape::Cross,
                                    CrosshairShape::X,
                                    CrosshairShape::Dot,
                                    CrosshairShape::Circle,
                                    CrosshairShape::CrossDot,
                                ] {
                                    if ui
                                        .selectable_label(
                                            self.config.crosshair.shape == s,
                                            s.as_str(),
                                        )
                                        .clicked()
                                    {
                                        self.config.crosshair.shape = s;
                                    }
                                }
                            });
                    });

                    let mut size = self.config.crosshair.size as f32;
                    ui.horizontal(|ui| {
                        ui.label("Tamanho");
                        if styled_slider(ui, &mut size, 8.0..=128.0).changed() {
                            self.config.crosshair.size = size as u32;
                        }
                    });
                    let mut thick = self.config.crosshair.thickness as f32;
                    ui.horizontal(|ui| {
                        ui.label("Espessura");
                        if styled_slider(ui, &mut thick, 1.0..=12.0).changed() {
                            self.config.crosshair.thickness = thick as u32;
                        }
                    });
                    let mut gap = self.config.crosshair.gap as f32;
                    ui.horizontal(|ui| {
                        ui.label("Gap");
                        if styled_slider(ui, &mut gap, 0.0..=40.0).changed() {
                            self.config.crosshair.gap = gap as u32;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Opacidade");
                        ui.add(
                            egui::Slider::new(&mut self.config.crosshair.opacity, 0.1..=1.0)
                                .trailing_fill(true),
                        );
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Cor").strong().color(TEXT_BRIGHT));
                    ui.horizontal(|ui| {
                        let mut color = hex_to_color32(&self.config.crosshair.color);
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            self.config.crosshair.color = color32_to_hex(color);
                        }
                        ui.label(RichText::new("seletor HSV").color(MUTED).small());
                    });

                    ui.horizontal(|ui| {
                        for (name, hex) in SWATCHES {
                            let c = hex_to_color32(hex);
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                            ui.painter().rect_filled(rect, 4.0, c);
                            ui.painter().rect_stroke(
                                rect,
                                4.0,
                                egui::Stroke::new(1.0_f32, Color32::from_rgb(0x50, 0x50, 0x55)),
                            );
                            if self.config.crosshair.color.eq_ignore_ascii_case(hex)
                                || hex_to_color32(&self.config.crosshair.color) == c
                            {
                                ui.painter().rect_stroke(
                                    rect.expand(2.0),
                                    5.0,
                                    egui::Stroke::new(2.0_f32, ACCENT),
                                );
                            }
                            if resp.clicked() {
                                self.config.crosshair.color = (*hex).into();
                            }
                            resp.on_hover_text(*name);
                        }
                        // swatch da cor atual
                        let cur = hex_to_color32(&self.config.crosshair.color);
                        let (rect, resp) =
                            ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                        ui.painter().circle_filled(rect.center(), 10.0, cur);
                        ui.painter().circle_stroke(
                            rect.center(),
                            10.0,
                            egui::Stroke::new(1.5_f32, TEXT_BRIGHT),
                        );
                        resp.on_hover_text("Cor atual");
                    });

                    ui.checkbox(&mut self.config.crosshair.outline, "Contorno");
                    if self.config.crosshair.outline {
                        ui.horizontal(|ui| {
                            ui.label("Cor contorno");
                            let mut oc = hex_to_color32(&self.config.crosshair.outline_color);
                            if ui.color_edit_button_srgba(&mut oc).changed() {
                                self.config.crosshair.outline_color = color32_to_hex(oc);
                            }
                        });
                        let mut ot = self.config.crosshair.outline_thickness as f32;
                        ui.horizontal(|ui| {
                            ui.label("Espessura contorno");
                            if styled_slider(ui, &mut ot, 1.0..=6.0).changed() {
                                self.config.crosshair.outline_thickness = ot as u32;
                            }
                        });
                    }

                    ui.add_space(6.0);
                    ui.collapsing(RichText::new("Avançado").color(MUTED), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Hex RGBA");
                            ui.text_edit_singleline(&mut self.config.crosshair.color);
                        });
                    });

                    ui.add_space(10.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Salvar e aplicar").color(TEXT_BRIGHT),
                            )
                            .fill(ACCENT)
                            .rounding(6.0)
                            .min_size(Vec2::new(160.0, 32.0)),
                        )
                        .clicked()
                    {
                        self.persist_config();
                        self.signal_crosshair_reload();
                    }
                });
            });

            ui.add_space(16.0);
            card(ui, "Pré-visualização", |ui| {
                ui.set_min_size(Vec2::new(200.0, 200.0));
                if let Some(tex) = &self.preview_tex {
                    let size = tex.size_vec2();
                    let show = size * (160.0 / size.max_elem()).min(1.0);
                    // fundo xadrez suave
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(180.0), Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 8.0, Color32::from_rgb(0x12, 0x13, 0x15));
                    let img_rect = egui::Rect::from_center_size(rect.center(), show);
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
            });
        });
    }

    fn ui_perfis(&mut self, ui: &mut egui::Ui) {
        card(ui, "Perfis", |ui| {
            ui.label(
                RichText::new("Aplicar grava os valores no monitor via DDC.")
                    .color(MUTED)
                    .size(13.0),
            );
            ui.add_space(8.0);
            let names: Vec<_> = self.config.profiles.keys().cloned().collect();
            for name in names {
                let desc = self
                    .config
                    .profiles
                    .get(&name)
                    .map(|p| p.description.clone())
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("Aplicar «{name}»")).color(TEXT_BRIGHT),
                            )
                            .fill(BG_CARD_ALT)
                            .rounding(6.0),
                        )
                        .clicked()
                    {
                        self.apply_profile_named(&name);
                    }
                    ui.label(RichText::new(desc).color(MUTED));
                });
                ui.add_space(4.0);
            }
        });
    }
}
