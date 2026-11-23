//! De app: zijbalk + pagina's. Alle teksten via `t!` (NL/EN), alle stijl via `theme`.

use anyhow::{Result, anyhow};
use chrono::{Duration, NaiveDate, Weekday};
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, Ui, vec2};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::mpsc;

use crate::agent;
use crate::export;
use crate::i18n::{self, Lang};
use crate::live::LiveState;
use crate::live_training;
use crate::model::{Component, Effectiveness, Factor, category_of, level_color, level_name, session_by_code};
use crate::planner::{self, CalendarOptions, CalendarWeek, PlannerSettings, WeekKind, WeekPlan, unique_schedules};
use crate::t;
use crate::theme::{self, icon};
use crate::vision;

pub fn run() -> Result<()> {
    // testhulp: FM27_WINDOW=1320x1400 voor hogere schermafbeeldingen
    let size = std::env::var("FM27_WINDOW")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?])
        })
        .unwrap_or([1320.0, 880.0]);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FM27 Trainer · Holyman3")
            .with_inner_size(size)
            .with_min_inner_size([1040.0, 660.0])
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/icon_256.png")).unwrap_or_default()),
        ..Default::default()
    };
    eframe::run_native(
        "FM27 Trainer · Holyman3",
        options,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
    .map_err(|e| anyhow!("{e}"))
}

// ------------------------------------------------------------------------ instellingen

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    lang: Lang,
    rate: f32,
    capture_delay: bool,
    #[serde(default = "yes")]
    auto_training: bool,
    /// Aanpassen in FM toestaan op een build die nog niet live getest is.
    #[serde(default = "yes")]
    allow_untested: bool,
    /// Tactisch begrip op 100 % houden zolang de app open is (FM laat het wekelijks zakken).
    #[serde(default)]
    keep_understanding: bool,
    /// De Breakdown elke zoveel seconden zelf opnieuw uitlezen zolang hij in FM open staat
    /// (idee en eerste versie: arhxmmmmm, fork forked-fm27-trainer).
    #[serde(default)]
    auto_read: bool,
    #[serde(default = "ten")]
    auto_read_secs: u32,
    /// Bij meer dan één manager in de save: de UID van de club van de manager die de app bedient.
    #[serde(default)]
    manager_club: Option<u32>,
    /// Het team waarvan de app de training plant (plek in de teamlijst van de club; None = eigen team).
    #[serde(default)]
    training_team: Option<usize>,
    planner: PlannerSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { lang: i18n::detect(), rate: 1.17, capture_delay: true, auto_training: true, allow_untested: true, keep_understanding: false, auto_read: false, auto_read_secs: 10, manager_club: None, training_team: None, planner: PlannerSettings::default() }
    }
}

fn yes() -> bool {
    true
}

fn ten() -> u32 {
    10
}

/// Zelfde waarden (op een halve punt na) als bij de vorige uitlezing.
fn same_values(a: &Effectiveness, b: &Effectiveness) -> bool {
    Factor::ALL.iter().all(|f| (a.get(*f) - b.get(*f)).abs() < 0.5) && Component::ALL.iter().all(|c| (a.component(*c) - b.component(*c)).abs() < 0.5)
}

fn settings_file() -> PathBuf {
    crate::data_dir().join("settings.json")
}

fn calendar_file() -> PathBuf {
    crate::data_dir().join("calendar.json")
}

// ------------------------------------------------------------------------ app

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Overview,
    Training,
    Planning,
    Club,
    Squad,
    Tactics,
    Sell,
    Settings,
}

#[derive(PartialEq, Clone, Copy)]
enum PlanTab {
    Week,
    Season,
}

type AnalysisResult = Result<(vision::Analysis, String), String>;

struct App {
    page: Page,
    /// de pagina die het vorige beeld toonde (voor de intrede-animatie)
    shown_page: Option<Page>,
    settings: Settings,
    eff: Effectiveness,
    status: String,
    status_error: bool,
    analysis_rx: Option<mpsc::Receiver<AnalysisResult>>,
    /// de lopende uitlezing is door automatisch uitlezen gestart (stil, haalt FM niet naar voren)
    auto_pending: bool,
    auto_note: String,
    last_auto: std::time::Instant,
    image_path: String,
    show_file: bool,
    edit_mode: bool,
    plan_tab: PlanTab,
    week_start: String,
    week_matches: [bool; 7],
    prev_sunday: bool,
    next_monday: bool,
    week_kind: WeekKind,
    week_plan: Option<WeekPlan>,
    cal_opts: CalendarOptions,
    year_start: String,
    calendar: Vec<CalendarWeek>,
    year: Vec<WeekPlan>,
    live: LiveState,
    /// de tactiek die de tactiekpagina toont (None: de tactiek die in FM in gebruik is)
    tactic_pick: Option<usize>,
    last_game_tactic: Option<usize>,
    tactics_asked: bool,
    frames: u32,
    logo_tex: Option<egui::TextureHandle>,
    update_rx: Option<mpsc::Receiver<Result<Option<(String, String)>, String>>>,
    latest_release: Option<(String, String)>,
    update_status: String,
}

const GITHUB_REPO_URL: &str = "https://github.com/Holymanrepo/Holyman3-Antreman-fm27";
const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/Holymanrepo/Holyman3-Antreman-fm27/releases/latest";

fn spawn_update_check() -> mpsc::Receiver<Result<Option<(String, String)>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("curl");
        cmd.args([
            "-sL",
            "--max-time",
            "8",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: FM27-Trainer-Holyman3",
            GITHUB_RELEASES_API,
        ]);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let res = match cmd.output() {
            Ok(out) if out.status.success() => match serde_json::from_slice::<Value>(&out.stdout) {
                Ok(v) => {
                    if let Some(tag) = v.get("tag_name").and_then(|t| t.as_str()) {
                        let clean = tag.trim_start_matches('v').trim();
                        let current = env!("CARGO_PKG_VERSION").trim_start_matches('v').trim();
                        let url = v
                            .get("html_url")
                            .and_then(|u| u.as_str())
                            .unwrap_or("https://github.com/Holymanrepo/Holyman3-Antreman-fm27/releases/latest")
                            .to_string();
                        if !clean.is_empty() && clean != current {
                            Ok(Some((tag.to_string(), url)))
                        } else {
                            Ok(None)
                        }
                    } else {
                        Ok(None)
                    }
                }
                Err(_) => Err("GitHub sürüm bilgisi okunamadı.".into()),
            },
            _ => Err("GitHub sunucusuna ulaşılamadı.".into()),
        };
        let _ = tx.send(res);
    });
    rx
}

impl App {
    fn new() -> Self {
        let mut settings: Settings = std::fs::read_to_string(settings_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        settings.allow_untested = true;
        i18n::set(settings.lang);
        let cal_opts = CalendarOptions::default();
        let calendar = std::fs::read_to_string(calendar_file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| planner::default_calendar(&cal_opts));
        let monday = planner::monday_of(chrono::Local::now().date_naive());
        let mut week_matches = [false; 7];
        week_matches[6] = true;
        // FM27_PAGE=training|planning|club|squad|sell|buy|settings opent direct die pagina (handig bij testen)
        let page = match std::env::var("FM27_PAGE").unwrap_or_default().as_str() {
            "training" => Page::Training,
            "planning" => Page::Planning,
            "club" => Page::Club,
            "squad" => Page::Squad,
            "tactics" => Page::Tactics,
            "sell" | "buy" => Page::Sell,
            "settings" => Page::Settings,
            _ => Page::Overview,
        };
        App {
            page,
            shown_page: None,
            eff: crate::load_effectiveness().unwrap_or_default(),
            settings,
            status: String::new(),
            status_error: false,
            analysis_rx: None,
            auto_pending: false,
            auto_note: String::new(),
            last_auto: std::time::Instant::now(),
            image_path: String::new(),
            show_file: false,
            edit_mode: false,
            plan_tab: PlanTab::Week,
            week_start: monday.format("%Y-%m-%d").to_string(),
            week_matches,
            prev_sunday: true,
            next_monday: false,
            week_kind: WeekKind::Season,
            week_plan: None,
            year_start: cal_opts.start.format("%Y-%m-%d").to_string(),
            cal_opts,
            calendar,
            year: Vec::new(),
            tactic_pick: None,
            last_game_tactic: None,
            tactics_asked: false,
            live: {
                let mut l = LiveState::default();
                l.sell.buy = std::env::var("FM27_PAGE").as_deref() == Ok("buy");
                l
            },
            frames: 0,
            logo_tex: None,
            update_rx: Some(spawn_update_check()),
            latest_release: None,
            update_status: String::new(),
        }
    }

    fn poll_update(&mut self) {
        let Some(rx) = &self.update_rx else { return };
        if let Ok(res) = rx.try_recv() {
            self.update_rx = None;
            match res {
                Ok(Some((tag, url))) => {
                    self.update_status = t!(format!("Yeni sürüm mevcut: {tag}"), format!("New version available: {tag}"));
                    self.latest_release = Some((tag, url));
                }
                Ok(None) => {
                    self.update_status = t!(
                        format!("Uygulama güncel (v{}).", env!("CARGO_PKG_VERSION")),
                        format!("App is up to date (v{}).", env!("CARGO_PKG_VERSION"))
                    );
                    self.latest_release = None;
                }
                Err(e) => {
                    self.update_status = e;
                }
            }
        }
    }

    /// Testhulp: FM27_SNAPSHOT=pad.png legt na het laden het eigen venster vast en sluit af.
    fn snapshot(&mut self, ctx: &egui::Context) {
        let Ok(path) = std::env::var("FM27_SNAPSHOT") else { return };
        self.frames += 1;
        if self.frames == 90 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buf.save(&path);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }

    fn save_settings(&self) {
        let _ = std::fs::create_dir_all(crate::data_dir());
        if let Ok(s) = serde_json::to_string_pretty(&self.settings) {
            let _ = std::fs::write(settings_file(), s);
        }
    }

    fn save_calendar(&self) {
        let _ = std::fs::create_dir_all(crate::data_dir());
        if let Ok(s) = serde_json::to_string_pretty(&self.calendar) {
            let _ = std::fs::write(calendar_file(), s);
        }
    }

    fn set_status(&mut self, msg: impl Into<String>, error: bool) {
        self.status = msg.into();
        self.status_error = error;
    }

    fn has_data(&self) -> bool {
        !self.eff.source.is_empty()
    }

    /// Aktif taktik ve kadronun taktik anlayışı verilerini doğrudan FM27 belleğinden okuyup
    /// antrenman planlayıcıya aktarır (ekran görüntüsü almaya gerek kalmadan taktikle tam uyumlu).
    fn sync_from_live_tactic(&mut self, ctx: &egui::Context) {
        if let Some(u) = &self.live.understanding {
            if let Some(team) = u.get("team").and_then(|v| v.as_array()) {
                for (i, f) in LIVE_UNDERSTANDING_FACTORS.iter().enumerate() {
                    if let Some(val) = team.get(i).and_then(|v| v.as_u64()) {
                        self.eff.set(*f, (val as f32 / 100.0).clamp(0.0, 100.0));
                    }
                }
            }
        }
        let mut mode_msg = String::new();
        if let Some(data) = &self.live.tactics {
            if !data.tactics.is_empty() {
                let idx = self
                    .tactic_pick
                    .filter(|i| *i < data.tactics.len())
                    .unwrap_or_else(|| data.current.min(data.tactics.len() - 1));
                let active = &data.tactics[idx];
                let suggested = crate::tactics::suggest_tactic_mode(active);
                self.settings.planner.tactic_mode = suggested;
                self.save_settings();
                let places = crate::tactics::places(active);
                let by_uid: std::collections::HashMap<u32, &crate::tactics::Player> = data.players.iter().map(|p| (p.uid, p)).collect();
                let fits: Vec<f32> = places.iter().filter_map(|pl| pl.uid.and_then(|u| by_uid.get(&u)).map(|p| crate::tactics::fit(p, pl))).collect();
                if !fits.is_empty() {
                    let avg_fit = fits.iter().sum::<f32>() / fits.len() as f32;
                    self.eff.set(Factor::PlayerRoleSuitability, avg_fit);
                    self.eff.set(Factor::PlayerCapability, avg_fit);
                }
                mode_msg = t!(
                    format!(" · Taktik {} · Önerilen Tarz: {}", active.index + 1, suggested.label()),
                    format!(" · Tactic {} · Suggested Style: {}", active.index + 1, suggested.label())
                );
            }
        } else if self.live.connected {
            self.live.load_tactics(ctx);
        }
        if !self.live.squad.is_empty() {
            let morales: Vec<f32> = self.live.squad.iter().filter_map(|p| p.get("morale").and_then(|v| v.as_u64())).map(|m| ((m as f32 - 1.0) / 19.0 * 100.0).clamp(0.0, 100.0)).collect();
            if !morales.is_empty() {
                let avg_m = morales.iter().sum::<f32>() / morales.len() as f32;
                self.eff.set(Factor::ManagerialSupport, avg_m);
                self.eff.set(Factor::TeammatesAndStaffRelationships, avg_m);
            }
        }
        self.eff.components.clear();
        let when = chrono::Local::now().format("%d-%m %H:%M");
        self.eff.source = format!("Canlı Taktik Verisi · {when}");
        if let Ok(v) = agent::request(json!({"cmd": "game_date"})) {
            let (y, d) = (v.get("year").and_then(|x| x.as_i64()), v.get("doy").and_then(|x| x.as_u64()));
            if let (Some(y), Some(d)) = (y, d) {
                self.eff.game_date = NaiveDate::from_yo_opt(y as i32, d as u32);
            }
        }
        let _ = crate::save_effectiveness(&self.eff);
        self.week_plan = None;
        self.year.clear();
        self.set_status(format!("Aktif taktik ve kadro verileri FM27 belleğinden okundu{mode_msg}."), false);
    }

    // -------------------------------------------------------------------- analyse

    fn start_analysis(&mut self, ctx: &egui::Context, file: Option<PathBuf>) {
        let (tx, rx) = mpsc::channel();
        let delay = self.settings.capture_delay && file.is_none() && !self.auto_pending;
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            if delay && cfg!(target_os = "macos") {
                let _ = std::process::Command::new("open").args(["-b", "com.SportsInteractive.FootballManager27"]).status();
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
            let res: AnalysisResult = match file {
                Some(p) => vision::analyze_file(&p).map(|(a, _)| (a, p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default())).map_err(|e| format!("{e:#}")),
                None => vision::analyze_screen().map(|(a, _)| (a, "FM27 ekranı".to_string())).map_err(|e| format!("{e:#}")),
            };
            let _ = tx.send(res);
            ctx2.request_repaint();
        });
        self.analysis_rx = Some(rx);
        if !self.auto_pending {
            self.set_status("Okunuyor…", false);
        }
    }

    fn poll_analysis(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.analysis_rx else { return };
        if let Ok(res) = rx.try_recv() {
            self.analysis_rx = None;
            let _ = ctx;
            let auto = std::mem::take(&mut self.auto_pending);
            match res {
                // automatisch: alleen een volledige, zekere uitlezing telt, en alleen als er iets veranderd is
                Ok((a, _)) if auto && (a.gauges.len() < 17 || a.components.len() < 4 || a.gauges.iter().any(|g| g.doubt)) => {
                    self.auto_note = "FM'de Breakdown ekranı bekleniyor".to_string();
                }
                Ok((a, _)) if auto && same_values(&self.eff, &a.to_effectiveness("")) => {
                    self.auto_note = format!("değişiklik yok ({})", chrono::Local::now().format("%H:%M:%S"));
                }
                Err(_) if auto => {
                    self.auto_note = "FM'de Breakdown ekranı bekleniyor".to_string();
                }
                Ok((a, src)) => {
                    if auto {
                        self.auto_note = format!("güncellendi ({})", chrono::Local::now().format("%H:%M:%S"));
                    }
                    let src_is_screen = src == "FM27 ekranı";
                    let when = chrono::Local::now().format("%d-%m %H:%M");
                    self.eff = a.to_effectiveness(&format!("{src} · {when}"));
                    // spel-datum erbij, zodat de app weet hoe oud de meting in speltijd is
                    if let Ok(v) = agent::request(json!({"cmd": "game_date"})) {
                        let (y, d) = (v.get("year").and_then(|x| x.as_i64()), v.get("doy").and_then(|x| x.as_u64()));
                        if let (Some(y), Some(d)) = (y, d) {
                            self.eff.game_date = NaiveDate::from_yo_opt(y as i32, d as u32);
                        }
                    }
                    let _ = crate::save_effectiveness(&self.eff);
                    let mut msg = format!("17 göstergeden {} tanesi okundu.", a.gauges.len());
                    if !a.lang.starts_with("english") {
                        msg.push_str(&format!(" (FM: {})", a.lang));
                    }
                    // een meting die niet bij de kleur van de meter past: laten nakijken
                    let doubts: Vec<&str> = a.gauges.iter().filter(|g| g.doubt).map(|g| g.factor.label()).collect();
                    if !doubts.is_empty() {
                        msg.push(' ');
                        msg.push_str(&format!("Lütfen kontrol edin (renk ve doluluk uyuşmuyor): {}. 'Değerleri elle ayarla' ile düzeltin.", doubts.join(", ")));
                    }
                    let incomplete = a.gauges.len() < 17 || a.components.len() < 4 || !doubts.is_empty();
                    if incomplete && src_is_screen {
                        msg.push(' ');
                        msg.push_str("Doğru değil mi? Bildiriminize uygulama klasöründeki (Ayarlar → Uygulama klasörünü aç) last-breakdown.png ve last-analysis.txt dosyalarını ekleyin.");
                    }
                    self.set_status(msg, incomplete);
                    self.week_plan = None;
                    self.year.clear();
                }
                Err(e) => self.set_status(e, true),
            }
        }
    }

    // -------------------------------------------------------------------- shell

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .exact_width(244.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(theme::SIDEBAR).inner_margin(Margin { left: 14, right: 14, top: 20, bottom: 16 }))
            .show(ctx, |ui| {
                // haarlijn tussen zijbalk en inhoud
                let edge = ui.max_rect().expand2(vec2(14.0, 20.0));
                ui.painter().vline(edge.right() - 0.5, edge.y_range(), Stroke::new(1.0, theme::BORDER));
                ui.horizontal(|ui| {
                    let tex = self.logo_tex.get_or_insert_with(|| {
                        let img = image::load_from_memory(include_bytes!("../assets/icon/icon_256.png"))
                            .map(|d| d.to_rgba8())
                            .unwrap_or_else(|_| image::RgbaImage::new(1, 1));
                        let size = [img.width() as usize, img.height() as usize];
                        let cimg = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                        ui.ctx().load_texture("holyman_logo", cimg, egui::TextureOptions::LINEAR)
                    });
                    ui.add(egui::Image::from_texture(&*tex).fit_to_exact_size(vec2(36.0, 36.0)).corner_radius(CornerRadius::same(9)));
                    ui.vertical(|ui| {
                        ui.add_space(1.0);
                        ui.label(RichText::new("FM27 Trainer").font(egui::FontId::new(16.0, theme::bold())).color(theme::TEXT));
                        ui.label(theme::faint("by Holyman3"));
                    });
                });
                ui.add_space(20.0);
                ui.label(theme::caps(t!("Menü", "Menu")));
                ui.add_space(4.0);
                let items = [
                    (Page::Overview, icon::GAUGE, t!("Genel Bakış", "Overview")),
                    (Page::Training, icon::BARBELL, t!("Antrenman", "Training")),
                    (Page::Planning, icon::CALENDAR_DOTS, t!("Planlama", "Planning")),
                    (Page::Club, icon::BANK, t!("Kulüp", "Club")),
                    (Page::Squad, icon::USERS_THREE, t!("Kadro", "Squad")),
                    (Page::Tactics, icon::STRATEGY, t!("Taktik", "Tactics")),
                    (Page::Sell, icon::HANDSHAKE, t!("Transferler", "Transfers")),
                    (Page::Settings, icon::GEAR_SIX, t!("Ayarlar", "Settings")),
                ];
                for (p, ic, label) in items {
                    if theme::nav_item(ui, ic, label, self.page == p).clicked() {
                        self.page = p;
                    }
                    ui.add_space(2.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.label(theme::faint(format!(
                        "v{} · {} · Holyman3",
                        env!("CARGO_PKG_VERSION"),
                        t!("Türkçe", "English")
                    )));
                    if let Some((tag, url)) = self.latest_release.clone() {
                        ui.add_space(6.0);
                        if theme::primary_button(
                            ui,
                            icon::ARROW_CLOCKWISE,
                            &t!(format!("Yeni sürüm ({tag}) · İndir"), format!("New version ({tag}) · Download")),
                            true,
                        )
                        .clicked()
                        {
                            open_url(&url);
                        }
                    }
                    ui.add_space(8.0);
                    let sb_w = (ui.available_width() - 22.0).max(80.0);
                    Frame::new().fill(theme::SURFACE).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(10)).show(ui, |ui| {
                        ui.set_width(sb_w);
                        ui.set_max_width(sb_w);
                        ui.label(theme::caps(t!("İletişim", "Contact")));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let (logo_rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                            theme::paint_kick_logo(ui.painter(), logo_rect);
                            if ui.add(egui::Label::new(RichText::new("kick.com/holyman3").size(12.5).color(theme::KICK_GREEN)).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                open_url(KICK_URL);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(theme::ic(icon::ENVELOPE_SIMPLE).size(15.0).color(theme::ACCENT_HI));
                            if ui.add(egui::Label::new(RichText::new(CONTACT_EMAIL).size(12.0).color(theme::TEXT_DIM)).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                ui.ctx().copy_text(CONTACT_EMAIL.to_string());
                                open_url(&format!("mailto:{CONTACT_EMAIL}"));
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(theme::ic(icon::EXPORT).size(15.0).color(theme::ACCENT_HI));
                            if ui.add(egui::Label::new(RichText::new("GitHub · Holyman3").size(12.0).color(theme::TEXT_DIM)).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                open_url(GITHUB_REPO_URL);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    let sb_w2 = (ui.available_width() - 26.0).max(80.0);
                    Frame::new().fill(theme::SURFACE).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(12)).show(ui, |ui| {
                        ui.set_width(sb_w2);
                        ui.set_max_width(sb_w2);
                        ui.horizontal(|ui| {
                            let (c, txt) = if !agent::LIVE_SUPPORTED {
                                (theme::TEXT_FAINT, t!("Canlı: desteklenmiyor", "Live: unsupported"))
                            } else if self.live.connected {
                                (theme::SUCCESS, t!("FM27'ye Bağlı", "Connected to FM27"))
                            } else {
                                (theme::TEXT_FAINT, t!("Bağlı değil", "Not connected"))
                            };
                            theme::status_dot(ui, c);
                            ui.label(RichText::new(txt).size(13.0).color(theme::TEXT));
                        });
                        if let Some(n) = self.live.club_name() {
                            ui.label(theme::faint(n));
                        }
                    });
                });
            });
    }

    fn status_line(&self, ui: &mut Ui, msg: &str, error: bool) {
        if msg.is_empty() {
            return;
        }
        let (c, ic) = if error { (theme::DANGER, icon::WARNING) } else { (theme::INFO, icon::CHECK_CIRCLE) };
        let inner_w = (ui.available_width() - 26.0).max(80.0);
        Frame::new().fill(theme::tint(c, 0.12)).stroke(Stroke::new(1.0, theme::tint(c, 0.4))).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(inner_w);
            ui.set_max_width(inner_w);
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::ic(ic).color(c));
                ui.label(RichText::new(msg).color(theme::TEXT));
            });
        });
        ui.add_space(12.0);
    }

    /// Kaart met stappen om FM met de module te starten.
    fn connect_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        theme::card(ui, |ui| {
            if !agent::LIVE_SUPPORTED {
                ui.label(theme::h2(t!(
                    "FM içinde canlı bağlantı bu sistemde kullanılamıyor",
                    "Live connection inside FM is not available on this system"
                )));
                ui.label(theme::dim(t!(
                    "Okuma, planlama ve dışa aktarım çalışmaktadır.",
                    "Reading, planning and export work normally."
                )));
                return;
            }
            let windows = cfg!(windows);
            ui.horizontal(|ui| {
                ui.label(theme::ic(icon::PLUGS_CONNECTED).size(28.0).color(theme::ACCENT));
                ui.vertical(|ui| {
                    ui.label(theme::h2(t!("Football Manager'a Bağlan", "Connect to Football Manager")));
                    ui.add(
                        egui::Label::new(theme::dim(if windows {
                            t!(
                                "Uygulama FM27 çalışırken doğrudan okur. Hiçbir şey kurmanıza gerek yoktur.",
                                "The app reads directly while FM27 is running. No installation needed."
                            )
                        } else {
                            t!(
                                "Uygulama, FM ile birlikte başlayan küçük bir modül aracılığıyla doğrudan FM27 ile iletişim kurar.",
                                "The app talks directly to FM27 through a small module that starts with FM."
                            )
                        }))
                        .wrap(),
                    );
                });
            });
            ui.add_space(10.0);
            let steps = if windows {
                [
                    (
                        1,
                        t!(
                            "Bilgisayarınızda FM27'yi başlatın: Steam (veya aşağıdaki düğme), Xbox uygulaması / Game Pass veya Epic.",
                            "Start FM27 on your PC: via Steam (or the button below), the Xbox app / Game Pass or Epic."
                        ),
                    ),
                    (2, t!("Kayıtlı oyununuzu (save) yükleyin.", "Load your savegame.")),
                    (3, t!("Uygulama otomatik olarak bağlanır.", "The app connects automatically.")),
                ]
            } else {
                [
                    (1, t!("Oyununuzu kaydedin ve FM27'den çıkın.", "Save your game and quit FM27.")),
                    (
                        2,
                        t!(
                            "'FM'yi modül ile başlat' düğmesine tıklayın (Steam açık olmalı).",
                            "Click 'Start FM with module' (Steam must be open)."
                        ),
                    ),
                    (
                        3,
                        t!(
                            "Kaydınızı yükleyin. Uygulama otomatik olarak bağlanır.",
                            "Load your savegame. The app connects automatically."
                        ),
                    ),
                ]
            };
            for (n, s) in steps {
                ui.horizontal_wrapped(|ui| {
                    Frame::new().fill(theme::ACCENT_SOFT).corner_radius(CornerRadius::same(20)).inner_margin(Margin::symmetric(8, 2)).show(ui, |ui| {
                        ui.label(RichText::new(n.to_string()).color(theme::ACCENT).strong());
                    });
                    ui.label(s);
                });
            }
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                let start = if windows {
                    t!("FM27'yi Başlat", "Start FM27")
                } else {
                    t!("FM'yi modül ile başlat", "Start FM with module")
                };
                if theme::primary_button(ui, icon::PLAY, start, !self.live.busy()).clicked() {
                    self.live.start_fm(ctx);
                }
                if theme::button(ui, icon::ARROW_COUNTER_CLOCKWISE, t!("Yeniden Bağlan", "Reconnect"), !self.live.busy()).clicked() {
                    self.live.refresh(ctx, t!("bağlan", "connect"), None);
                }
                if self.live.busy() {
                    ui.spinner();
                }
            });
        });
    }

    // -------------------------------------------------------------------- overzicht

    fn page_overview(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let subtitle = if self.has_data() {
            t!(format!("Son okuma: {}", self.eff.source), format!("Last read: {}", self.eff.source))
        } else {
            t!(
                "FM27 belleğinden canlı taktiğinizi okuyun veya Taktiksel Etkinlik (Breakdown) ekranını okutun.",
                "Read your live tactic from FM27 memory or scan the Tactical Effectiveness (Breakdown) screen."
            )
            .to_string()
        };
        let busy = self.analysis_rx.is_some();
        let connected = self.live.connected;
        let mut read = false;
        let mut file = false;
        let mut sync_live = false;
        theme::page_header(ui, t!("Taktiksel Etkinlik", "Tactical Effectiveness"), &subtitle, |ui| {
            if theme::button(ui, icon::EXPORT, t!("Dosyadan", "From file"), !busy).clicked() {
                file = true;
            }
            if theme::button(ui, icon::CAMERA, t!("FM Ekranını Oku", "Read FM Screen"), !busy).clicked() {
                read = true;
            }
            if theme::primary_button(ui, icon::STRATEGY, t!("Taktikten Canlı Oku", "Read Live Tactic"), connected && !busy).clicked() {
                sync_live = true;
            }
            if busy {
                ui.spinner();
            }
        });
        if sync_live {
            self.sync_from_live_tactic(ctx);
        }
        if read {
            self.last_auto = std::time::Instant::now();
            self.start_analysis(ctx, None);
        }
        if file {
            self.show_file = !self.show_file;
        }
        ui.horizontal_wrapped(|ui| {
            if theme::toggle(ui, &mut self.settings.auto_read).changed() {
                self.last_auto = std::time::Instant::now() - std::time::Duration::from_secs(600);
                self.auto_note.clear();
                self.save_settings();
            }
            ui.label(theme::label_strong(t!("Otomatik Ekran Okuma", "Auto Screen Read")));
            if self.settings.auto_read {
                ui.label(theme::dim(t!("her", "every")));
                if ui.add(egui::DragValue::new(&mut self.settings.auto_read_secs).range(5..=120).suffix(t!(" sn", " s"))).changed() {
                    self.save_settings();
                }
                if !self.auto_note.is_empty() {
                    theme::pill(ui, &self.auto_note, theme::INFO);
                }
            } else {
                ui.label(theme::faint(t!(
                    "FM'ye bağlıyken 'Taktikten Canlı Oku' ile ekran görüntüsüne gerek kalmadan verileri doğrudan çekebilirsiniz.",
                    "While connected to FM, use 'Read Live Tactic' to pull data directly without taking a screenshot."
                )));
            }
        });
        ui.add_space(12.0);
        if self.show_file {
            theme::card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("Ekran Görüntüsü (PNG):", "Screenshot (PNG):"));
                    ui.add(egui::TextEdit::singleline(&mut self.image_path).hint_text("/…/breakdown.png").desired_width(360.0));
                    if theme::button(ui, "", t!("Oku", "Read"), !busy && !self.image_path.trim().is_empty()).clicked() {
                        let p = PathBuf::from(self.image_path.trim().trim_matches('\''));
                        self.start_analysis(ctx, Some(p));
                    }
                });
            });
            ui.add_space(10.0);
        }
        let (msg, err) = (self.status.clone(), self.status_error);
        self.status_line(ui, &msg, err);

        // onderdelen
        ui.columns(4, |cols| {
            for (i, c) in Component::ALL.iter().enumerate() {
                let v = self.eff.component(*c);
                let col = theme::rgb(level_color(v));
                let ic = [icon::TARGET, icon::SOCCER_BALL, icon::USERS_THREE, icon::EYE][i];
                theme::stat_tile(&mut cols[i], ic, c.label(), &format!("{v:.0}"), level_name(v), col, Some(v));
            }
        });
        ui.add_space(14.0);

        // factors per onderdeel
        let mut changed = false;
        ui.columns(4, |cols| {
            for (i, c) in Component::ALL.iter().enumerate() {
                let ui = &mut cols[i];
                theme::card(ui, |ui| {
                    ui.label(theme::caps(c.label()));
                    ui.add_space(6.0);
                    for f in Factor::ALL.iter().filter(|f| f.component() == *c) {
                        let v = self.eff.get(*f);
                        theme::metric_row(ui, f.label(), v, theme::rgb(level_color(v)));
                        if self.edit_mode {
                            let mut nv = v;
                            if ui.add(egui::Slider::new(&mut nv, 0.0..=100.0).show_value(false)).changed() && (nv - v).abs() > 0.01 {
                                self.eff.set(*f, nv);
                                changed = true;
                            }
                        }
                    }
                });
            }
        });
        if changed {
            self.eff.components.clear();
            if !self.eff.source.ends_with('*') {
                self.eff.source = format!("{} *", if self.eff.source.is_empty() { t!("elle", "manual") } else { &self.eff.source });
            }
            let _ = crate::save_effectiveness(&self.eff);
            self.week_plan = None;
            self.year.clear();
        }
        ui.add_space(4.0);
        ui.checkbox(&mut self.edit_mode, t!("Değerleri elle ayarla", "Adjust values manually"));
        ui.add_space(10.0);

        // prioriteiten
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::ic(icon::LIGHTNING).color(theme::WARNING).size(18.0));
                ui.label(theme::h2(t!("Öncelikler", "Priorities")));
            });
            ui.add_space(6.0);
            for (f, v) in self.eff.weakest().into_iter().take(6) {
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(250.0, 20.0), egui::Label::new(theme::label_strong(f.label())));
                    theme::pill(ui, &format!("{v:.0} · {}", level_name(v)), theme::rgb(level_color(v)));
                    ui.add(egui::Label::new(theme::dim(f.advice())).wrap());
                });
                ui.add_space(4.0);
            }
        });
    }

    // -------------------------------------------------------------------- training

    fn page_training(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let connected = agent::LIVE_SUPPORTED && self.live.connected;
        let can = connected && !self.live.busy();
        let (mut view, mut write, mut sync_and_write) = (false, false, false);
        theme::page_header(
            ui,
            t!("Antrenman", "Training"),
            t!(
                "FM27 Trainer antrenmanınızı taktiğinize ve en zayıf alanlarınıza göre planlar, FM takviminde sabit tutar.",
                "FM27 Trainer plans your training around your tactic and weakest areas, and keeps it locked in the FM calendar."
            ),
            |ui| {
                if theme::primary_button(ui, icon::LIGHTNING, t!("Antrenmanı Şimdi FM'ye Uygula", "Apply Training to FM Now"), can).clicked() {
                    write = true;
                }
                if theme::button(ui, icon::STRATEGY, t!("Taktikten Oku & Uygula", "Read Tactic & Apply"), can).clicked() {
                    sync_and_write = true;
                }
                if theme::button(ui, icon::EYE, t!("Planı Önizle", "Preview Plan"), can).clicked() {
                    view = true;
                }
                if self.live.busy() {
                    ui.spinner();
                }
            },
        );
        if sync_and_write {
            self.sync_from_live_tactic(ctx);
            self.live.training(ctx, &self.eff, &self.settings.planner, true, false);
        }
        if write {
            self.live.training(ctx, &self.eff, &self.settings.planner, true, false);
        }
        if view {
            self.live.training(ctx, &self.eff, &self.settings.planner, false, false);
        }
        let (m, e) = (self.live.train_status.clone(), self.live.train_error);
        self.status_line(ui, &m, e);
        // testhulp: FM27_TEAMS_JSON=<bestand> toont de keuze zonder verbinding
        let demo = std::env::var_os("FM27_TEAMS_JSON");
        if let (Some(p), None) = (&demo, &self.live.teams) {
            self.live.teams = std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok());
        }
        if connected || demo.is_some() {
            self.team_picker(ui, ctx);
        }

        // ---- Taktikle Uyumlu Antrenman Programı Seçici
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::ic(icon::STRATEGY).color(theme::ACCENT).size(20.0));
                ui.label(theme::h2(t!("Taktikle Uyumlu Antrenman Programı", "Tactic-Compatible Training Program")));
            });
            ui.add_space(6.0);
            let mut mode = self.settings.planner.tactic_mode;
            let opts: Vec<(planner::TacticMode, &str)> = planner::TacticMode::ALL.iter().map(|m| (*m, m.short_label())).collect();
            if theme::segmented(ui, &mut mode, &opts) {
                self.settings.planner.tactic_mode = mode;
                self.save_settings();
                self.week_plan = None;
                self.year.clear();
                if can {
                    self.live.training(ctx, &self.eff, &self.settings.planner, self.settings.auto_training, false);
                }
            }
            ui.add_space(6.0);
            ui.add(egui::Label::new(theme::dim(mode.description())).wrap());
        });
        ui.add_space(14.0);

        // ---- statusblok: automatisch bijhouden
        let auto = self.settings.auto_training;
        let accent = if auto && connected { theme::SUCCESS } else { theme::WARNING };
        let banner_w = (ui.available_width() - 38.0).max(80.0);
        Frame::new()
            .fill(theme::tint(accent, 0.10))
            .stroke(Stroke::new(1.0, theme::tint(accent, 0.45)))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.set_width(banner_w);
                ui.set_max_width(banner_w);
                ui.horizontal(|ui| {
                    ui.label(theme::ic(if auto { icon::CHECK_CIRCLE } else { icon::WARNING }).size(30.0).color(accent));
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(theme::h2(t!("Antrenmanı Otomatik Olarak Planda Tut", "Keep Training Automatically on Schedule")));
                            ui.add_space(8.0);
                            if theme::toggle(ui, &mut self.settings.auto_training).changed() {
                                self.save_settings();
                            }
                            ui.label(RichText::new(if auto { t!("AÇIK", "ON") } else { t!("KAPALI", "OFF") }).strong().color(accent));
                        });
                        let text = if !connected {
                            t!(
                                "FM çalıştığında ve bağlandığında aktif olur (aşağıya bakın).",
                                "Becomes active once FM is running and connected (see below)."
                            )
                        } else if auto {
                            t!(
                                "FM asistanı her hafta gelecek 1–2 haftayı yeniden planlar. FM27 Trainer her 8 saniyede bir kontrol eder ve değişiklikleri seçtiğiniz taktik antrenman planına geri döndürür. Oynarken uygulamayı açık tutun.",
                                "The FM assistant reschedules the next 1–2 weeks every week. FM27 Trainer checks every 8 seconds and restores any changes back to your tactical training plan. Keep the app open while playing."
                            )
                        } else {
                            t!(
                                "Kapalı: FM her hafta antrenmanınızın üzerine kendi planını yazar. Bunu açın veya birkaç oyun haftasında bir 'Antrenmanı Şimdi FM'ye Uygula'ya tıklayın.",
                                "Off: FM overwrites your training every week. Turn this on, or click 'Apply Training to FM Now' every few game weeks."
                            )
                        };
                        ui.add(egui::Label::new(theme::dim(text)).wrap());
                    });
                });
                if connected {
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 36.0;
                        let last = self.live.last_check.map(|t| t.format("%H:%M:%S").to_string()).unwrap_or_else(|| "—".into());
                        theme::mini_stat(ui, t!("Son Kontrol", "Last Check"), &last);
                        theme::mini_stat(
                            ui,
                            t!("Geri Yüklenen (Bu Oturum)", "Restored (This Session)"),
                            &t!(format!("{} gün", self.live.fixed_total), format!("{} days", self.live.fixed_total)),
                        );
                        let d = self.live.training.as_ref().and_then(|o| o.today).map(i18n::date).unwrap_or_else(|| "—".into());
                        theme::mini_stat(ui, t!("Oyun Tarihi", "Game Date"), &d);
                    });
                }
            });
        ui.add_space(14.0);

        // ---- zo werkt het: meten → plannen → bijhouden
        let today = self.live.training.as_ref().and_then(|o| o.today);
        let age_weeks = match (self.eff.game_date, today) {
            (Some(d), Some(t)) => Some((t - d).num_days() / 7),
            _ => None,
        };
        let stale = !self.has_data() || age_weeks.is_none_or(|w| w >= 3);
        let mut go_overview = false;
        ui.columns(3, |cols| {
            step_card(&mut cols[0], 1, icon::CAMERA, t!("Ölçüm", "Measure"), |ui| {
                let txt = if !self.has_data() {
                    t!("Henüz okunmadı.", "Not read yet.").to_string()
                } else {
                    match age_weeks {
                        Some(0) => t!("Bu hafta okundu (oyun zamanı).", "Read this week (game time).").to_string(),
                        Some(w) => t!(format!("{w} hafta önce okundu (oyun zamanı)."), format!("Read {w} weeks ago (game time).")),
                        None => t!(format!("Okundu: {}", self.eff.source), format!("Read: {}", self.eff.source)),
                    }
                };
                ui.label(if stale { RichText::new(txt).color(theme::WARNING) } else { theme::dim(txt) });
                ui.add(egui::Label::new(theme::faint(t!(
                    "'Taktikten Oku & Uygula' düğmesiyle veya Genel Bakış sekmesinden her 2–3 oyun haftasında bir verileri güncelleyin.",
                    "Refresh the data every 2–3 game weeks using 'Read Tactic & Apply' or from the Overview tab."
                ))).wrap());
                if theme::small_button(ui, t!("Genel Bakış'a Git", "Go to Overview"), true).clicked() {
                    go_overview = true;
                }
            });
            step_card(&mut cols[1], 2, icon::TARGET, t!("Planlama", "Plan"), |ui| {
                ui.label(theme::dim(t!("Ekstra odaklanılan alanlar:", "Extra focus areas:")));
                let mut areas: Vec<(Factor, f32)> = Factor::TACTICAL_AREAS.iter().map(|f| (*f, self.eff.get(*f))).collect();
                areas.sort_by(|a, b| (a.1 / self.settings.planner.tactic_mode.weight(a.0)).total_cmp(&(b.1 / self.settings.planner.tactic_mode.weight(b.0))));
                for (f, v) in areas.into_iter().take(3) {
                    theme::pill(ui, &format!("{} · {v:.0}", f.label()), theme::rgb(level_color(v)));
                }
                ui.add(egui::Label::new(theme::faint(t!(
                    "Maçlar etrafındaki ritim SI şablonlarını takip eder; odak seçtiğiniz taktik tarzına göre her hafta kayar.",
                    "The rhythm around matches follows SI templates; the focus shifts each week according to your tactical style."
                ))).wrap());
            });
            step_card(&mut cols[2], 3, icon::SPARKLE, t!("Koruma", "Keep"), |ui| {
                for l in [
                    t!("Yalnızca bugünden sonraki günler değiştirilir.", "Only days after today are changed."),
                    t!("Maç ve seyahat günleri FM'ye bırakılır.", "Match and travel days stay with FM."),
                    t!("Dinlenme haftaları (tatil) FM'ye bırakılır.", "Rest weeks (holidays) stay with FM."),
                    t!("FM içinde kaydetmek her şeyi kaydınızda saklar.", "Saving inside FM keeps everything in your save."),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(theme::ic(icon::CHECK_CIRCLE).color(theme::SUCCESS));
                        ui.add(egui::Label::new(theme::dim(l)).wrap());
                    });
                }
            });
        });
        if go_overview {
            self.page = Page::Overview;
        }
        ui.add_space(14.0);

        if !connected {
            self.connect_card(ui, ctx);
            return;
        }

        // ---- logboek
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::ic(icon::CALENDAR_DOTS).color(theme::INFO).size(16.0));
                ui.label(theme::h2(t!("Etkinlik Günlüğü", "Activity Log")));
            });
            ui.add_space(4.0);
            if self.live.log.is_empty() {
                ui.label(theme::faint(t!(
                    "Henüz değişiklik yok. FM bir haftayı yeniden planladığında neyin geri yüklendiğini burada göreceksiniz.",
                    "No changes yet. When FM reschedules a week, you will see what was restored here."
                )));
            }
            for e in self.live.log.iter().take(6) {
                ui.horizontal(|ui| {
                    ui.label(theme::faint(e.at.format("%H:%M:%S").to_string()));
                    theme::pill(
                        ui,
                        if e.auto { t!("otomatik", "auto") } else { t!("elle", "manual") },
                        if e.auto { theme::SUCCESS } else { theme::ACCENT },
                    );
                    ui.add(egui::Label::new(RichText::new(&e.text).color(theme::TEXT)).wrap());
                });
            }
        });
        ui.add_space(14.0);

        let Some(o) = self.live.training.clone() else {
            if !self.live.busy() {
                self.live.training(ctx, &self.eff, &self.settings.planner, false, false);
            }
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::h2(t!("Gelecek Haftalar", "Upcoming Weeks")));
            ui.add_space(10.0);
            legend(ui);
        });
        ui.add_space(6.0);
        for w in &o.weeks {
            week_card(ui, w, o.today, &self.live.fm_had);
            ui.add_space(10.0);
        }
    }

    // -------------------------------------------------------------------- planning

    /// Keuze van manager (bij meer dan één in de save) en van het team waarvan de training gepland
    /// wordt. Alleen zichtbaar als er iets te kiezen valt.
    fn team_picker(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let Some(data) = self.live.teams.clone() else { return };
        let list = |k: &str| data.get(k).and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let (managers, teams) = (list("managers"), list("teams"));
        if managers.len() < 2 && teams.len() < 2 {
            return;
        }
        let team_name = |t: &Value| -> String {
            let n = |k: &str| t.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
            let name = match (n("kind"), n("age_limit")) {
                (0, _) => "A Takım".to_string(),
                (_, a) if (1..100).contains(&a) => format!("{a} Yaş Altı"),
                _ => format!("Takım {}", n("index") + 1),
            };
            format!("{name} · {} oyuncu", n("players"))
        };
        let sel = |v: &Value| v.get("selected").and_then(|b| b.as_bool()) == Some(true);
        let mut pick: Option<(Option<u32>, Option<usize>)> = None;
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::ic(icon::USERS_THREE).color(theme::ACCENT));
                if managers.len() > 1 {
                    ui.label("Menajer:");
                    let cur = managers.iter().find(|m| sel(m)).and_then(|m| m["club"].as_str()).unwrap_or("?").to_string();
                    egui::ComboBox::from_id_salt("manager_pick").selected_text(cur).show_ui(ui, |ui| {
                        for m in &managers {
                            let uid = m["club_uid"].as_u64().unwrap_or(0) as u32;
                            if ui.selectable_label(sel(m), m["club"].as_str().unwrap_or("?")).clicked() && !sel(m) {
                                pick = Some((Some(uid), None));
                            }
                        }
                    });
                    ui.add_space(14.0);
                }
                if teams.len() > 1 {
                    ui.label("Antrenman Takımı:");
                    let cur = teams.iter().find(|x| sel(x)).map(&team_name).unwrap_or_else(|| "?".into());
                    egui::ComboBox::from_id_salt("team_pick").selected_text(cur).width(210.0).show_ui(ui, |ui| {
                        for x in &teams {
                            if ui.selectable_label(sel(x), team_name(x)).clicked() && !sel(x) {
                                let own = x.get("own").and_then(|b| b.as_bool()) == Some(true);
                                let idx = x["index"].as_u64().unwrap_or(0) as usize;
                                pick = Some((self.settings.manager_club, if own { None } else { Some(idx) }));
                            }
                        }
                    });
                }
            });
            if teams.iter().any(|x| sel(x) && x.get("own").and_then(|b| b.as_bool()) != Some(true)) {
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(theme::dim(
                        "Bu kendi takımınız değil. Uygulama yalnızca FM bu takım için ayrı bir takvim tutuyorsa (bu takımın antrenmanını siz yönetiyorsanız) antrenmanını planlayabilir. Plan en son okuduğunuz değerleri kullanır.",
                    ))
                    .wrap(),
                );
            } else if managers.len() > 1 {
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(theme::dim(
                        "Kulüp, kadro, transferler ve antrenman seçilen menajere aittir. Antrenmanı planlatmadan önce bu takımın verilerini tekrar okutun.",
                    ))
                    .wrap(),
                );
            }
        });
        ui.add_space(14.0);
        if let Some((club, team)) = pick {
            self.settings.manager_club = club;
            self.settings.training_team = team;
            self.save_settings();
            self.live.choose(ctx, (club, team));
        }
    }

    fn page_planning(&mut self, ui: &mut Ui) {
        let mut tab = self.plan_tab;
        theme::page_header(
            ui,
            t!("Planlama", "Planning"),
            t!(
                "Taktik tarzınıza göre haftalık ve sezonluk antrenman planı oluşturun ve dışa aktarın.",
                "Generate and export weekly and seasonal training plans tailored to your tactical style."
            ),
            |ui| {
                theme::segmented(ui, &mut tab, &[(PlanTab::Week, t!("Hafta", "Week")), (PlanTab::Season, t!("Sezon", "Season"))]);
            },
        );
        self.plan_tab = tab;
        let (m, e) = (self.status.clone(), self.status_error);
        self.status_line(ui, &m, e);

        theme::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::ic(icon::STRATEGY).color(theme::ACCENT).size(18.0));
                ui.label(theme::label_strong(t!("Taktik Tarzı:", "Tactical Style:")));
                let mut mode = self.settings.planner.tactic_mode;
                let opts: Vec<(planner::TacticMode, &str)> = planner::TacticMode::ALL.iter().map(|m| (*m, m.short_label())).collect();
                if theme::segmented(ui, &mut mode, &opts) {
                    self.settings.planner.tactic_mode = mode;
                    self.save_settings();
                    self.week_plan = None;
                    self.year.clear();
                }
            });
        });
        ui.add_space(10.0);

        match self.plan_tab {
            PlanTab::Week => self.planning_week(ui),
            PlanTab::Season => self.planning_season(ui),
        }
    }

    fn planning_week(&mut self, ui: &mut Ui) {
        theme::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(t!("Hafta başlangıcı", "Week start"));
                ui.add(egui::TextEdit::singleline(&mut self.week_start).desired_width(96.0));
                ui.separator();
                ui.label(t!("Maç günü", "Match day"));
                for i in 0..7 {
                    ui.toggle_value(&mut self.week_matches[i], i18n::weekday_short(i));
                }
                ui.separator();
                ui.checkbox(&mut self.prev_sunday, t!("önceki pazar maç var", "match previous Sunday"));
                ui.checkbox(&mut self.next_monday, t!("sonraki pazartesi maç var", "match next Monday"));
                kind_combo(ui, "weekkind", &mut self.week_kind);
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                // bij een testopname (FM27_SNAPSHOT) meteen een plan laten zien
                let auto = self.week_plan.is_none() && std::env::var("FM27_SNAPSHOT").is_ok();
                if theme::primary_button(ui, icon::SPARKLE, t!("Haftalık Plan Oluştur", "Generate Week Plan"), true).clicked() || auto {
                    let start = NaiveDate::parse_from_str(self.week_start.trim(), "%Y-%m-%d")
                        .map(planner::monday_of)
                        .unwrap_or_else(|_| planner::monday_of(chrono::Local::now().date_naive()));
                    let week = CalendarWeek { start, kind: self.week_kind, matches: self.week_matches, note: String::new() };
                    self.week_plan = Some(planner::plan_week(&week, self.prev_sunday, self.next_monday, &self.eff, &self.settings.planner, 1));
                }
                if let Some(p) = self.week_plan.clone() {
                    if theme::button(ui, icon::EXPORT, t!("Dışa Aktar & Aç", "Export & Open"), true).clicked() {
                        let r = export::write_all(&export::output_dir(), "weekplan", t!("FM27 Haftalık Plan", "FM27 Week Plan"), &self.eff, std::slice::from_ref(&p));
                        self.after_export(r);
                    }
                }
            });
        });
        ui.add_space(12.0);
        if let Some(p) = &self.week_plan {
            theme::card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::h2(&p.schedule_name));
                    theme::pill(
                        ui,
                        &t!(
                            format!("Odak: {}", crate::model::focus_tr(p.focus)),
                            format!("Focus: {}", crate::model::focus_tr(p.focus))
                        ),
                        theme::ACCENT,
                    );
                });
                ui.add_space(8.0);
                plan_grid(ui, p);
                if !p.notes.is_empty() {
                    ui.add_space(8.0);
                    ui.label(theme::caps(t!("Antrenman Dışı", "Outside Training")));
                    for n in &p.notes {
                        ui.add(egui::Label::new(theme::dim(format!("• {n}"))).wrap());
                    }
                }
            });
            ui.add_space(8.0);
            legend(ui);
        }
    }

    fn planning_season(&mut self, ui: &mut Ui) {
        theme::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(t!("Başlangıç", "Start"));
                ui.add(egui::TextEdit::singleline(&mut self.year_start).desired_width(96.0));
                ui.label(t!("Lig maçı", "League match"));
                weekday_combo(ui, "league", &mut self.cal_opts.league_day);
                ui.label(t!("Avrupa", "Europe"));
                let mut e = self.cal_opts.europe;
                egui::ComboBox::from_id_salt("europe")
                    .selected_text(match e {
                        None => t!("yok", "none"),
                        Some(Weekday::Tue) => t!("Sal (ŞL)", "Tue (CL)"),
                        Some(Weekday::Wed) => t!("Çar (ŞL)", "Wed (CL)"),
                        _ => t!("Per (AL/KL)", "Thu (EL/UECL)"),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut e, None, t!("yok", "none"));
                        ui.selectable_value(&mut e, Some(Weekday::Tue), t!("Sal (ŞL)", "Tue (CL)"));
                        ui.selectable_value(&mut e, Some(Weekday::Wed), t!("Çar (ŞL)", "Wed (CL)"));
                        ui.selectable_value(&mut e, Some(Weekday::Thu), t!("Per (AL/KL)", "Thu (EL/UECL)"));
                    });
                self.cal_opts.europe = e;
                ui.checkbox(&mut self.cal_opts.cup, t!("kupa", "cup"));
                ui.add(egui::DragValue::new(&mut self.cal_opts.weeks).range(4..=60).suffix(t!(" hafta", " weeks")));
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if theme::button(ui, icon::CALENDAR_BLANK, t!("Varsayılan Takvim", "Default Calendar"), true).clicked() {
                    if let Ok(d) = NaiveDate::parse_from_str(self.year_start.trim(), "%Y-%m-%d") {
                        self.cal_opts.start = d;
                    }
                    self.calendar = planner::default_calendar(&self.cal_opts);
                    self.year.clear();
                    self.save_calendar();
                }
                if theme::primary_button(ui, icon::SPARKLE, t!("Sezon Planı Oluştur", "Generate Season Plan"), true).clicked() {
                    self.year = planner::plan_year(&self.calendar, &self.eff, &self.settings.planner);
                    self.save_calendar();
                }
                if !self.year.is_empty() {
                    if theme::button(ui, icon::EXPORT, t!("Dışa Aktar & Aç", "Export & Open"), true).clicked() {
                        let r = export::write_all(&export::output_dir(), "seasonplan", t!("FM27 Sezon Planı", "FM27 Season Plan"), &self.eff, &self.year);
                        self.after_export(r);
                    }
                    let (n, u) = (self.year.len(), unique_schedules(&self.year).len());
                    ui.label(theme::faint(t!(
                        format!("{n} hafta · {u} farklı haftalık program"),
                        format!("{n} weeks · {u} distinct weekly schedules")
                    )));
                }
            });
        });
        ui.add_space(12.0);
        let mut edited = false;
        theme::card(ui, |ui| {
            egui::Grid::new("cal").num_columns(5).spacing([14.0, 8.0]).striped(true).show(ui, |ui| {
                for h in [
                    t!("Hafta", "Week"),
                    t!("Dönem", "Period"),
                    t!("Maçlar", "Matches"),
                    t!("Odak", "Focus"),
                    t!("Taktiksel", "Tactical"),
                ] {
                    ui.label(theme::caps(h));
                }
                ui.end_row();
                for (wi, week) in self.calendar.iter_mut().enumerate() {
                    ui.label(i18n::date(week.start));
                    let before = week.kind;
                    kind_combo(ui, &format!("k{wi}"), &mut week.kind);
                    edited |= week.kind != before;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for d in 0..7 {
                            if ui.toggle_value(&mut week.matches[d], i18n::weekday_initial(d)).changed() {
                                edited = true;
                            }
                        }
                        if !week.note.is_empty() {
                            ui.label(theme::faint(&week.note));
                        }
                    });
                    if let Some(p) = self.year.get(wi) {
                        theme::pill(ui, crate::model::focus_tr(p.focus), theme::ACCENT);
                        let t: Vec<String> = p.targets.iter().map(|(f, n)| format!("{}×{}", planner::short(*f), n)).collect();
                        ui.label(theme::faint(t.join("  ")));
                    } else {
                        ui.label("");
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        });
        if edited {
            self.year.clear();
            self.save_calendar();
        }
    }

    fn after_export(&mut self, res: Result<PathBuf>) {
        match res {
            Ok(p) => {
                open_path(&p);
                self.set_status(format!("Kaydedildi: {}", p.display()), false);
            }
            Err(e) => self.set_status(format!("{e:#}"), true),
        }
    }

    // -------------------------------------------------------------------- club

    fn page_club(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let busy = self.live.busy();
        let connected = self.live.connected;
        let name = self.live.club_name().unwrap_or_else(|| t!("Kulüp", "Club").into());
        let mut action: Option<(String, Value)> = None;
        let mut backup = false;
        theme::page_header(
            ui,
            &name,
            t!(
                "Finansı canlı olarak düzenleyin. Değişiklikler FM içinde kaydettiğinizde kalıcı olur.",
                "Edit finances live. Changes become permanent when you save inside FM."
            ),
            |ui| {
                if theme::button(ui, icon::ARROW_COUNTER_CLOCKWISE, t!("Geri Al", "Undo"), connected && !busy).clicked() {
                    action = Some((t!("geri al", "undo").into(), json!({"cmd": "undo"})));
                }
                if theme::button(ui, icon::FLOPPY_DISK, t!("Kayıtları Yedekle", "Backup Saves"), !busy).clicked() {
                    backup = true;
                }
            },
        );
        if backup {
            match agent::backup_saves() {
                Ok(d) => self.live.status = t!(format!("Yedek oluşturuldu: {}", d.display()), format!("Backup created: {}", d.display())),
                Err(e) => self.live.status = format!("{e:#}"),
            }
        }
        if let Some((l, r)) = action.take() {
            self.live.refresh(ctx, &l, Some(r));
        }
        let (m, e) = (self.live.status.clone(), self.live.status_error);
        self.status_line(ui, &m, e);
        if !connected {
            self.connect_card(ui, ctx);
            return;
        }
        let Some(c) = self.live.club.clone() else { return };
        let g = |k: &str| c.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
        let rate = self.settings.rate;
        let eur = |v: i64| format!("€ {}", money((v as f32 * rate) as i64));
        let gbp = |v: i64| format!("£ {}", money(v));
        ui.columns(3, |cols| {
            theme::stat_tile(&mut cols[0], icon::COINS, t!("Bakiye", "Balance"), &eur(g("balance")), &gbp(g("balance")), theme::INFO, None);
            theme::stat_tile(&mut cols[1], icon::BANK, t!("Transfer Bütçesi", "Transfer Budget"), &eur(g("transfer_budget")), &gbp(g("transfer_budget")), theme::ACCENT, None);
            theme::stat_tile(&mut cols[2], icon::USERS_THREE, t!("Haftalık Maaş Bütçesi", "Weekly Wage Budget"), &eur(g("wage_budget")), &gbp(g("wage_budget")), theme::SUCCESS, None);
        });
        ui.add_space(14.0);
        ui.columns(2, |cols| {
            theme::card(&mut cols[0], |ui| {
                ui.label(theme::h2(t!("Transfer Bütçesi", "Transfer Budget")));
                ui.label(theme::faint(t!("Milyon Euro cinsinden tutar", "Amount in million euros")));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.live.transfer_input).hint_text("50").desired_width(90.0));
                    ui.label(t!("Mn €", "M €"));
                    if theme::primary_button(ui, icon::CHECK_CIRCLE, t!("Ayarla", "Set"), !busy).clicked() {
                        if let Ok(m) = self.live.transfer_input.replace(',', ".").trim().parse::<f64>() {
                            action = Some((t!("transfer bütçesi", "transfer budget").into(), json!({"cmd": "set_finances", "transfer_budget": (m * 1e6 / rate as f64) as i64})));
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for add in [10, 25, 50, 100] {
                        if theme::small_button(ui, &format!("+€{add}M"), !busy).clicked() {
                            let v = g("transfer_budget") + (add as f64 * 1e6 / rate as f64) as i64;
                            action = Some((t!("transfer bütçesi", "transfer budget").into(), json!({"cmd": "set_finances", "transfer_budget": v})));
                        }
                    }
                });
            });
            theme::card(&mut cols[1], |ui| {
                ui.label(theme::h2(t!("Maaş Bütçesi", "Wage Budget")));
                ui.label(theme::faint(t!("Haftalık bin Euro cinsinden tutar", "Weekly amount in thousand euros")));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.live.wage_input).hint_text("900").desired_width(90.0));
                    ui.label(t!("B € / hf", "K € / wk"));
                    if theme::primary_button(ui, icon::CHECK_CIRCLE, t!("Ayarla", "Set"), !busy).clicked() {
                        if let Ok(k) = self.live.wage_input.replace(',', ".").trim().parse::<f64>() {
                            action = Some((t!("maaş bütçesi", "wage budget").into(), json!({"cmd": "set_finances", "wage_budget": (k * 1e3 / rate as f64) as i64})));
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for add in [50, 100, 250] {
                        if theme::small_button(ui, &format!("+€{add}{}", t!("B", "K")), !busy).clicked() {
                            let v = g("wage_budget") + (add as f64 * 1e3 / rate as f64) as i64;
                            action = Some((t!("maaş bütçesi", "wage budget").into(), json!({"cmd": "set_finances", "wage_budget": v})));
                        }
                    }
                });
            });
        });
        ui.add_space(8.0);
        ui.label(theme::faint(t!(
            format!("FM dahili olarak sterlin kullanır; Euro tutarları {rate:.2} kuru ile hesaplanır (Ayarlar'dan değiştirilebilir)."),
            format!("FM uses pounds internally; euro amounts are converted at {rate:.2} (changeable in Settings).")
        )));
        ui.add_space(12.0);
        let hf = self.live.human_flags.clone();
        theme::card(ui, |ui| {
            ui.label(theme::h2(t!("Oyun Kuralları", "Game Rules")));
            ui.add(egui::Label::new(theme::faint(t!(
                "FM oyun içi editöründeki anahtarların aynısı, doğrudan menajerinize uygulanır. FM'de turnuvanın kadro kayıt ekranından kontrol edebilirsiniz.",
                "Same switches as FM's in-game editor, applied directly to your manager. You can verify them on the competition squad registration screen in FM."
            ))).wrap());
            ui.add_space(6.0);
            match hf.as_ref().and_then(|v| v.get("supported")).and_then(|v| v.as_bool()) {
                Some(true) => {
                    let rows = [
                        (
                            "registration_anytime",
                            t!("Oyuncuları her zaman kaydet", "Allow squad registration anytime"),
                            t!(
                                "Kayıt dönemi dışında da kadro kaydı yapılabilsin (lig, UEFA).",
                                "Register players outside the registration window (league, UEFA)."
                            ),
                        ),
                        (
                            "numbers_anytime",
                            t!("Forma numaralarını her zaman değiştir", "Allow changing squad numbers anytime"),
                            t!(
                                "Turnuva forma numaralarını kilitlemiş olsa bile değiştirilebilsin.",
                                "Change shirt numbers even after a competition has locked them."
                            ),
                        ),
                    ];
                    for (key, label, hint) in rows {
                        let mut on = hf.as_ref().and_then(|v| v.get(key)).and_then(|v| v.as_bool()).unwrap_or(false);
                        ui.horizontal_wrapped(|ui| {
                            if theme::toggle(ui, &mut on).changed() && connected && !busy {
                                let mut m = serde_json::Map::new();
                                m.insert("cmd".into(), Value::String("set_human_flags".into()));
                                m.insert(key.into(), Value::Bool(on));
                                action = Some((label.to_string(), Value::Object(m)));
                            }
                            ui.label(theme::label_strong(label));
                            ui.label(theme::faint(hint));
                        });
                    }
                }
                Some(false) => {
                    ui.label(theme::faint(t!(
                        "Bu FM sürümünde veya platformda henüz mevcut değil.",
                        "Not yet available on this FM build or platform."
                    )));
                }
                None => {
                    ui.label(theme::faint(t!("Henüz okunmadı.", "Not read yet.")));
                }
            }
        });
        if let Some((l, r)) = action {
            self.live.refresh(ctx, &l, Some(r));
        }
    }

    // -------------------------------------------------------------------- selectie

    fn page_squad(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let busy = self.live.busy();
        let connected = self.live.connected;
        let injured = self.live.squad.iter().filter(|p| p.get("injuries").and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty())).count();
        let banned = self.live.squad.iter().filter(|p| p.get("bans").and_then(|v| v.as_u64()).unwrap_or(0) > 0).count();
        let mut action: Option<(String, Value)> = None;
        let mut sell: Option<u32> = None;
        let n = self.live.squad.len();
        let subtitle = t!(
            format!("{n} oyuncu · {injured} sakat · {banned} cezalı"),
            format!("{n} players · {injured} injured · {banned} suspended")
        );
        theme::page_header(ui, t!("Kadro", "Squad"), &subtitle, |ui| {
            if theme::button(ui, icon::SMILEY, t!("Herkesin Moralini Maksimum Yap", "Max Everyone's Morale"), connected && !busy).clicked() {
                action = Some((t!("moral", "morale").into(), json!({"cmd": "inspire", "morale": 20, "clear_unhappiness": true})));
            }
            if theme::button(ui, icon::WARNING, t!("Cezaları Kaldır", "Clear Suspensions"), connected && !busy && banned > 0).clicked() {
                action = Some((t!("cezalar", "suspensions").into(), json!({"cmd": "lift_bans"})));
            }
            let heal = t!(format!("Herkesi İyileştir ({injured})"), format!("Heal Everyone ({injured})"));
            if theme::primary_button(ui, icon::FIRST_AID, &heal, connected && !busy && injured > 0).clicked() {
                action = Some((t!("iyileştirme", "heal").into(), json!({"cmd": "heal"})));
            }
        });
        let (m, e) = (self.live.status.clone(), self.live.status_error);
        self.status_line(ui, &m, e);
        if !connected {
            self.connect_card(ui, ctx);
            return;
        }
        let tu = self.live.understanding.clone();
        let tu_players: std::collections::HashMap<u64, f32> = tu
            .as_ref()
            .and_then(|u| u.get("players"))
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let uid = p.get("uid")?.as_u64()?;
                        let levels: Vec<f32> = p.get("levels")?.as_array()?.iter().filter_map(|x| x.as_u64()).map(|x| x as f32 / 100.0).collect();
                        (!levels.is_empty()).then(|| (uid, levels.iter().sum::<f32>() / levels.len() as f32))
                    })
                    .collect()
            })
            .unwrap_or_default();
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(theme::h2(t!("Taktik Anlayışı", "Tactical Understanding")));
                    ui.add(
                        egui::Label::new(theme::faint(t!(
                            "Kadronun mevcut taktiğinizi alan bazında ne kadar iyi bildiği (oyuncuların ortalaması). Maksimuma çıkarma işlemi FM oyun içi editörünün yaptığını yapar: her şeyi %100'e çıkarır.",
                            "How well the squad knows your current tactic per area (player average). Maximising does what FM's in-game editor does: sets everything to 100%."
                        )))
                        .wrap(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if theme::primary_button(ui, icon::BRAIN, t!("Taktik Anlayışını Maksimum Yap", "Maximise Tactical Understanding"), connected && !busy).clicked() {
                        action = Some((t!("taktik anlayışı", "tactical understanding").into(), json!({"cmd": "max_understanding"})));
                    }
                });
            });
            match tu.as_ref().and_then(|u| u.get("team")).and_then(|v| v.as_array()) {
                Some(team) if !team.is_empty() => {
                    ui.add_space(6.0);
                    for row in team.iter().take(LIVE_UNDERSTANDING_FACTORS.len()).enumerate().collect::<Vec<_>>().chunks(4) {
                        ui.horizontal_wrapped(|ui| {
                            for (i, v) in row {
                                let pct = v.as_u64().unwrap_or(0) as f32 / 100.0;
                                theme::pill(ui, &format!("{} · {pct:.0}%", LIVE_UNDERSTANDING_FACTORS[*i].label()), theme::rgb(level_color(pct)));
                            }
                        });
                    }
                    let without = tu.as_ref().and_then(|u| u.get("without_record")).and_then(|v| v.as_u64()).unwrap_or(0);
                    if without > 0 {
                        ui.label(theme::faint(t!(
                            format!("{without} oyuncunun henüz taktik anlayışı kaydı yok (bu takımla henüz antrenman yapmadılar); bir haftalık antrenmandan sonra dahil olurlar."),
                            format!("{without} players do not have a tactical understanding record yet (they haven't trained with this team yet); they will be included after a week of training.")
                        )));
                    }
                }
                _ => {
                    ui.label(theme::faint(t!("Henüz okunmadı.", "Not read yet.")));
                }
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let keep = self.settings.keep_understanding;
                if theme::toggle(ui, &mut self.settings.keep_understanding).changed() {
                    self.save_settings();
                }
                ui.label(theme::label_strong(t!("%100'de Sabit Tut", "Keep Locked at 100%")));
                ui.label(RichText::new(if keep { t!("AÇIK", "ON") } else { t!("KAPALI", "OFF") }).strong().color(if keep { theme::SUCCESS } else { theme::TEXT_FAINT }));
                ui.label(theme::faint(t!(
                    "Uygulama açık olduğu sürece her 20 saniyede bir FM'nin düşürdüğü değerleri tekrar %100'e geri çeker.",
                    "While the app is open, restores any values lowered by FM back to 100% every 20 seconds."
                )));
            });
        });
        ui.add_space(12.0);
        theme::card(ui, |ui| {
            egui::Grid::new("squad").num_columns(8).spacing([18.0, 10.0]).striped(true).show(ui, |ui| {
                for h in [
                    t!("Oyuncu", "Player"),
                    t!("Moral", "Morale"),
                    t!("Kondisyon", "Condition"),
                    t!("Taktik", "Tactic"),
                    t!("Sakatlık", "Injury"),
                    t!("Ceza", "Ban"),
                    t!("Mutsuz", "Unhappy"),
                    "",
                ] {
                    ui.label(theme::caps(h));
                }
                ui.end_row();
                for p in &self.live.squad {
                    let uid = p.get("uid").and_then(|v| v.as_u64()).unwrap_or(0);
                    let m = p.get("morale").and_then(|v| v.as_u64()).unwrap_or(0);
                    ui.label(theme::label_strong(p.get("name").and_then(|v| v.as_str()).unwrap_or("?")));
                    theme::pill(ui, &format!("{m} · {}", morale_label(m)), theme::rgb(level_color((m as f32 - 1.0) / 19.0 * 100.0)));
                    let cond = p.get("condition").and_then(|v| v.as_u64()).unwrap_or(0) as f32 / 100.0;
                    ui.scope(|ui| {
                        ui.set_width(90.0);
                        theme::progress(ui, cond, 6.0, theme::rgb(level_color(cond)));
                    });
                    match tu_players.get(&uid) {
                        Some(pct) => {
                            theme::pill(ui, &format!("{pct:.0}%"), theme::rgb(level_color(*pct)));
                        }
                        None => {
                            ui.label(theme::faint("—"));
                        }
                    }
                    let inj: Vec<String> = p
                        .get("injuries")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().map(|i| i.get("name").and_then(|n| n.as_str()).unwrap_or(t!("sakatlık", "injury")).to_string()).collect())
                        .unwrap_or_default();
                    if inj.is_empty() {
                        ui.label(theme::faint("—"));
                    } else {
                        theme::pill(ui, &inj.join(", "), theme::DANGER);
                    }
                    let bans = p.get("bans").and_then(|v| v.as_u64()).unwrap_or(0);
                    if bans > 0 {
                        theme::pill(ui, t!("cezalı", "banned"), theme::WARNING);
                    } else {
                        ui.label(theme::faint("—"));
                    }
                    let unhappy = p.get("unhappy_reasons").and_then(|v| v.as_bool()).unwrap_or(false);
                    if unhappy {
                        theme::pill(ui, t!("evet", "yes"), theme::WARNING);
                    } else {
                        ui.label(theme::faint("—"));
                    }
                    ui.horizontal(|ui| {
                        if !inj.is_empty() && theme::small_button(ui, t!("İyileştir", "Heal"), !busy).clicked() {
                            action = Some((t!("iyileştirme", "heal").into(), json!({"cmd": "heal", "uids": [uid]})));
                        }
                        if (m < 20 || unhappy) && theme::small_button(ui, t!("Moral Ver", "Inspire"), !busy).clicked() {
                            action = Some((t!("moral", "morale").into(), json!({"cmd": "inspire", "uids": [uid], "morale": 20, "clear_unhappiness": true})));
                        }
                        let coach = p.get("player_coach").and_then(|v| v.as_bool()).unwrap_or(false);
                        if !coach && theme::small_button(ui, t!("Sat", "Sell"), !busy).clicked() {
                            sell = Some(uid as u32);
                        }
                    });
                    ui.end_row();
                }
            });
        });
        if let Some((l, r)) = action {
            self.live.refresh(ctx, &l, Some(r));
        }
        if let Some(uid) = sell {
            self.start_sale(uid);
        }
    }

    /// Naar de verkooppagina met deze speler gekozen; de vraagprijs begint bij zijn waarde.
    fn start_sale(&mut self, uid: u32) {
        let value = self.live.squad.iter().find(|p| p.get("uid").and_then(|v| v.as_u64()) == Some(uid as u64)).and_then(|p| p.get("value")).and_then(|v| v.as_i64()).unwrap_or(0);
        self.live.sell.player = Some(uid);
        self.live.sell.buy = false;
        self.live.sell.plan = None;
        self.live.sell.status.clear();
        self.live.sell.fee_input = format!("{:.1}", value as f64 * self.settings.rate as f64 / 1e6);
        self.page = Page::Sell;
    }

    // -------------------------------------------------------------------- verkopen

    // -------------------------------------------------------------------- tactiek

    fn page_tactics(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        use crate::tactics as tx;
        // testhulp: FM27_TACTICS_JSON=<bestand> toont de pagina met bewaarde gegevens, zonder FM
        let from_file = std::env::var_os("FM27_TACTICS_JSON");
        if let (Some(f), None) = (&from_file, &self.live.tactics) {
            self.live.tactics = std::fs::read_to_string(f).ok().and_then(|t| serde_json::from_str(&t).ok());
        }
        let connected = self.live.connected || (from_file.is_some() && self.live.tactics.is_some());
        let busy = self.live.tactics_busy();
        // bij het openen één keer vanzelf ophalen
        if connected && !self.tactics_asked && from_file.is_none() {
            self.tactics_asked = true;
            self.live.load_tactics(ctx);
        }
        if !connected {
            self.tactics_asked = false;
        }
        let mut reload = false;
        let mut apply_tactic_train = false;
        theme::page_header(
            ui,
            t!("Taktik", "Tactics"),
            t!(
                "Oyuncularınızın taktiğinizdeki rollere ne kadar iyi uyduğu ve taktikle uyumlu antrenman",
                "How well your players fit the roles in your tactic and tactic-compatible training"
            ),
            |ui| {
                if theme::primary_button(
                    ui,
                    icon::BARBELL,
                    t!("Bu Taktiğe Göre Antrenman Uygula", "Apply Training for This Tactic"),
                    connected && !busy,
                )
                .clicked()
                {
                    apply_tactic_train = true;
                }
                if theme::button(
                    ui,
                    icon::ARROWS_CLOCKWISE,
                    if busy { t!("Okunuyor…", "Reading…") } else { t!("Yeniden Oku", "Reload") },
                    connected && !busy,
                )
                .clicked()
                {
                    reload = true;
                }
            },
        );
        if apply_tactic_train {
            self.sync_from_live_tactic(ctx);
            self.live.training(ctx, &self.eff, &self.settings.planner, true, false);
            self.page = Page::Training;
        }
        if reload {
            self.live.load_tactics(ctx);
        }
        if !connected {
            self.connect_card(ui, ctx);
            return;
        }
        let err = self.live.tactics_error.clone();
        self.status_line(ui, &err, true);
        let Some(data) = self.live.tactics.clone() else {
            if err.is_empty() {
                theme::card(ui, |ui| {
                    ui.label(theme::faint(t!("Taktiğiniz okunuyor…", "Reading your tactic…")));
                });
            }
            return;
        };
        let cur_idx = data.current.min(data.tactics.len() - 1);
        if self.last_game_tactic != Some(cur_idx) {
            self.last_game_tactic = Some(cur_idx);
            self.tactic_pick = Some(cur_idx);
        }
        let pick = self.tactic_pick.filter(|i| *i < data.tactics.len()).unwrap_or(cur_idx);
        let tactic = &data.tactics[pick];
        let places = tx::places(tactic);
        let by_uid: std::collections::HashMap<u32, &tx::Player> = data.players.iter().map(|p| (p.uid, p)).collect();
        let in_eleven: std::collections::HashSet<u32> = places.iter().filter_map(|p| p.uid).collect();
        let mut order: Vec<usize> = (0..places.len()).collect();
        order.sort_by_key(|&i| places[i].oop_order);
        let fits: Vec<Option<f32>> = places.iter().map(|pl| pl.uid.and_then(|u| by_uid.get(&u)).map(|p| tx::fit(p, pl))).collect();
        let known: Vec<f32> = fits.iter().flatten().copied().collect();
        let avg = if known.is_empty() { 0.0 } else { known.iter().sum::<f32>() / known.len() as f32 };
        let weakest = order.iter().filter_map(|&i| fits[i].map(|f| (i, f))).min_by(|a, b| a.1.total_cmp(&b.1));
        let best = tx::best_eleven(&places, &data.players);
        let best_avg = if best.len() == places.len() { places.iter().zip(&best).map(|(pl, &j)| tx::fit(&data.players[j], pl)).sum::<f32>() / places.len() as f32 } else { avg };

        // welke tactiek
        let mut switched_to: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            let labels: Vec<String> = data
                .tactics
                .iter()
                .map(|t| {
                    let mut l = t!(format!("Taktik {}", t.index + 1), format!("Tactic {}", t.index + 1));
                    if t.index == data.tactics[cur_idx].index {
                        l.push_str(t!(" · kullanımda", " · active"));
                    }
                    l
                })
                .collect();
            let options: Vec<(usize, &str)> = labels.iter().enumerate().map(|(i, l)| (i, l.as_str())).collect();
            let mut v = pick;
            if theme::segmented(ui, &mut v, &options) {
                self.tactic_pick = Some(v);
                self.last_game_tactic = Some(v);
                switched_to = Some(v);
                theme::restart_enter(ctx);
            }
            if !tactic.style.is_empty() {
                theme::pill(ui, &t!(format!("şablon {}", tactic.style), format!("preset {}", tactic.style)), theme::TEXT_DIM);
            }
            let suggested = tx::suggest_tactic_mode(tactic);
            theme::pill(
                ui,
                &t!(
                    format!("Uyumlu Antrenman: {}", suggested.short_label()),
                    format!("Suggested Training: {}", suggested.short_label())
                ),
                theme::ACCENT,
            );
        });
        if let Some(v) = switched_to {
            self.live.select_tactic(ctx, v);
        }
        ui.add_space(12.0);

        ui.columns(3, |c| {
            theme::stat_tile(&mut c[0], icon::STRATEGY, t!("Mevcut 11", "Current XI"), &format!("{avg:.0}"), t!("ortalama uyum", "average fit"), theme::rgb(level_color(avg)), Some(avg));
            let gain = best_avg - avg;
            theme::stat_tile(
                &mut c[1],
                icon::USERS_THREE,
                t!("En İyi 11", "Best XI"),
                &format!("{best_avg:.0}"),
                &if gain >= 0.5 {
                    t!(format!("+{gain:.1} kazanılabilir"), format!("+{gain:.1} possible"))
                } else {
                    t!("ilk 11'iniz zaten en iyisi", "your XI is already the best").to_string()
                },
                theme::rgb(level_color(best_avg)),
                Some(best_avg),
            );
            let (wname, wval) = weakest
                .map(|(i, f)| (format!("{} · {}", places[i].line.label(places[i].side), places[i].uid.and_then(|u| by_uid.get(&u)).map(|p| p.name.as_str()).unwrap_or("?")), f))
                .unwrap_or((t!("ilk 11 seçilmedi", "no XI selected").to_string(), 0.0));
            theme::stat_tile(&mut c[2], icon::WARNING, t!("En Zayıf Mevki", "Weakest Slot"), &format!("{wval:.0}"), &wname, theme::rgb(level_color(wval)), Some(wval));
        });
        ui.add_space(14.0);

        // de elf plekken
        theme::card(ui, |ui| {
            ui.label(theme::h2(t!("Bu Taktikteki İlk 11'iniz", "Your Starting XI in This Tactic")));
            ui.add_space(6.0);
            egui::Grid::new("tactic_slots").num_columns(6).spacing([18.0, 10.0]).striped(true).show(ui, |ui| {
                for h in [
                    t!("Mevki", "Line"),
                    t!("Topla / Topsuz Rol", "IP / OOP Role"),
                    t!("Oyuncu", "Player"),
                    t!("Uyum", "Fit"),
                    t!("Pozisyon", "Position"),
                    t!("Kadrodaki Daha İyi Alternatif", "Better Option in Squad"),
                ] {
                    ui.label(theme::caps(h));
                }
                ui.end_row();
                for &i in &order {
                    let pl = &places[i];
                    ui.label(theme::label_strong(pl.line.label(pl.side)));
                    let (ic, iname) = tx::ip_label(pl.ip);
                    let (oc, oname) = tx::oop_label(pl.oop);
                    ui.horizontal(|ui| {
                        theme::pill(ui, &ic, theme::ACCENT_HI).on_hover_text(iname);
                        theme::pill(ui, &oc, theme::INFO).on_hover_text(oname);
                    });
                    let cur = pl.uid.and_then(|u| by_uid.get(&u));
                    match cur {
                        Some(p) => {
                            let f = tx::fit(p, pl);
                            let r = tx::position_rating(p, pl);
                            ui.horizontal(|ui| {
                                ui.label(theme::label_strong(&p.name));
                                if p.injured {
                                    theme::pill(ui, t!("sakat", "injured"), theme::DANGER);
                                }
                                if p.banned {
                                    theme::pill(ui, t!("cezalı", "banned"), theme::WARNING);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.scope(|ui| {
                                    ui.set_width(80.0);
                                    theme::progress(ui, f, 6.0, theme::rgb(level_color(f)));
                                });
                                ui.label(RichText::new(format!("{f:.0}")).color(theme::TEXT));
                            });
                            theme::pill(ui, tx::position_word(r), theme::rgb(level_color(r as f32 * 5.0)));
                            // de beste speler buiten de elf voor deze plek
                            let alt = data
                                .players
                                .iter()
                                .filter(|q| !in_eleven.contains(&q.uid) && !q.injured && !q.banned)
                                .map(|q| (q, tx::fit(q, pl)))
                                .max_by(|a, b| a.1.total_cmp(&b.1));
                            match alt {
                                Some((q, qf)) if qf >= f + 2.0 => {
                                    ui.label(RichText::new(format!("{} (+{:.0})", q.name, qf - f)).color(theme::SUCCESS));
                                }
                                _ => {
                                    ui.label(theme::faint("—"));
                                }
                            }
                        }
                        None => {
                            ui.label(theme::faint(t!("boş", "empty")));
                            ui.label(theme::faint("—"));
                            ui.label(theme::faint("—"));
                            ui.label(theme::faint("—"));
                        }
                    }
                    ui.end_row();
                }
            });
        });

        // de beste elf: alleen wat anders is dan nu
        theme::card(ui, |ui| {
            ui.label(theme::h2(t!("Bu Taktik İçin En İyi 11", "Best XI for This Tactic")));
            ui.add(
                egui::Label::new(theme::faint(t!(
                    "Müsait oyuncularınızın en yüksek toplam uyumu sağlayacak şekilde 11 mevkiye dağılımı. Sakat ve cezalı oyuncular dahil edilmez.",
                    "Assignment of your available players across the 11 slots for the highest total fit. Injured and suspended players are excluded."
                )))
                .wrap(),
            );
            ui.add_space(6.0);
            let mut changes = 0;
            if best.len() == places.len() {
                for &i in &order {
                    let pl = &places[i];
                    let b = &data.players[best[i]];
                    if pl.uid == Some(b.uid) {
                        continue;
                    }
                    changes += 1;
                    let now = pl.uid.and_then(|u| by_uid.get(&u));
                    ui.horizontal_wrapped(|ui| {
                        ui.scope(|ui| {
                            ui.set_width(90.0);
                            ui.label(theme::label_strong(pl.line.label(pl.side)));
                        });
                        ui.label(theme::dim(now.map(|p| p.name.as_str()).unwrap_or(t!("boş", "empty"))));
                        ui.label(theme::ic(icon::ARROW_RIGHT).color(theme::TEXT_FAINT));
                        ui.label(theme::label_strong(&b.name));
                        let (bf, nf) = (tx::fit(b, pl), now.map(|p| tx::fit(p, pl)));
                        theme::pill(ui, &match nf { Some(nf) => format!("{nf:.0} → {bf:.0}"), None => format!("{bf:.0}") }, theme::rgb(level_color(bf)));
                        if in_eleven.contains(&b.uid) {
                            ui.label(theme::faint(t!("(mevki değiştirir)", "(switches slot)")));
                        }
                    });
                }
            }
            if changes == 0 {
                ui.label(theme::dim(t!(
                    "Bu hesaplamaya göre mevcut ilk 11'iniz zaten en iyisi.",
                    "According to this calculation, your current starting XI is already the best."
                )));
            }
        });

        // een rol die beter bij de speler past
        let role_tips: Vec<(String, &'static str, &'static str, &'static str, f32, f32)> = order
            .iter()
            .filter_map(|&i| {
                let pl = &places[i];
                let p = pl.uid.and_then(|u| by_uid.get(&u))?;
                let (r, now, then) = tx::better_role(p, pl)?;
                Some((p.name.clone(), tx::ip_role(pl.ip?)?.code_str(), r.code_str(), r.name_str(), now, then))
            })
            .collect();
        theme::card(ui, |ui| {
            ui.label(theme::h2(t!("Daha İyi Uyan Roller", "Better Fitting Roles")));
            ui.add(
                egui::Label::new(theme::faint(t!(
                    "Aynı hattaki oyuncunun özelliklerine belirgin şekilde daha iyi uyan başka bir topla oynama rolü.",
                    "Another in-possession role on the same line that fits the player's attributes noticeably better."
                )))
                .wrap(),
            );
            ui.add_space(6.0);
            if role_tips.is_empty() {
                ui.label(theme::dim(t!("Yok: her oyuncu kendisine uygun bir rolde oynuyor.", "None: every player is in a well-suited role.")));
            }
            for (name, cur, code, full, now, then) in &role_tips {
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::label_strong(name));
                    theme::pill(ui, &format!("{cur} {now:.0}"), theme::TEXT_DIM);
                    ui.label(theme::ic(icon::ARROW_RIGHT).color(theme::TEXT_FAINT));
                    theme::pill(ui, &format!("{code} {then:.0}"), theme::SUCCESS).on_hover_text(*full);
                    ui.label(theme::faint(*full));
                });
            }
        });

        ui.add(
            egui::Label::new(theme::faint(t!(
                "Roller, mevkiler ve özellikler FM'den alınır. Hangi özelliklerin hangi role ait olduğu bizim tahminimizdir ve sol/sağ bilgisi mevkilerin sırasından türetilmiştir: puanı FM'nin resmi bir verisi değil, bir rehber olarak görün. FM içinde hiçbir şey değiştirilmez.",
                "Roles, positions and attributes are read from FM. Which attributes belong to which role is our estimate, and left/right is derived from slot order: treat the score as a guide rather than official FM data. Nothing is changed inside FM."
            )))
            .wrap(),
        );
    }

    fn page_sell(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let busy = self.live.busy() || self.live.sell_busy();
        let connected = self.live.connected;
        let rate = self.settings.rate as f64;
        let mut undo = false;
        let buying = self.live.sell.buy;
        theme::page_header(
            ui,
            t!("Transferler", "Transfers"),
            if buying {
                t!(
                    "Başka bir kulüpten bir oyuncuyu doğrudan A takımınıza getirin. Bonservis bedelini siz ödersiniz.",
                    "Bring a player from another club directly into your first team. You pay the transfer fee."
                )
            } else {
                t!(
                    "Bir oyuncuyu istediğiniz bir kulübe doğrudan satın. Bonservis bedeli anında hesaba işlenir.",
                    "Sell a player directly to any club. The transfer fee is credited immediately."
                )
            },
            |ui| {
                if theme::button(ui, icon::ARROW_COUNTER_CLOCKWISE, t!("Geri Al", "Undo"), connected && !busy).clicked() {
                    undo = true;
                }
            },
        );
        if undo {
            self.live.sell.plan = None;
            self.live.refresh(ctx, t!("geri al", "undo"), Some(json!({"cmd": "undo"})));
        }
        let (m, e) = (self.live.status.clone(), self.live.status_error);
        if !m.is_empty() && (e || undo) {
            self.status_line(ui, &m, e);
        }
        let (m, e) = (self.live.sell.status.clone(), self.live.sell.error);
        self.status_line(ui, &m, e);
        if !connected {
            self.connect_card(ui, ctx);
            return;
        }
        let mut buy = self.live.sell.buy;
        if theme::segmented(ui, &mut buy, &[(false, t!("Sat", "Sell")), (true, t!("Satın Al", "Buy"))]) {
            self.live.sell.buy = buy;
            self.live.sell.plan = None;
            self.live.sell.status.clear();
        }
        ui.add_space(10.0);
        if self.live.sell.buy {
            self.page_buy(ui, ctx);
            return;
        }

        let eur = |pounds: i64| format!("€ {}", money((pounds as f64 * rate) as i64));
        let squad = self.live.squad.clone();
        let chosen = self.live.sell.player.and_then(|uid| squad.iter().find(|p| p.get("uid").and_then(|v| v.as_u64()) == Some(uid as u64)).cloned());
        let mut pick: Option<u32> = None;
        let mut search = false;
        let mut check: Option<(u32, u32, i64, Option<i64>)> = None;
        let mut confirm: Option<(u32, u32, i64, Option<i64>)> = None;

        ui.columns(2, |cols| {
            // 1. speler
            theme::card(&mut cols[0], |ui| {
                ui.label(theme::h2(t!("1 · Oyuncu", "1 · Player")));
                ui.label(theme::faint(t!("Satmak istediğiniz oyuncuyu seçin.", "Select the player you want to sell.")));
                ui.add_space(6.0);
                ScrollArea::vertical().id_salt("sell_players").max_height(430.0).show(ui, |ui| {
                    let mut sorted = squad.clone();
                    sorted.sort_by_key(|p| std::cmp::Reverse(p.get("value").and_then(|v| v.as_i64()).unwrap_or(0)));
                    for p in &sorted {
                        let uid = p.get("uid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let coach = p.get("player_coach").and_then(|v| v.as_bool()).unwrap_or(false);
                        let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let value = p.get("value").and_then(|v| v.as_i64()).unwrap_or(0);
                        let until = p.get("contract_ends").and_then(|d| d.get("year")).and_then(|y| y.as_u64());
                        let selected = self.live.sell.player == Some(uid);
                        ui.horizontal(|ui| {
                            let label = if coach {
                                t!(format!("{name} · oyuncu-antrenör"), format!("{name} · player-coach"))
                            } else {
                                name.to_string()
                            };
                            let r = ui.add_enabled(!coach, egui::SelectableLabel::new(selected, RichText::new(label).size(14.0)));
                            if r.clicked() {
                                pick = Some(uid);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if let Some(y) = until {
                                    ui.label(theme::faint(t!(format!("bitiş: {y}"), format!("until {y}"))));
                                }
                                ui.label(RichText::new(eur(value)).color(theme::TEXT));
                                if p.get("listed").and_then(|v| v.as_u64()).unwrap_or(0) & 1 == 1 {
                                    theme::pill(ui, t!("transfer listesinde", "transfer listed"), theme::WARNING);
                                }
                            });
                        });
                    }
                });
            });

            // 2. koper en 3. bedrag
            let ui = &mut cols[1];
            theme::card(ui, |ui| {
                ui.label(theme::h2(t!("2 · Alıcı Kulüp", "2 · Buying Club")));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let w = (ui.available_width() - 75.0).max(120.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.live.sell.query)
                            .hint_text(t!("Bir kulüp arayın, örn. AC Milan", "Search a club, e.g. AC Milan"))
                            .desired_width(w),
                    );
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        search = true;
                    }
                    if theme::small_button(ui, t!("Ara", "Search"), !busy && self.live.sell.query.trim().len() >= 2).clicked() {
                        search = true;
                    }
                });
                let q = self.live.sell.query.trim().to_string();
                if q.len() >= 3 && q != self.live.sell.searched && !self.live.sell_busy() {
                    search = true;
                }
                ui.add_space(4.0);
                let clubs = self.live.sell.clubs.clone();
                for c in clubs.iter().take(8) {
                    let uid = c.get("uid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                    let selected = self.live.sell.club.as_ref().is_some_and(|(u, _)| *u == uid);
                    ui.horizontal(|ui| {
                        if ui.add(egui::SelectableLabel::new(selected, RichText::new(&name).size(14.0))).clicked() {
                            self.live.sell.club = Some((uid, name.clone()));
                            self.live.sell.plan = None;
                        }
                        if c.get("women").and_then(|v| v.as_bool()).unwrap_or(false) {
                            theme::pill(ui, t!("kadınlar", "women"), theme::INFO);
                        }
                    });
                }
                if clubs.is_empty() && !self.live.sell.searched.is_empty() && !self.live.sell_busy() {
                    ui.label(theme::faint(t!("Kulüp bulunamadı.", "No clubs found.")));
                }
            });
            ui.add_space(12.0);
            theme::card(ui, |ui| {
                ui.label(theme::h2(t!("3 · Bonservis Bedeli", "3 · Transfer Fee")));
                ui.add_space(4.0);
                let value = chosen.as_ref().and_then(|p| p.get("value")).and_then(|v| v.as_i64()).unwrap_or(0);
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.live.sell.fee_input).hint_text("10").desired_width(90.0));
                    ui.label(t!("Mn €", "M €"));
                    for (label, f) in [(t!("değeri", "value"), 1.0), ("×1.5", 1.5), ("×2", 2.0), ("×3", 3.0)] {
                        if theme::small_button(ui, label, value > 0).clicked() {
                            self.live.sell.fee_input = format!("{:.1}", value as f64 * f * rate / 1e6);
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("Yeni maaş", "New wage"));
                    ui.add(egui::TextEdit::singleline(&mut self.live.sell.wage_input).hint_text(t!("değişmez", "unchanged")).desired_width(90.0));
                    ui.label(t!("B € / hf", "K € / wk"));
                });
                let fee = self.live.sell.fee_input.replace(',', ".").trim().parse::<f64>().ok().map(|m| (m * 1e6 / rate).round() as i64);
                let wage = self.live.sell.wage_input.replace(',', ".").trim().parse::<f64>().ok().map(|k| (k * 1e3 / rate).round() as i64);
                let key = match (self.live.sell.player, &self.live.sell.club, fee) {
                    (Some(p), Some((c, _)), Some(f)) if f >= 0 => Some((p, *c, f, wage)),
                    _ => None,
                };
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if theme::button(ui, icon::MAGNIFYING_GLASS, t!("Kontrol Et", "Verify"), key.is_some() && !busy).clicked() {
                        check = key;
                    }
                    let ready = self.live.sell.plan.as_ref().is_some_and(|(v, k)| Some(*k) == key && v.get("allowed").and_then(|a| a.as_bool()) == Some(true));
                    let label = match (&chosen, &self.live.sell.club) {
                        (Some(p), Some((_, c))) => t!(
                            format!("{} oyuncusunu {} kulübüne sat", p.get("name").and_then(|v| v.as_str()).unwrap_or("?"), c),
                            format!("Sell {} to {}", p.get("name").and_then(|v| v.as_str()).unwrap_or("?"), c)
                        ),
                        _ => t!("Sat", "Sell").to_string(),
                    };
                    if theme::primary_button(ui, icon::HANDSHAKE, &label, ready && !busy).clicked() {
                        confirm = key;
                    }
                    if self.live.sell_busy() {
                        ui.spinner();
                    }
                });
                if let Some((v, k)) = &self.live.sell.plan {
                    if Some(*k) == key {
                        ui.add_space(8.0);
                        let s = |n: &str| v.get(n).and_then(|x| x.as_str()).unwrap_or("?").to_string();
                        let i = |n: &str| v.get(n).and_then(|x| x.as_i64()).unwrap_or(0);
                        let b = |n: &str| v.get(n).and_then(|x| x.as_bool()).unwrap_or(false);
                        let mut lines = vec![format!("{} oyuncusu {} karşılığında {} kulübüne gidiyor.", s("player"), eur(i("fee")), s("to"))];
                        lines.push(if b("buyer_pays") {
                            format!("{} kendi bakiyesinden ve transfer bütçesinden öder.", s("to"))
                        } else {
                            format!("{} kulübünün kaydınızda tam finans verisi yok: parayı yalnızca siz alırsınız.", s("to"))
                        });
                        lines.push(format!("Bakiyeniz {} ve transfer bütçeniz {} olur.", eur(i("balance_after")), eur(i("transfer_budget_after"))));
                        if i("selections") > 0 {
                            lines.push(format!("{} kadro seçimindeki yeri boşaltılır.", i("selections")));
                        }
                        if b("was_captain") {
                            lines.push("Takım kaptanıydı: kaptanlık pazubandı boşa çıkar.".into());
                        }
                        if !b("allowed") {
                            lines.push("Bu FM sürümü test edilmedi: önce Ayarlar'dan değişikliklere izin verin.".into());
                        }
                        for l in lines {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(theme::ic(icon::CHECK_CIRCLE).color(theme::SUCCESS));
                                ui.label(l);
                            });
                        }
                    }
                }
            });
        });
        ui.add_space(12.0);
        theme::card(ui, |ui| {
            ui.label(theme::h2("Bilmekte Fayda Var"));
            ui.add_space(4.0);
            for l in [
                "Bu bir teklif değil, doğrudan satıştır: FM transferi haberlere veya transfer geçmişine eklemez.",
                "Oyuncu alıcı kulüpte bugünden itibaren aynı sözleşmeyi (veya yeni maaşı) forma numarası olmadan alır.",
                "Oyuna devam etmediğiniz sürece Geri Al her şeyi eski haline döndürür. Önce yedek alın ve yalnızca her şey doğruysa kaydedin.",
                "Oyuncu-antrenörler ve kiralık oyuncular (henüz) satılamaz.",
            ] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::ic(icon::INFO).color(theme::INFO));
                    ui.label(theme::dim(l));
                });
            }
        });

        if let Some(uid) = pick {
            self.start_sale(uid);
        }
        if search {
            self.live.search_clubs(ctx);
        }
        if let Some((p, c, f, w)) = check {
            self.live.plan_sale(ctx, p, c, f, w);
        }
        if let Some((p, c, f, w)) = confirm {
            self.live.sell_now(ctx, p, c, f, w);
        }
    }

    /// Kopen: een speler van een andere club naar je eerste elftal halen.
    fn page_buy(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let busy = self.live.busy() || self.live.sell_busy();
        let rate = self.settings.rate as f64;
        let eur = |pounds: i64| format!("€ {}", money((pounds as f64 * rate) as i64));
        let club = self.live.sell.club.clone();
        let squad = self.live.sell.buy_squad.clone();
        let chosen = self.live.sell.buy_player.and_then(|uid| squad.iter().find(|p| p.get("uid").and_then(|v| v.as_u64()) == Some(uid as u64)).cloned());
        let mut search = false;
        let mut load: Option<u32> = None;
        let mut pick: Option<(u32, i64, Option<i64>)> = None;
        let mut check: Option<(u32, u32, i64, Option<i64>)> = None;
        let mut confirm: Option<(u32, u32, i64, Option<i64>)> = None;

        ui.columns(2, |cols| {
            // 1. club
            theme::card(&mut cols[0], |ui| {
                ui.label(theme::h2(t!("1 · Kulüp", "1 · Club")));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let w = (ui.available_width() - 75.0).max(120.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.live.sell.query)
                            .hint_text(t!("Bir kulüp arayın, örn. Real Madrid", "Search a club, e.g. Real Madrid"))
                            .desired_width(w),
                    );
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        search = true;
                    }
                    if theme::small_button(ui, t!("Ara", "Search"), !busy && self.live.sell.query.trim().len() >= 2).clicked() {
                        search = true;
                    }
                });
                let q = self.live.sell.query.trim().to_string();
                if q.len() >= 3 && q != self.live.sell.searched && !self.live.sell_busy() {
                    search = true;
                }
                ui.add_space(4.0);
                let clubs = self.live.sell.clubs.clone();
                for c in clubs.iter().take(8) {
                    let uid = c.get("uid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                    let selected = self.live.sell.club.as_ref().is_some_and(|(u, _)| *u == uid);
                    ui.horizontal(|ui| {
                        if ui.add(egui::SelectableLabel::new(selected, RichText::new(&name).size(14.0))).clicked() {
                            self.live.sell.club = Some((uid, name.clone()));
                            self.live.sell.plan = None;
                            load = Some(uid);
                        }
                        if c.get("women").and_then(|v| v.as_bool()).unwrap_or(false) {
                            theme::pill(ui, t!("kadınlar", "women"), theme::INFO);
                        }
                    });
                }
                if clubs.is_empty() && !self.live.sell.searched.is_empty() && !self.live.sell_busy() {
                    ui.label(theme::faint(t!("Kulüp bulunamadı.", "No clubs found.")));
                }
            });
            cols[0].add_space(12.0);
            // 2. speler
            theme::card(&mut cols[0], |ui| {
                ui.label(theme::h2(t!("2 · Oyuncu", "2 · Player")));
                match &club {
                    None => {
                        ui.label(theme::faint(t!("Önce bir kulüp seçin.", "Select a club first.")));
                    }
                    Some((_, name)) => {
                        ui.label(theme::faint(t!(
                            format!("{name} A takımı, en değerliler başta."),
                            format!("{name} first team, highest value first.")
                        )));
                        if squad.is_empty() && self.live.sell_busy() {
                            ui.spinner();
                        }
                        ui.add_space(6.0);
                        ScrollArea::vertical().id_salt("buy_players").max_height(360.0).show(ui, |ui| {
                            for p in &squad {
                                let uid = p.get("uid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                let coach = p.get("player_coach").and_then(|v| v.as_bool()).unwrap_or(false);
                                let pname = p.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let value = p.get("value").and_then(|v| v.as_i64()).unwrap_or(0);
                                let wage = p.get("wage").and_then(|v| v.as_i64());
                                let until = p.get("contract_ends").and_then(|d| d.get("year")).and_then(|y| y.as_u64());
                                let selected = self.live.sell.buy_player == Some(uid);
                                ui.horizontal(|ui| {
                                    let label = if coach {
                                        t!(format!("{pname} · oyuncu-antrenör"), format!("{pname} · player-coach"))
                                    } else {
                                        pname.to_string()
                                    };
                                    if ui.add_enabled(!coach, egui::SelectableLabel::new(selected, RichText::new(label).size(14.0))).clicked() {
                                        pick = Some((uid, value, wage));
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if let Some(y) = until {
                                            ui.label(theme::faint(t!(format!("bitiş: {y}"), format!("until {y}"))));
                                        }
                                        if let Some(w) = wage {
                                            ui.label(theme::faint(format!("{} / {}", eur(w), t!("hf", "wk"))));
                                        }
                                        ui.label(RichText::new(eur(value)).color(theme::TEXT));
                                    });
                                });
                            }
                        });
                    }
                }
            });

            // 3. bedrag
            let ui = &mut cols[1];
            theme::card(ui, |ui| {
                ui.label(theme::h2(t!("3 · Bonservis Bedeli", "3 · Transfer Fee")));
                ui.add_space(4.0);
                let value = chosen.as_ref().and_then(|p| p.get("value")).and_then(|v| v.as_i64()).unwrap_or(0);
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.live.sell.fee_input).hint_text("10").desired_width(90.0));
                    ui.label(t!("Mn €", "M €"));
                    for (label, f) in [(t!("değeri", "value"), 1.0), ("×0.5", 0.5), ("×1.5", 1.5), ("0", 0.0)] {
                        if theme::small_button(ui, label, value > 0).clicked() {
                            self.live.sell.fee_input = format!("{:.1}", value as f64 * f * rate / 1e6);
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("Sizdeki maaşı", "Wage at your club"));
                    ui.add(egui::TextEdit::singleline(&mut self.live.sell.wage_input).hint_text(t!("değişmez", "unchanged")).desired_width(90.0));
                    ui.label(t!("B € / hf", "K € / wk"));
                });
                let fee = self.live.sell.fee_input.replace(',', ".").trim().parse::<f64>().ok().map(|m| (m * 1e6 / rate).round() as i64);
                let wage = self.live.sell.wage_input.replace(',', ".").trim().parse::<f64>().ok().map(|k| (k * 1e3 / rate).round() as i64);
                let key = match (self.live.sell.buy_player, &self.live.sell.club, fee) {
                    (Some(p), Some((c, _)), Some(f)) if f >= 0 => Some((p, *c, f, wage)),
                    _ => None,
                };
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if theme::button(ui, icon::MAGNIFYING_GLASS, t!("Kontrol Et", "Verify"), key.is_some() && !busy).clicked() {
                        check = key;
                    }
                    let ready = self.live.sell.plan.as_ref().is_some_and(|(v, k)| Some(*k) == key && v.get("allowed").and_then(|a| a.as_bool()) == Some(true));
                    let label = match (&chosen, &club) {
                        (Some(p), Some((_, c))) => t!(
                            format!("{} oyuncusunu {} kulübünden satın al", p.get("name").and_then(|v| v.as_str()).unwrap_or("?"), c),
                            format!("Buy {} from {}", p.get("name").and_then(|v| v.as_str()).unwrap_or("?"), c)
                        ),
                        _ => t!("Satın Al", "Buy").to_string(),
                    };
                    if theme::primary_button(ui, icon::USER_PLUS, &label, ready && !busy).clicked() {
                        confirm = key;
                    }
                    if self.live.sell_busy() {
                        ui.spinner();
                    }
                });
                if let Some((v, k)) = &self.live.sell.plan {
                    if Some(*k) == key {
                        ui.add_space(8.0);
                        let s = |n: &str| v.get(n).and_then(|x| x.as_str()).unwrap_or("?").to_string();
                        let i = |n: &str| v.get(n).and_then(|x| x.as_i64()).unwrap_or(0);
                        let b = |n: &str| v.get(n).and_then(|x| x.as_bool()).unwrap_or(false);
                        let mut lines = vec![format!("{} oyuncusu {} karşılığında {} kulübünden A takımınıza geliyor.", s("player"), eur(i("fee")), s("from"))];
                        lines.push(if b("buyer_pays") {
                            format!("{} parayı bakiyesine ve transfer bütçesine alır.", s("from"))
                        } else {
                            format!("{} kulübünün kaydınızda tam finans verisi yok: yalnızca siz ödersiniz.", s("from"))
                        });
                        lines.push(format!("Bakiyeniz {} ve transfer bütçeniz {} olur.", eur(i("balance_after")), eur(i("transfer_budget_after"))));
                        if b("was_captain") {
                            lines.push(format!("{} kulübünde kaptandı; kaptanlık pazubandı orada boşa çıkar.", s("from")));
                        }
                        if !b("allowed") {
                            lines.push("Bu FM sürümü test edilmedi: önce Ayarlar'dan değişikliklere izin verin.".into());
                        }
                        for l in lines {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(theme::ic(icon::CHECK_CIRCLE).color(theme::SUCCESS));
                                ui.label(l);
                            });
                        }
                    }
                }
            });
        });
        ui.add_space(12.0);
        theme::card(ui, |ui| {
            ui.label(theme::h2("Bilmekte Fayda Var"));
            ui.add_space(4.0);
            for l in [
                "Bu bir teklif değil, doğrudan transferdir: FM transferi haberlere veya transfer geçmişine eklemez.",
                "Oyuncu bugünden itibaren kulübünüzde mevcut sözleşmesini (veya yeni maaşını) forma numarası olmadan sürdürür. FM içinde ona bir forma numarası verin ve gereken yerlere kaydedin.",
                "Oyuna devam etmediğiniz sürece Geri Al her şeyi eski haline döndürür. Önce yedek alın ve yalnızca her şey doğruysa kaydedin.",
                "Oyuncu-antrenörler ve kiralık oyuncular (henüz) transfer edilemez.",
            ] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::ic(icon::INFO).color(theme::INFO));
                    ui.label(theme::dim(l));
                });
            }
        });
        if search {
            self.live.search_clubs(ctx);
        }
        if let Some(uid) = load {
            self.live.load_club_squad(ctx, uid);
        }
        if let Some((uid, value, wage)) = pick {
            self.live.sell.buy_player = Some(uid);
            self.live.sell.plan = None;
            self.live.sell.fee_input = format!("{:.1}", value as f64 * rate / 1e6);
            self.live.sell.wage_input = wage.map(|w| format!("{:.1}", w as f64 * rate / 1e3)).unwrap_or_default();
        }
        if let Some((p, c, f, w)) = check {
            self.live.plan_buy(ctx, p, c, f, w);
        }
        if let Some((p, c, f, w)) = confirm {
            self.live.buy_now(ctx, p, c, f, w);
        }
    }

    // -------------------------------------------------------------------- instellingen

    fn page_settings(&mut self, ui: &mut Ui) {
        theme::page_header(ui, t!("Ayarlar", "Settings"), "", |_| {});
        let mut dirty = false;
        ui.columns(2, |cols| {
            theme::card(&mut cols[0], |ui| {
                ui.label(theme::h2(t!("Genel", "General")));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::ic(icon::TRANSLATE).color(theme::ACCENT));
                    ui.label(t!("Uygulama Dili:", "App Language:"));
                    let mut lang = self.settings.lang;
                    if theme::segmented(ui, &mut lang, &[(Lang::Tr, "Türkçe"), (Lang::En, "English")]) {
                        self.settings.lang = lang;
                        i18n::set(lang);
                        dirty = true;
                    }
                });
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::ic(icon::COINS).color(theme::ACCENT));
                    ui.label(t!("£ Başına € Kuru", "€ per £ Rate"));
                    dirty |= ui.add(egui::DragValue::new(&mut self.settings.rate).range(0.5..=2.0).speed(0.01).fixed_decimals(2)).changed();
                });
                ui.add_space(4.0);
                dirty |= ui
                    .checkbox(
                        &mut self.settings.capture_delay,
                        t!("Okumadan önce FM penceresini öne getir", "Bring FM window to front before reading"),
                    )
                    .changed();
            });
            theme::card(&mut cols[1], |ui| {
                ui.label(theme::h2(t!("Planlayıcı & Taktik Antrenmanı", "Planner & Tactical Training")));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("Taktik Tarzı:", "Tactical Style:"));
                    egui::ComboBox::from_id_salt("settings_tactic_mode")
                        .selected_text(self.settings.planner.tactic_mode.label())
                        .width(210.0)
                        .show_ui(ui, |ui| {
                            for m in planner::TacticMode::ALL {
                                if ui.selectable_value(&mut self.settings.planner.tactic_mode, m, m.label()).changed() {
                                    dirty = true;
                                }
                            }
                        });
                });
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(RichText::new(t!(
                        "Oyuncu Benimsemesi şu değerin altındayken Takım Kaynaşması ekle:",
                        "Add Team Bonding when Player Buy-In is below:"
                    )).color(theme::TEXT))
                    .wrap(),
                );
                ui.horizontal(|ui| {
                    dirty |= ui
                        .add(egui::Slider::new(&mut self.settings.planner.bonding_below, 0.0..=100.0).fixed_decimals(0).suffix("%"))
                        .changed();
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("Duran toplar kaç haftada bir (0 = kapalı)", "Set pieces every N weeks (0 = off)"));
                    dirty |= ui.add(egui::DragValue::new(&mut self.settings.planner.set_pieces_every).range(0..=8)).changed();
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("Hedefleri kaç haftada bir güncelle", "Update targets every N weeks"));
                    dirty |= ui.add(egui::DragValue::new(&mut self.settings.planner.replan_every_weeks).range(1..=8)).changed();
                });
                ui.add_space(6.0);
                ui.label(RichText::new(t!("Dinlenme Yükü", "Rest Load")).color(theme::TEXT));
                ui.horizontal_wrapped(|ui| {
                    use planner::Load;
                    dirty |= theme::segmented(
                        ui,
                        &mut self.settings.planner.load,
                        &[
                            (Load::Light, t!("Daha Fazla Dinlenme", "More Rest")),
                            (Load::Balanced, t!("Dengeli", "Balanced")),
                            (Load::Intense, t!("Daha Az Dinlenme", "Less Rest")),
                        ],
                    );
                });
                ui.add(
                    egui::Label::new(theme::dim(match self.settings.planner.load {
                        planner::Load::Light => t!(
                            "Her antrenman günü ve maçtan önceki gün Dinlenme ile biter.",
                            "Every training day and the day before a match ends with Rest."
                        ),
                        planner::Load::Balanced => t!("Standart dengeli plan.", "Standard balanced schedule."),
                        planner::Load::Intense => t!(
                            "Maçtan sonraki gün Yenilenme sonrasında bir de taktik seansı eklenir.",
                            "The day after a match adds a tactical session after Recovery."
                        ),
                    }))
                    .wrap(),
                );
                ui.add_space(6.0);
                dirty |= ui
                    .checkbox(
                        &mut self.settings.planner.no_tactical,
                        t!("Taktiksel seans ekleme (Sadece Oyuncu Gelişimi)", "No tactical sessions (Player Development only)"),
                    )
                    .changed();
                ui.add(
                    egui::Label::new(theme::dim(t!(
                        "Bu yuvalar seçilen taktik tarzına uygun oyuncu gelişimine ayrılır; maç hazırlığı kalır. Taktik anlayışını \"%100'de Sabit Tut\" (Kadro) özelliğiyle kullananlar içindir.",
                        "These slots are used for player development matching your tactical style; match preparation stays. Ideal if you use 'Keep Locked at 100%' on the Squad tab."
                    )))
                    .wrap(),
                );
            });
        });
        ui.add_space(14.0);
        if agent::LIVE_SUPPORTED {
            theme::card(ui, |ui| {
                ui.label(theme::h2(t!("FM İçinde Canlı Bağlantı", "Live Connection in FM")));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    let mut on = self.settings.allow_untested;
                    if theme::toggle(ui, &mut on).changed() {
                        self.settings.allow_untested = on;
                        self.live.allow_untested = on;
                        dirty = true;
                        if self.live.connected {
                            self.live.refresh(ui.ctx(), "", None);
                        }
                    }
                    ui.label(RichText::new(t!("Tüm FM27 sürümlerinde canlı düzenlemeye izin ver", "Allow live editing on all FM27 builds")).strong());
                });
                ui.add(egui::Label::new(theme::dim(t!(
                    "FM27 güncellemelerinden sonra da tüm canlı özelliklerin (antrenman, bütçe, moral, sakatlık, transfer) kesintisiz çalışmasını sağlar. Her değişiklik yazılmadan önce kontrol edilir, geri alınabilir ve yalnızca FM içinde kaydettiğinizde kalıcı olur.",
                    "Keeps all live features (training, budget, morale, injuries, transfers) working seamlessly after FM27 updates. Every change is verified before writing, can be undone, and only becomes permanent when you save in FM."
                )))
                .wrap());
                if self.live.connected {
                    ui.add_space(4.0);
                    let (c, txt) = if !self.live.untested_build {
                        (theme::SUCCESS, t!("Bu FM sürümü tam uyumlu ve doğrulanmıştır.", "This FM build is verified and fully compatible."))
                    } else if self.settings.allow_untested {
                        (theme::SUCCESS, t!("Canlı düzenleme aktif: tüm özellikler kullanılabilir.", "Live editing active: all features available."))
                    } else {
                        (theme::WARNING, t!("Salt okunur mod: değişiklik yapmak için yukarıdaki anahtarı açın.", "Read-only mode: enable the toggle above to make changes."))
                    };
                    ui.horizontal_wrapped(|ui| {
                        theme::status_dot(ui, c);
                        ui.label(RichText::new(txt).color(theme::TEXT));
                    });
                }
            });
            ui.add_space(14.0);
        }
        theme::card(ui, |ui| {
            ui.label(theme::h2(t!("Klasörler ve İzinler", "Folders & Permissions")));
            ui.add_space(6.0);
            ui.label(theme::dim(t!(
                format!("Dışa Aktarım: {}", export::output_dir().display()),
                format!("Export: {}", export::output_dir().display())
            )));
            ui.label(theme::dim(t!(
                format!("Ayarlar: {}", crate::data_dir().display()),
                format!("Settings: {}", crate::data_dir().display())
            )));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if theme::button(ui, icon::EXPORT, t!("Dışa aktarım klasörünü aç", "Open export folder"), true).clicked() {
                    let _ = std::fs::create_dir_all(export::output_dir());
                    open_path(&export::output_dir());
                }
                if theme::button(
                    ui,
                    icon::EXPORT,
                    t!("Uygulama klasörünü aç (son ekran görüntüsü ve okuma)", "Open app folder (last screenshot & analysis)"),
                    true,
                )
                .clicked()
                {
                    let _ = std::fs::create_dir_all(crate::data_dir());
                    open_path(&crate::data_dir());
                }
                if cfg!(target_os = "macos") && theme::button(ui, icon::CAMERA, t!("Ekran Kaydı İzni", "Screen Recording Permission"), true).clicked() {
                    open_path(std::path::Path::new("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"));
                }
            });
        });
        ui.add_space(14.0);
        theme::card(ui, |ui| {
            ui.label(theme::h2(t!("İletişim & Güncellemeler", "Contact & Updates")));
            ui.add_space(6.0);
            ui.add(egui::Label::new(theme::dim(t!(
                "Sorularınız, geri bildirimleriniz veya destek için e-posta üzerinden iletişime geçebilir, Kick kanalımı takip edebilir veya GitHub üzerinden en güncel sürümü kontrol edebilirsiniz.",
                "For questions, feedback or support, feel free to reach out via email, follow my Kick channel, or check GitHub for the latest version."
            )))
            .wrap());
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if theme::kick_button(ui, "https://kick.com/holyman3").clicked() {
                    open_url(KICK_URL);
                }
                ui.add_space(6.0);
                if theme::button(ui, icon::ENVELOPE_SIMPLE, CONTACT_EMAIL, true).clicked() {
                    ui.ctx().copy_text(CONTACT_EMAIL.to_string());
                    open_url(&format!("mailto:{CONTACT_EMAIL}"));
                }
                ui.add_space(6.0);
                if theme::button(ui, icon::EXPORT, t!("GitHub Sayfası", "GitHub Page"), true).clicked() {
                    open_url(GITHUB_REPO_URL);
                }
                ui.add_space(6.0);
                if theme::button(
                    ui,
                    icon::ARROW_CLOCKWISE,
                    t!("Güncellemeleri Kontrol Et", "Check for Updates"),
                    self.update_rx.is_none(),
                )
                .clicked()
                {
                    self.update_status = t!("GitHub kontrol ediliyor…", "Checking GitHub…").into();
                    self.update_rx = Some(spawn_update_check());
                }
                if let Some((tag, url)) = self.latest_release.clone() {
                    ui.add_space(6.0);
                    if theme::primary_button(
                        ui,
                        icon::ARROW_CLOCKWISE,
                        &t!(format!("Yeni sürüm ({tag}) İndir"), format!("Download New Version ({tag})")),
                        true,
                    )
                    .clicked()
                    {
                        open_url(&url);
                    }
                }
            });
            if !self.update_status.is_empty() {
                ui.add_space(6.0);
                ui.label(theme::dim(&self.update_status));
            }
        });
        ui.add_space(14.0);
        theme::card(ui, |ui| {
            ui.label(theme::h2(t!("Hakkında", "About")));
            ui.add(egui::Label::new(theme::dim(t!(
                "Seans adları, limitler ve şablonlar FM27'nin kendi oyun verilerinden alınmıştır. Modül yalnızca kontrolden sonra okur ve yazar, geri alma (undo) desteklidir. Yalnızca tek oyunculu (single-player) kullanım içindir. Yazı tipi: Inter (SIL OFL). İkonlar: Phosphor (MIT).",
                "Session names, limits and templates come from FM27's own game data. The module only reads and writes after verification, with undo support. For single-player use only. Font: Inter (SIL OFL). Icons: Phosphor (MIT)."
            )))
            .wrap());
        });
        if dirty {
            self.save_settings();
            self.week_plan = None;
            self.year.clear();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.snapshot(ctx);
        self.poll_analysis(ctx);
        self.poll_update();
        // automatisch uitlezen: niet tijdens handmatig aanpassen en nooit tijdens een testopname
        if self.settings.auto_read
            && !self.edit_mode
            && self.analysis_rx.is_none()
            && std::env::var_os("FM27_SNAPSHOT").is_none()
            && self.last_auto.elapsed().as_secs() >= self.settings.auto_read_secs.max(5) as u64
        {
            self.last_auto = std::time::Instant::now();
            self.auto_pending = true;
            self.start_analysis(ctx, None);
        }
        self.live.allow_untested = self.settings.allow_untested;
        self.live.choice = (self.settings.manager_club, self.settings.training_team);
        self.live.keep_understanding = self.settings.keep_understanding;
        self.live.tick(ctx, &self.eff, &self.settings.planner, self.settings.auto_training);
        self.sidebar(ctx);
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin { left: 28, right: 28, top: 24, bottom: 16 }))
            .show(ctx, |ui| {
                let panel_w = (ui.available_width() - 12.0).min(1400.0).max(400.0);
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width(panel_w);
                    ui.set_max_width(panel_w);
                    // pagina komt zacht binnen: kort vervagen en een paar pixels omhoog schuiven
                    if self.shown_page != Some(self.page) {
                        self.shown_page = Some(self.page);
                        theme::restart_enter(ctx);
                    }
                    let t = theme::enter(ctx, 0.22);
                    ui.set_opacity(0.25 + 0.75 * t);
                    ui.add_space(8.0 * (1.0 - t));
                    match self.page {
                        Page::Overview => self.page_overview(ui, ctx),
                        Page::Training => self.page_training(ui, ctx),
                        Page::Planning => self.page_planning(ui),
                        Page::Club => self.page_club(ui, ctx),
                        Page::Squad => self.page_squad(ui, ctx),
                        Page::Tactics => self.page_tactics(ui, ctx),
                        Page::Sell => self.page_sell(ui, ctx),
                        Page::Settings => self.page_settings(ui),
                    }
                    ui.add_space(20.0);
                });
            });
        if self.analysis_rx.is_some() || self.live.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else if self.settings.auto_read {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs(5));
        }
    }
}

// ------------------------------------------------------------------------ helpers

/// Contactgegevens.
const KICK_URL: &str = "https://kick.com/holyman3";
const CONTACT_EMAIL: &str = "holyymann3@gmail.com";

fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).status();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

fn open_path(p: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(p).status();
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(p).status();
}

fn money(v: i64) -> String {
    let a = v.unsigned_abs() as f64;
    let s = if a >= 1e6 { format!("{:.1} Mn", a / 1e6) } else if a >= 1e3 { format!("{:.0} B", a / 1e3) } else { format!("{a:.0}") };
    if v < 0 { format!("-{s}") } else { s }
}

/// De acht onderdelen van FM's Tactical Understanding, in de volgorde van de module (Türkçe).
const UNDERSTANDING_AREAS: [&str; 8] = [
    "Pas Stili",
    "Oyun Kurulum",
    "Pozisyon Üretimi",
    "Top Takımdayken Diziliş",
    "Karşılama Çizgisi",
    "Pres Yoğunluğu",
    "Pres Tuzağı",
    "Top Rakipteyken Diziliş",
];

const LIVE_UNDERSTANDING_FACTORS: [Factor; 8] = [
    Factor::PassingStyle,
    Factor::BuildUp,
    Factor::ChanceCreation,
    Factor::InPossessionShape,
    Factor::LineOfEngagement,
    Factor::PressingIntensity,
    Factor::PressingTrap,
    Factor::OutOfPossessionShape,
];

fn morale_label(m: u64) -> &'static str {
    if i18n::is_tr() {
        match m {
            0..=4 => "Berbat",
            5..=7 => "Çok Kötü",
            8..=10 => "Kötü",
            11..=13 => "İdare Eder",
            14..=16 => "İyi",
            17..=18 => "Çok İyi",
            _ => "Mükemmel",
        }
    } else {
        match m {
            0..=4 => "Abysmal",
            5..=7 => "Very Poor",
            8..=10 => "Poor",
            11..=13 => "Fairly Okay",
            14..=16 => "Good",
            17..=18 => "Very Good",
            _ => "Superb",
        }
    }
}

/// Kısa, okunabilir Türkçe / İngilizce antrenman seansı adı (kartlar ve etiketler için).
fn chip_name(name: &str) -> String {
    if !i18n::is_tr() {
        return match name {
            "In Possession Shape" => "IP Shape".into(),
            "Out Of Possession Shape" => "OOP Shape".into(),
            "Line of Engagement" => "Line of Eng.".into(),
            "Pressing Intensity" => "Pressing Int.".into(),
            "Individual Development" => "Indiv. Dev.".into(),
            "Theory - Attacking Shadow Play" => "Theory - Attack".into(),
            "Theory - Defensive Shadow Play" => "Theory - Defence".into(),
            "Technical - Chance Creation" => "Tech - Creation".into(),
            "Technical - Chance Conversion" => "Tech - Finishing".into(),
            "Technical - Ball Retention" => "Tech - Retention".into(),
            "Technical - Ground Defence" => "Tech - Ground Def.".into(),
            "Technical - Aerial Defence" => "Tech - Aerial Def.".into(),
            "Rest" | "Rest (IM)" => "Rest".into(),
            "Recovery" | "Recovery (IM)" => "Recovery".into(),
            "Light Conditioning" | "Light Conditioning (IM)" => "Light Cond.".into(),
            "Heavy Conditioning" => "Heavy Cond.".into(),
            "Match" => "MATCH".into(),
            "Not Available" => "Empty".into(),
            other => other.into(),
        };
    }
    match name {
        "Passing Style" => "Pas Stili".into(),
        "Build Up" => "Oyun Kurulum".into(),
        "Chance Creation" => "Pozisyon Üretimi".into(),
        "In Possession Shape" => "Topla Diziliş".into(),
        "Line of Engagement" => "Karşılama Çizgisi".into(),
        "Pressing Intensity" => "Pres Yoğunluğu".into(),
        "Pressing Trap" => "Pres Tuzağı".into(),
        "Out Of Possession Shape" => "Topsuz Diziliş".into(),
        "Press Avoidance" => "Presten Çıkış".into(),
        "Ball Progression" => "Topu İleri Taşıma".into(),
        "Final Third" => "Üçüncü Bölge".into(),
        "Defensive Block" => "Savunma Bloğu".into(),
        "Defensive Positioning" => "Savunma Yerleşimi".into(),
        "Defensive Engagement" => "Savunma Karşılaması".into(),
        "Corners" => "Kornerler".into(),
        "Free Kicks" => "Serbest Vuruş".into(),
        "Penalties" => "Penaltılar".into(),
        "Individual Development" => "Bireysel Gelişim".into(),
        "Attacking Ball Work" => "Hücum Top Çal.".into(),
        "Defensive Ball Work" => "Savunma Top Çal.".into(),
        "Possession Ball Work" => "Pas & Topa Sahip".into(),
        "Attacking - Balanced" => "Hücum - Dengeli".into(),
        "Attacking - Patient" => "Hücum - Sabırlı".into(),
        "Attacking - Direct" => "Hücum - Dikine".into(),
        "Attacking - Wings" => "Hücum - Kanatlar".into(),
        "Defending - Balanced" => "Savunma - Dengeli".into(),
        "Defending - Engaged" => "Savunma - Önde Baskı".into(),
        "Defending - Disengaged" => "Savunma - Geride".into(),
        "Defending - Wide" => "Savunma - Kanat".into(),
        "Theory - Attacking Shadow Play" => "Teori - Hücum".into(),
        "Theory - Defensive Shadow Play" => "Teori - Savunma".into(),
        "Technical - Chance Creation" => "Teknik - Pozisyon".into(),
        "Technical - Chance Conversion" => "Teknik - Bitiricilik".into(),
        "Technical - Ball Retention" => "Teknik - Top Saklama".into(),
        "Technical - Ground Defence" => "Teknik - Yerden Sav.".into(),
        "Technical - Aerial Defence" => "Teknik - Hava Sav.".into(),
        "Transition - Restrict" => "Geçiş - Kısıtlama".into(),
        "Transition - Press" => "Geçiş - Şok Pres".into(),
        "Transition - Progress" => "Geçiş - Hızlı Çıkış".into(),
        "GK - Balanced" => "KL - Dengeli".into(),
        "GK - Shot Stopping" => "KL - Şut Kurtarma".into(),
        "GK - Handling" => "KL - Top Kontrolü".into(),
        "GK - One On Ones" => "KL - Bire Bir".into(),
        "GK - Distribution" => "KL - Dağıtım".into(),
        "Set Pieces - Delivery" => "Duran Top - Orta".into(),
        "Set Pieces - Penalties" => "Duran Top - Penaltı".into(),
        "Rest" | "Rest (IM)" => "Dinlenme".into(),
        "Recovery" | "Recovery (IM)" => "Yenilenme".into(),
        "Light Conditioning" | "Light Conditioning (IM)" => "Hafif Kondisyon".into(),
        "Conditioning" => "Kondisyon".into(),
        "Heavy Conditioning" => "Ağır Kondisyon".into(),
        "Team Bonding" => "Takım Kaynaşması".into(),
        "Community Outreach" => "Topluluk Etkinliği".into(),
        "Travel" => "Seyahat".into(),
        "Match" => "MAÇ".into(),
        "Not Available" => "Boş".into(),
        "Match Preparation" => "Maç Hazırlığı".into(),
        "Tactical Understanding IP" => "Taktik (Topla)".into(),
        "Tactical Understanding OOP" => "Taktik (Topsuz)".into(),
        other => crate::model::session_tr(other).to_string(),
    }
}

fn chip(ui: &mut Ui, name: &str, highlight: bool, faded: bool) {
    let col = if name == "Match" { theme::DANGER } else { theme::rgb(category_of(name).color()) };
    let col = if faded { theme::tint(col, 0.45) } else { col };
    theme::session_chip(ui, &chip_name(name), col, highlight).on_hover_text(crate::model::session_tr(name));
}

fn step_card(ui: &mut Ui, n: u8, icon_s: &str, title: &str, body: impl FnOnce(&mut Ui)) {
    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            Frame::new().fill(theme::ACCENT_SOFT).corner_radius(CornerRadius::same(20)).inner_margin(Margin::symmetric(8, 2)).show(ui, |ui| {
                ui.label(RichText::new(n.to_string()).color(theme::ACCENT).strong());
            });
            ui.label(theme::ic(icon_s).color(theme::ACCENT).size(16.0));
            ui.label(theme::h2(title));
        });
        ui.add_space(4.0);
        body(ui);
    });
}

fn codes_text(c: &[u16; 3]) -> String {
    c.iter().map(|x| chip_name(session_by_code(*x).unwrap_or("?"))).collect::<Vec<_>>().join(" · ")
}

fn week_card(ui: &mut Ui, w: &live_training::WeekPreview, today: Option<NaiveDate>, fm_had: &std::collections::HashMap<NaiveDate, [u16; 3]>) {
    theme::card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::h2(t!(
                format!("{} Haftası", i18n::date_short(w.start)),
                format!("Week of {}", i18n::date_short(w.start))
            )));
            if w.rest {
                theme::pill(ui, t!("Dinlenme haftası: FM'ye bırakıldı", "Rest week: left to FM"), theme::WARNING);
                return;
            }
            theme::pill(ui, &t!(format!("{} maç", w.matches), format!("{} matches", w.matches)), theme::DANGER);
            theme::pill(ui, crate::model::focus_tr(&w.focus), theme::ACCENT);
            let pending = (0..7).filter(|d| w.current[*d] != w.planned[*d]).count();
            if w.written > 0 {
                theme::pill(ui, &t!(format!("{} gün geri yüklendi", w.written), format!("{} days restored", w.written)), theme::SUCCESS);
            } else if pending > 0 {
                theme::pill(ui, &t!(format!("{pending} gün planınızdan farklı"), format!("{pending} days differ from plan")), theme::WARNING);
            } else {
                theme::pill(ui, t!("plana uygun", "matches plan"), theme::SUCCESS);
            }
        });
        if w.rest {
            return;
        }
        ui.add_space(8.0);
        ui.columns(7, |cols| {
            for d in 0..7 {
                let ui = &mut cols[d];
                let date = w.start + Duration::days(d as i64);
                let past = today.is_some_and(|t| date <= t);
                ui.label(theme::caps(i18n::weekday_short(d)));
                ui.label(theme::faint(i18n::date_short(date)));
                let differs = w.current[d] != w.planned[d];
                let was = if differs { Some(w.current[d]) } else { fm_had.get(&date).copied().filter(|h| *h != w.planned[d]) };
                for code in w.planned[d] {
                    let name = session_by_code(code).unwrap_or("?");
                    chip(ui, name, was.is_some() && !past, past || !w.editable[d]);
                }
                if let Some(h) = was.filter(|_| !past && w.editable[d]) {
                    ui.add(egui::Label::new(RichText::new(format!("FM: {}", codes_text(&h))).size(10.5).color(theme::TEXT_FAINT).strikethrough()).truncate())
                        .on_hover_text(t!(format!("FM'nin planladığı: {}", codes_text(&h)), format!("Planned by FM: {}", codes_text(&h))));
                }
            }
        });
    });
}

fn plan_grid(ui: &mut Ui, p: &WeekPlan) {
    ui.columns(7, |cols| {
        for (i, d) in p.days.iter().enumerate() {
            let ui = &mut cols[i];
            ui.label(theme::caps(i18n::weekday_short(i)));
            ui.label(theme::faint(format!("{} · {}", i18n::date_short(d.date), d.role.label())));
            for s in &d.slots {
                chip(ui, s, false, false);
            }
        }
    });
}

fn legend(ui: &mut Ui) {
    use crate::model::Category::*;
    ui.horizontal_wrapped(|ui| {
        ui.label(theme::faint(t!(
            "Çerçeveli = FM27 Trainer tarafından ayarlandı · üstü çizili = FM'nin istediği ·",
            "Bordered = set by FM27 Trainer · strikethrough = what FM wanted ·"
        )));
        for c in [TacticalUnderstanding, MatchPreparation, FootballingDevelopment, PhysicalRecovery, ExtraCurricular, Fixed] {
            theme::status_dot(ui, theme::rgb(c.color()));
            ui.label(theme::faint(c.label()));
            ui.add_space(8.0);
        }
    });
}

fn kind_combo(ui: &mut Ui, id: &str, kind: &mut WeekKind) {
    egui::ComboBox::from_id_salt(id).selected_text(kind.label()).width(170.0).show_ui(ui, |ui| {
        for k in WeekKind::ALL {
            ui.selectable_value(kind, k, k.label());
        }
    });
}

fn weekday_combo(ui: &mut Ui, id: &str, day: &mut Weekday) {
    let names = [
        (Weekday::Fri, t!("Cuma", "Friday")),
        (Weekday::Sat, t!("Cumartesi", "Saturday")),
        (Weekday::Sun, t!("Pazar", "Sunday")),
    ];
    let cur = names.iter().find(|(d, _)| d == day).map(|(_, n)| *n).unwrap_or("—");
    egui::ComboBox::from_id_salt(id).selected_text(cur).show_ui(ui, |ui| {
        for (d, n) in names {
            ui.selectable_value(day, d, n);
        }
    });
}
