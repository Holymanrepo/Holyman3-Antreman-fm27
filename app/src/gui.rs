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
    tactics_asked: bool,
    frames: u32,
}

impl App {
    fn new() -> Self {
        let mut settings: Settings = std::fs::read_to_string(settings_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        settings.lang = Lang::Tr;
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
            tactics_asked: false,
            live: {
                let mut l = LiveState::default();
                l.sell.buy = std::env::var("FM27_PAGE").as_deref() == Ok("buy");
                l
            },
            frames: 0,
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
                let idx = data.current.min(data.tactics.len() - 1);
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
                mode_msg = format!(" · Önerilen Tarz: {}", suggested.label());
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
                    Frame::new().fill(theme::ACCENT).corner_radius(CornerRadius::same(11)).inner_margin(Margin::same(7)).show(ui, |ui| {
                        ui.label(theme::ic(icon::SOCCER_BALL).size(20.0).color(Color32::WHITE));
                    });
                    ui.vertical(|ui| {
                        ui.add_space(1.0);
                        ui.label(RichText::new("FM27 Trainer").font(egui::FontId::new(16.0, theme::bold())).color(theme::TEXT));
                        ui.label(theme::faint("by Holyman3"));
                    });
                });
                ui.add_space(20.0);
                ui.label(theme::caps("Menü"));
                ui.add_space(4.0);
                let items = [
                    (Page::Overview, icon::GAUGE, "Genel Bakış"),
                    (Page::Training, icon::BARBELL, "Antrenman"),
                    (Page::Planning, icon::CALENDAR_DOTS, "Planlama"),
                    (Page::Club, icon::BANK, "Kulüp"),
                    (Page::Squad, icon::USERS_THREE, "Kadro"),
                    (Page::Tactics, icon::STRATEGY, "Taktik"),
                    (Page::Sell, icon::HANDSHAKE, "Transferler"),
                    (Page::Settings, icon::GEAR_SIX, "Ayarlar"),
                ];
                for (p, ic, label) in items {
                    if theme::nav_item(ui, ic, label, self.page == p).clicked() {
                        self.page = p;
                    }
                    ui.add_space(2.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.label(theme::faint(format!("v{} · Türkçe · Holyman3", env!("CARGO_PKG_VERSION"))));
                    ui.add_space(8.0);
                    Frame::new().fill(theme::SURFACE).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(theme::caps("İletişim"));
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
                    });
                    ui.add_space(8.0);
                    Frame::new().fill(theme::SURFACE).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(12)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (c, txt) = if !agent::LIVE_SUPPORTED {
                                (theme::TEXT_FAINT, "Canlı: desteklenmiyor")
                            } else if self.live.connected {
                                (theme::SUCCESS, "FM27'ye Bağlı")
                            } else {
                                (theme::TEXT_FAINT, "Bağlı değil")
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
        Frame::new().fill(theme::tint(c, 0.12)).stroke(Stroke::new(1.0, theme::tint(c, 0.4))).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
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
                ui.label(theme::h2("FM içinde canlı bağlantı bu sistemde kullanılamıyor"));
                ui.label(theme::dim("Okuma, planlama ve dışa aktarım çalışmaktadır."));
                return;
            }
            let windows = cfg!(windows);
            ui.horizontal(|ui| {
                ui.label(theme::ic(icon::PLUGS_CONNECTED).size(28.0).color(theme::ACCENT));
                ui.vertical(|ui| {
                    ui.label(theme::h2("Football Manager'a Bağlan"));
                    ui.label(theme::dim(if windows {
                        "Uygulama FM27 çalışırken doğrudan okur. Hiçbir şey kurmanıza gerek yoktur."
                    } else {
                        "Uygulama, FM ile birlikte başlayan küçük bir modül aracılığıyla doğrudan FM27 ile iletişim kurar."
                    }));
                });
            });
            ui.add_space(10.0);
            let steps = if windows {
                [
                    (1, "Bilgisayarınızda FM27'yi başlatın: Steam (veya aşağıdaki düğme), Xbox uygulaması / Game Pass veya Epic."),
                    (2, "Kayıtlı oyununuzu (save) yükleyin."),
                    (3, "Uygulama otomatik olarak bağlanır."),
                ]
            } else {
                [
                    (1, "Oyununuzu kaydedin ve FM27'den çıkın."),
                    (2, "'FM'yi modül ile başlat' düğmesine tıklayın (Steam açık olmalı)."),
                    (3, "Kaydınızı yükleyin. Uygulama otomatik olarak bağlanır."),
                ]
            };
            for (n, s) in steps {
                ui.horizontal(|ui| {
                    Frame::new().fill(theme::ACCENT_SOFT).corner_radius(CornerRadius::same(20)).inner_margin(Margin::symmetric(8, 2)).show(ui, |ui| {
                        ui.label(RichText::new(n.to_string()).color(theme::ACCENT).strong());
                    });
                    ui.label(s);
                });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let start = if windows { "FM27'yi Başlat" } else { "FM'yi modül ile başlat" };
                if theme::primary_button(ui, icon::PLAY, start, !self.live.busy()).clicked() {
                    self.live.start_fm(ctx);
                }
                if theme::button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Yeniden Bağlan", !self.live.busy()).clicked() {
                    self.live.refresh(ctx, "bağlan", None);
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
            format!("Son okuma: {}", self.eff.source)
        } else {
            "FM27 belleğinden canlı taktiğinizi okuyun veya Taktiksel Etkinlik (Breakdown) ekranını okutun.".to_string()
        };
        let busy = self.analysis_rx.is_some();
        let connected = self.live.connected;
        let mut read = false;
        let mut file = false;
        let mut sync_live = false;
        theme::page_header(ui, "Taktiksel Etkinlik", &subtitle, |ui| {
            if theme::button(ui, icon::EXPORT, "Dosyadan", !busy).clicked() {
                file = true;
            }
            if theme::button(ui, icon::CAMERA, "FM Ekranını Oku", !busy).clicked() {
                read = true;
            }
            if theme::primary_button(ui, icon::STRATEGY, "Taktikten Canlı Oku", connected && !busy).clicked() {
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
        ui.horizontal(|ui| {
            if theme::toggle(ui, &mut self.settings.auto_read).changed() {
                self.last_auto = std::time::Instant::now() - std::time::Duration::from_secs(600);
                self.auto_note.clear();
                self.save_settings();
            }
            ui.label(theme::label_strong("Otomatik Ekran Okuma"));
            if self.settings.auto_read {
                ui.label(theme::dim("her"));
                if ui.add(egui::DragValue::new(&mut self.settings.auto_read_secs).range(5..=120).suffix(" sn")).changed() {
                    self.save_settings();
                }
                if !self.auto_note.is_empty() {
                    theme::pill(ui, &self.auto_note, theme::INFO);
                }
            } else {
                ui.label(theme::faint(
                    "FM'ye bağlıyken 'Taktikten Canlı Oku' ile ekran görüntüsüne gerek kalmadan verileri doğrudan çekebilirsiniz.",
                ));
            }
        });
        ui.add_space(12.0);
        if self.show_file {
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Ekran Görüntüsü (PNG):");
                    ui.add(egui::TextEdit::singleline(&mut self.image_path).hint_text("/…/breakdown.png").desired_width(420.0));
                    if theme::button(ui, "", "Oku", !busy && !self.image_path.trim().is_empty()).clicked() {
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
                self.eff.source = format!("{} *", if self.eff.source.is_empty() { "elle" } else { &self.eff.source });
            }
            let _ = crate::save_effectiveness(&self.eff);
            self.week_plan = None;
            self.year.clear();
        }
        ui.add_space(4.0);
        ui.checkbox(&mut self.edit_mode, "Değerleri elle ayarla");
        ui.add_space(10.0);

        // prioriteiten
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::ic(icon::LIGHTNING).color(theme::WARNING).size(18.0));
                ui.label(theme::h2("Öncelikler"));
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
            "Antrenman",
            "FM27 Trainer antrenmanınızı taktiğinize ve en zayıf alanlarınıza göre planlar, FM takviminde sabit tutar.",
            |ui| {
                if theme::primary_button(ui, icon::LIGHTNING, "Antrenmanı Şimdi FM'ye Uygula", can).clicked() {
                    write = true;
                }
                if theme::button(ui, icon::STRATEGY, "Taktikten Oku & Uygula", can).clicked() {
                    sync_and_write = true;
                }
                if theme::button(ui, icon::EYE, "Planı Önizle", can).clicked() {
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
                ui.label(theme::h2("Taktikle Uyumlu Antrenman Programı"));
            });
            ui.add_space(6.0);
            let mut mode = self.settings.planner.tactic_mode;
            let opts: Vec<(planner::TacticMode, &str)> = planner::TacticMode::ALL.iter().map(|m| (*m, m.label())).collect();
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
        Frame::new()
            .fill(theme::tint(accent, 0.10))
            .stroke(Stroke::new(1.0, theme::tint(accent, 0.45)))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(theme::ic(if auto { icon::CHECK_CIRCLE } else { icon::WARNING }).size(30.0).color(accent));
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(theme::h2("Antrenmanı Otomatik Olarak Planda Tut"));
                            ui.add_space(8.0);
                            if theme::toggle(ui, &mut self.settings.auto_training).changed() {
                                self.save_settings();
                            }
                            ui.label(RichText::new(if auto { "AÇIK" } else { "KAPALI" }).strong().color(accent));
                        });
                        let text = if !connected {
                            "FM çalıştığında ve bağlandığında aktif olur (aşağıya bakın)."
                        } else if auto {
                            "FM asistanı her hafta gelecek 1–2 haftayı yeniden planlar. FM27 Trainer her 8 saniyede bir kontrol eder ve değişiklikleri seçtiğiniz taktik antrenman planına geri döndürür. Oynarken uygulamayı açık tutun."
                        } else {
                            "Kapalı: FM her hafta antrenmanınızın üzerine kendi planını yazar. Bunu açın veya birkaç oyun haftasında bir 'Antrenmanı Şimdi FM'ye Uygula'ya tıklayın."
                        };
                        ui.add(egui::Label::new(theme::dim(text)).wrap());
                    });
                });
                if connected {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 36.0;
                        let last = self.live.last_check.map(|t| t.format("%H:%M:%S").to_string()).unwrap_or_else(|| "—".into());
                        theme::mini_stat(ui, "Son Kontrol", &last);
                        theme::mini_stat(ui, "Geri Yüklenen (Bu Oturum)", &format!("{} gün", self.live.fixed_total));
                        let d = self.live.training.as_ref().and_then(|o| o.today).map(i18n::date).unwrap_or_else(|| "—".into());
                        theme::mini_stat(ui, "Oyun Tarihi", &d);
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
            step_card(&mut cols[0], 1, icon::CAMERA, "Ölçüm", |ui| {
                let txt = if !self.has_data() {
                    "Henüz okunmadı.".to_string()
                } else {
                    match age_weeks {
                        Some(0) => "Bu hafta okundu (oyun zamanı).".to_string(),
                        Some(w) => format!("{w} hafta önce okundu (oyun zamanı)."),
                        None => format!("Okundu: {}", self.eff.source),
                    }
                };
                ui.label(if stale { RichText::new(txt).color(theme::WARNING) } else { theme::dim(txt) });
                ui.add(egui::Label::new(theme::faint(
                    "'Taktikten Oku & Uygula' düğmesiyle veya Genel Bakış sekmesinden her 2–3 oyun haftasında bir verileri güncelleyin.",
                )).wrap());
                if theme::small_button(ui, "Genel Bakış'a Git", true).clicked() {
                    go_overview = true;
                }
            });
            step_card(&mut cols[1], 2, icon::TARGET, "Planlama", |ui| {
                ui.label(theme::dim("Ekstra odaklanılan alanlar:"));
                let mut areas: Vec<(Factor, f32)> = Factor::TACTICAL_AREAS.iter().map(|f| (*f, self.eff.get(*f))).collect();
                areas.sort_by(|a, b| (a.1 / self.settings.planner.tactic_mode.weight(a.0)).total_cmp(&(b.1 / self.settings.planner.tactic_mode.weight(b.0))));
                for (f, v) in areas.into_iter().take(3) {
                    theme::pill(ui, &format!("{} · {v:.0}", f.label()), theme::rgb(level_color(v)));
                }
                ui.add(egui::Label::new(theme::faint(
                    "Maçlar etrafındaki ritim SI şablonlarını takip eder; odak seçtiğiniz taktik tarzına göre her hafta kayar.",
                )).wrap());
            });
            step_card(&mut cols[2], 3, icon::SPARKLE, "Koruma", |ui| {
                for l in [
                    "Yalnızca bugünden sonraki günler değiştirilir.",
                    "Maç ve seyahat günleri FM'ye bırakılır.",
                    "Dinlenme haftaları (tatil) FM'ye bırakılır.",
                    "FM içinde kaydetmek her şeyi kaydınızda saklar.",
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
                ui.label(theme::h2("Etkinlik Günlüğü"));
            });
            ui.add_space(4.0);
            if self.live.log.is_empty() {
                ui.label(theme::faint(
                    "Henüz değişiklik yok. FM bir haftayı yeniden planladığında neyin geri yüklendiğini burada göreceksiniz.",
                ));
            }
            for e in self.live.log.iter().take(6) {
                ui.horizontal(|ui| {
                    ui.label(theme::faint(e.at.format("%H:%M:%S").to_string()));
                    theme::pill(ui, if e.auto { "otomatik" } else { "elle" }, if e.auto { theme::SUCCESS } else { theme::ACCENT });
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
        ui.horizontal(|ui| {
            ui.label(theme::h2("Gelecek Haftalar"));
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
            "Planlama",
            "Taktik tarzınıza göre haftalık ve sezonluk antrenman planı oluşturun ve dışa aktarın.",
            |ui| {
                theme::segmented(ui, &mut tab, &[(PlanTab::Week, "Hafta"), (PlanTab::Season, "Sezon")]);
            },
        );
        self.plan_tab = tab;
        let (m, e) = (self.status.clone(), self.status_error);
        self.status_line(ui, &m, e);

        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::ic(icon::STRATEGY).color(theme::ACCENT).size(18.0));
                ui.label(theme::label_strong("Taktik Tarzı:"));
                let mut mode = self.settings.planner.tactic_mode;
                let opts: Vec<(planner::TacticMode, &str)> = planner::TacticMode::ALL.iter().map(|m| (*m, m.label())).collect();
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
                ui.label("Hafta başlangıcı");
                ui.add(egui::TextEdit::singleline(&mut self.week_start).desired_width(96.0));
                ui.separator();
                ui.label("Maç günü");
                for i in 0..7 {
                    ui.toggle_value(&mut self.week_matches[i], i18n::weekday_short(i));
                }
                ui.separator();
                ui.checkbox(&mut self.prev_sunday, "önceki pazar maç var");
                ui.checkbox(&mut self.next_monday, "sonraki pazartesi maç var");
                kind_combo(ui, "weekkind", &mut self.week_kind);
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                // bij een testopname (FM27_SNAPSHOT) meteen een plan laten zien
                let auto = self.week_plan.is_none() && std::env::var("FM27_SNAPSHOT").is_ok();
                if theme::primary_button(ui, icon::SPARKLE, "Haftalık Plan Oluştur", true).clicked() || auto {
                    let start = NaiveDate::parse_from_str(self.week_start.trim(), "%Y-%m-%d")
                        .map(planner::monday_of)
                        .unwrap_or_else(|_| planner::monday_of(chrono::Local::now().date_naive()));
                    let week = CalendarWeek { start, kind: self.week_kind, matches: self.week_matches, note: String::new() };
                    self.week_plan = Some(planner::plan_week(&week, self.prev_sunday, self.next_monday, &self.eff, &self.settings.planner, 1));
                }
                if let Some(p) = self.week_plan.clone() {
                    if theme::button(ui, icon::EXPORT, "Dışa Aktar & Aç", true).clicked() {
                        let r = export::write_all(&export::output_dir(), "weekplan", "FM27 Haftalık Plan", &self.eff, std::slice::from_ref(&p));
                        self.after_export(r);
                    }
                }
            });
        });
        ui.add_space(12.0);
        if let Some(p) = &self.week_plan {
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::h2(&p.schedule_name));
                    theme::pill(ui, &format!("Odak: {}", crate::model::focus_tr(p.focus)), theme::ACCENT);
                });
                ui.add_space(8.0);
                plan_grid(ui, p);
                if !p.notes.is_empty() {
                    ui.add_space(8.0);
                    ui.label(theme::caps("Antrenman Dışı"));
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
                ui.label("Başlangıç");
                ui.add(egui::TextEdit::singleline(&mut self.year_start).desired_width(96.0));
                ui.label("Lig maçı");
                weekday_combo(ui, "league", &mut self.cal_opts.league_day);
                ui.label("Avrupa");
                let mut e = self.cal_opts.europe;
                egui::ComboBox::from_id_salt("europe")
                    .selected_text(match e {
                        None => "yok",
                        Some(Weekday::Tue) => "Sal (ŞL)",
                        Some(Weekday::Wed) => "Çar (ŞL)",
                        _ => "Per (AL/KL)",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut e, None, "yok");
                        ui.selectable_value(&mut e, Some(Weekday::Tue), "Sal (ŞL)");
                        ui.selectable_value(&mut e, Some(Weekday::Wed), "Çar (ŞL)");
                        ui.selectable_value(&mut e, Some(Weekday::Thu), "Per (AL/KL)");
                    });
                self.cal_opts.europe = e;
                ui.checkbox(&mut self.cal_opts.cup, "kupa");
                ui.add(egui::DragValue::new(&mut self.cal_opts.weeks).range(4..=60).suffix(" hafta"));
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if theme::button(ui, icon::CALENDAR_BLANK, "Varsayılan Takvim", true).clicked() {
                    if let Ok(d) = NaiveDate::parse_from_str(self.year_start.trim(), "%Y-%m-%d") {
                        self.cal_opts.start = d;
                    }
                    self.calendar = planner::default_calendar(&self.cal_opts);
                    self.year.clear();
                    self.save_calendar();
                }
                if theme::primary_button(ui, icon::SPARKLE, "Sezon Planı Oluştur", true).clicked() {
                    self.year = planner::plan_year(&self.calendar, &self.eff, &self.settings.planner);
                    self.save_calendar();
                }
                if !self.year.is_empty() {
                    if theme::button(ui, icon::EXPORT, "Dışa Aktar & Aç", true).clicked() {
                        let r = export::write_all(&export::output_dir(), "seasonplan", "FM27 Sezon Planı", &self.eff, &self.year);
                        self.after_export(r);
                    }
                    let (n, u) = (self.year.len(), unique_schedules(&self.year).len());
                    ui.label(theme::faint(format!("{n} hafta · {u} farklı haftalık program")));
                }
            });
        });
        ui.add_space(12.0);
        let mut edited = false;
        theme::card(ui, |ui| {
            egui::Grid::new("cal").num_columns(5).spacing([14.0, 8.0]).striped(true).show(ui, |ui| {
                for h in ["Hafta", "Dönem", "Maçlar", "Odak", "Taktiksel"] {
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
        let name = self.live.club_name().unwrap_or_else(|| "Kulüp".into());
        let mut action: Option<(String, Value)> = None;
        let mut backup = false;
        theme::page_header(
            ui,
            &name,
            "Finansı canlı olarak düzenleyin. Değişiklikler FM içinde kaydettiğinizde kalıcı olur.",
            |ui| {
                if theme::button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Geri Al", connected && !busy).clicked() {
                    action = Some(("geri al".into(), json!({"cmd": "undo"})));
                }
                if theme::button(ui, icon::FLOPPY_DISK, "Kayıtları Yedekle", !busy).clicked() {
                    backup = true;
                }
            },
        );
        if backup {
            match agent::backup_saves() {
                Ok(d) => self.live.status = format!("Yedek oluşturuldu: {}", d.display()),
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
            theme::stat_tile(&mut cols[0], icon::COINS, "Bakiye", &eur(g("balance")), &gbp(g("balance")), theme::INFO, None);
            theme::stat_tile(&mut cols[1], icon::BANK, "Transfer Bütçesi", &eur(g("transfer_budget")), &gbp(g("transfer_budget")), theme::ACCENT, None);
            theme::stat_tile(&mut cols[2], icon::USERS_THREE, "Haftalık Maaş Bütçesi", &eur(g("wage_budget")), &gbp(g("wage_budget")), theme::SUCCESS, None);
        });
        ui.add_space(14.0);
        ui.columns(2, |cols| {
            theme::card(&mut cols[0], |ui| {
                ui.label(theme::h2("Transfer Bütçesi"));
                ui.label(theme::faint("Milyon Euro cinsinden tutar"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.live.transfer_input).hint_text("50").desired_width(90.0));
                    ui.label("Mn €");
                    if theme::primary_button(ui, icon::CHECK_CIRCLE, "Ayarla", !busy).clicked() {
                        if let Ok(m) = self.live.transfer_input.replace(',', ".").trim().parse::<f64>() {
                            action = Some(("transfer bütçesi".into(), json!({"cmd": "set_finances", "transfer_budget": (m * 1e6 / rate as f64) as i64})));
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for add in [10, 25, 50, 100] {
                        if theme::small_button(ui, &format!("+€{add}M"), !busy).clicked() {
                            let v = g("transfer_budget") + (add as f64 * 1e6 / rate as f64) as i64;
                            action = Some(("transfer bütçesi".into(), json!({"cmd": "set_finances", "transfer_budget": v})));
                        }
                    }
                });
            });
            theme::card(&mut cols[1], |ui| {
                ui.label(theme::h2("Maaş Bütçesi"));
                ui.label(theme::faint("Haftalık bin Euro cinsinden tutar"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.live.wage_input).hint_text("900").desired_width(90.0));
                    ui.label("B € / hf");
                    if theme::primary_button(ui, icon::CHECK_CIRCLE, "Ayarla", !busy).clicked() {
                        if let Ok(k) = self.live.wage_input.replace(',', ".").trim().parse::<f64>() {
                            action = Some(("maaş bütçesi".into(), json!({"cmd": "set_finances", "wage_budget": (k * 1e3 / rate as f64) as i64})));
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for add in [50, 100, 250] {
                        if theme::small_button(ui, &format!("+€{add}B"), !busy).clicked() {
                            let v = g("wage_budget") + (add as f64 * 1e3 / rate as f64) as i64;
                            action = Some(("maaş bütçesi".into(), json!({"cmd": "set_finances", "wage_budget": v})));
                        }
                    }
                });
            });
        });
        ui.add_space(8.0);
        ui.label(theme::faint(
            format!("FM dahili olarak sterlin kullanır; Euro tutarları {rate:.2} kuru ile hesaplanır (Ayarlar'dan değiştirilebilir)."),
        ));
        ui.add_space(12.0);
        // de vlaggen van FM's in-game editor op je manager: registratie en rugnummers altijd
        let hf = self.live.human_flags.clone();
        theme::card(ui, |ui| {
            ui.label(theme::h2("Oyun Kuralları"));
            ui.label(theme::faint(
                "FM oyun içi editöründeki anahtarların aynısı, doğrudan menajerinize uygulanır. FM'de turnuvanın kadro kayıt ekranından kontrol edebilirsiniz.",
            ));
            ui.add_space(6.0);
            match hf.as_ref().and_then(|v| v.get("supported")).and_then(|v| v.as_bool()) {
                Some(true) => {
                    let rows = [
                        ("registration_anytime", "Oyuncuları her zaman kaydet", "Kayıt dönemi dışında da kadro kaydı yapılabilsin (lig, UEFA)."),
                        ("numbers_anytime", "Forma numaralarını her zaman değiştir", "Turnuva forma numaralarını kilitlemiş olsa bile değiştirilebilsin."),
                    ];
                    for (key, label, hint) in rows {
                        let mut on = hf.as_ref().and_then(|v| v.get(key)).and_then(|v| v.as_bool()).unwrap_or(false);
                        ui.horizontal(|ui| {
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
                    ui.label(theme::faint("Bu FM sürümünde veya platformda henüz mevcut değil."));
                }
                None => {
                    ui.label(theme::faint("Henüz okunmadı."));
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
        let subtitle = format!("{n} oyuncu · {injured} sakat · {banned} cezalı");
        theme::page_header(ui, "Kadro", &subtitle, |ui| {
            if theme::button(ui, icon::SMILEY, "Herkesin Moralini Maksimum Yap", connected && !busy).clicked() {
                action = Some(("moral".into(), json!({"cmd": "inspire", "morale": 20, "clear_unhappiness": true})));
            }
            if theme::button(ui, icon::WARNING, "Cezaları Kaldır", connected && !busy && banned > 0).clicked() {
                action = Some(("cezalar".into(), json!({"cmd": "lift_bans"})));
            }
            let heal = format!("Herkesi İyileştir ({injured})");
            if theme::primary_button(ui, icon::FIRST_AID, &heal, connected && !busy && injured > 0).clicked() {
                action = Some(("iyileştirme".into(), json!({"cmd": "heal"})));
            }
        });
        let (m, e) = (self.live.status.clone(), self.live.status_error);
        self.status_line(ui, &m, e);
        if !connected {
            self.connect_card(ui, ctx);
            return;
        }
        // tactisch begrip (FM: Tactical Understanding), te maximaliseren zoals FM's in-game editor dat kan
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
                    ui.label(theme::h2("Taktik Anlayışı"));
                    ui.add(
                        egui::Label::new(theme::faint(
                            "Kadronun mevcut taktiğinizi alan bazında ne kadar iyi bildiği (oyuncuların ortalaması). Maksimuma çıkarma işlemi FM oyun içi editörünün yaptığını yapar: her şeyi %100'e çıkarır.",
                        ))
                        .wrap(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if theme::primary_button(ui, icon::BRAIN, "Taktik Anlayışını Maksimum Yap", connected && !busy).clicked() {
                        action = Some(("taktik anlayışı".into(), json!({"cmd": "max_understanding"})));
                    }
                });
            });
            match tu.as_ref().and_then(|u| u.get("team")).and_then(|v| v.as_array()) {
                Some(team) if !team.is_empty() => {
                    ui.add_space(6.0);
                    // twee rijen van vier: in bezit, dan zonder bal
                    for row in team.iter().take(UNDERSTANDING_AREAS.len()).enumerate().collect::<Vec<_>>().chunks(4) {
                        ui.horizontal(|ui| {
                            for (i, v) in row {
                                let pct = v.as_u64().unwrap_or(0) as f32 / 100.0;
                                theme::pill(ui, &format!("{} · {pct:.0}%", UNDERSTANDING_AREAS[*i]), theme::rgb(level_color(pct)));
                            }
                        });
                    }
                    let without = tu.as_ref().and_then(|u| u.get("without_record")).and_then(|v| v.as_u64()).unwrap_or(0);
                    if without > 0 {
                        ui.label(theme::faint(
                            format!("{without} oyuncunun henüz taktik anlayışı kaydı yok (bu takımla henüz antrenman yapmadılar); bir haftalık antrenmandan sonra dahil olurlar."),
                        ));
                    }
                }
                _ => {
                    ui.label(theme::faint("Henüz okunmadı."));
                }
            }
            // FM laat het begrip elke speelweek een stuk zakken: op verzoek houdt de app het op 100 %
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let keep = self.settings.keep_understanding;
                if theme::toggle(ui, &mut self.settings.keep_understanding).changed() {
                    self.save_settings();
                }
                ui.label(theme::label_strong("%100'de Sabit Tut"));
                ui.label(RichText::new(if keep { "AÇIK" } else { "KAPALI" }).strong().color(if keep { theme::SUCCESS } else { theme::TEXT_FAINT }));
                ui.add(
                    egui::Label::new(theme::faint(
                        "Uygulama açık olduğu sürece her 20 saniyede bir FM'nin düşürdüğü değerleri tekrar %100'e geri çeker.",
                    ))
                    .wrap(),
                );
            });
        });
        theme::card(ui, |ui| {
            egui::Grid::new("squad").num_columns(8).spacing([18.0, 10.0]).striped(true).show(ui, |ui| {
                for h in ["Oyuncu", "Moral", "Kondisyon", "Taktik", "Sakatlık", "Ceza", "Mutsuz", ""] {
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
                        .map(|a| a.iter().map(|i| i.get("name").and_then(|n| n.as_str()).unwrap_or("sakatlık").to_string()).collect())
                        .unwrap_or_default();
                    if inj.is_empty() {
                        ui.label(theme::faint("—"));
                    } else {
                        theme::pill(ui, &inj.join(", "), theme::DANGER);
                    }
                    let bans = p.get("bans").and_then(|v| v.as_u64()).unwrap_or(0);
                    if bans > 0 {
                        theme::pill(ui, "cezalı", theme::WARNING);
                    } else {
                        ui.label(theme::faint("—"));
                    }
                    let unhappy = p.get("unhappy_reasons").and_then(|v| v.as_bool()).unwrap_or(false);
                    if unhappy {
                        theme::pill(ui, "evet", theme::WARNING);
                    } else {
                        ui.label(theme::faint("—"));
                    }
                    ui.horizontal(|ui| {
                        if !inj.is_empty() && theme::small_button(ui, "İyileştir", !busy).clicked() {
                            action = Some(("iyileştirme".into(), json!({"cmd": "heal", "uids": [uid]})));
                        }
                        if (m < 20 || unhappy) && theme::small_button(ui, "Moral Ver", !busy).clicked() {
                            action = Some(("moral".into(), json!({"cmd": "inspire", "uids": [uid], "morale": 20, "clear_unhappiness": true})));
                        }
                        let coach = p.get("player_coach").and_then(|v| v.as_bool()).unwrap_or(false);
                        if !coach && theme::small_button(ui, "Sat", !busy).clicked() {
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
            "Taktik",
            "Oyuncularınızın taktiğinizdeki rollere ne kadar iyi uyduğu ve taktikle uyumlu antrenman",
            |ui| {
                if theme::primary_button(ui, icon::BARBELL, "Bu Taktiğe Göre Antrenman Uygula", connected && !busy).clicked() {
                    apply_tactic_train = true;
                }
                if theme::button(ui, icon::ARROWS_CLOCKWISE, if busy { "Okunuyor…" } else { "Yeniden Oku" }, connected && !busy).clicked() {
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
                    ui.label(theme::faint("Taktiğiniz okunuyor…"));
                });
            }
            return;
        };
        let pick = self.tactic_pick.filter(|i| *i < data.tactics.len()).unwrap_or(data.current.min(data.tactics.len() - 1));
        let tactic = &data.tactics[pick];
        let in_use = pick == data.current.min(data.tactics.len() - 1);
        let mut places = tx::places(tactic);
        if !in_use {
            for p in &mut places {
                p.uid = None;
            }
        }
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
        ui.horizontal(|ui| {
            let labels: Vec<String> = data
                .tactics
                .iter()
                .map(|t| {
                    let mut l = format!("Taktik {}", t.index + 1);
                    if t.index == data.tactics[data.current.min(data.tactics.len() - 1)].index {
                        l.push_str(" · kullanımda");
                    }
                    l
                })
                .collect();
            let options: Vec<(usize, &str)> = labels.iter().enumerate().map(|(i, l)| (i, l.as_str())).collect();
            let mut v = pick;
            if theme::segmented(ui, &mut v, &options) {
                self.tactic_pick = Some(v);
                theme::restart_enter(ctx);
            }
            if !tactic.style.is_empty() {
                theme::pill(ui, &format!("şablon {}", tactic.style), theme::TEXT_DIM);
            }
            let suggested = tx::suggest_tactic_mode(tactic);
            theme::pill(ui, &format!("Uyumlu Antrenman: {}", suggested.label()), theme::ACCENT);
        });
        ui.add_space(12.0);
        if !in_use {
            ui.add(
                egui::Label::new(theme::faint(
                    "Bu şu anda kullanımda olan taktik değil: FM ilk 11'inizi yalnızca aktif taktik için saklar. Bu nedenle aşağıda sadece bu taktik için en iyi 11 gösterilmektedir.",
                ))
                .wrap(),
            );
            ui.add_space(8.0);
        }

        ui.columns(3, |c| {
            theme::stat_tile(&mut c[0], icon::STRATEGY, "Mevcut 11", &format!("{avg:.0}"), "ortalama uyum", theme::rgb(level_color(avg)), Some(avg));
            let gain = best_avg - avg;
            theme::stat_tile(
                &mut c[1],
                icon::USERS_THREE,
                "En İyi 11",
                &format!("{best_avg:.0}"),
                &if gain >= 0.5 { format!("+{gain:.1} kazanılabilir") } else { "ilk 11'iniz zaten en iyisi".to_string() },
                theme::rgb(level_color(best_avg)),
                Some(best_avg),
            );
            let (wname, wval) = weakest
                .map(|(i, f)| (format!("{} · {}", places[i].line.label(places[i].side), places[i].uid.and_then(|u| by_uid.get(&u)).map(|p| p.name.as_str()).unwrap_or("?")), f))
                .unwrap_or(("ilk 11 seçilmedi".to_string(), 0.0));
            theme::stat_tile(&mut c[2], icon::WARNING, "En Zayıf Mevki", &format!("{wval:.0}"), &wname, theme::rgb(level_color(wval)), Some(wval));
        });
        ui.add_space(14.0);

        // de elf plekken
        theme::card(ui, |ui| {
            ui.label(theme::h2("Bu Taktikteki İlk 11'iniz"));
            ui.add_space(6.0);
            egui::Grid::new("tactic_slots").num_columns(6).spacing([18.0, 10.0]).striped(true).show(ui, |ui| {
                for h in ["Mevki", "Topla / Topsuz Rol", "Oyuncu", "Uyum", "Pozisyon", "Kadrodaki Daha İyi Alternatif"] {
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
                                    theme::pill(ui, "sakat", theme::DANGER);
                                }
                                if p.banned {
                                    theme::pill(ui, "cezalı", theme::WARNING);
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
                            ui.label(theme::faint("boş"));
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
            ui.label(theme::h2("Bu Taktik İçin En İyi 11"));
            ui.add(
                egui::Label::new(theme::faint(
                    "Müsait oyuncularınızın en yüksek toplam uyumu sağlayacak şekilde 11 mevkiye dağılımı. Sakat ve cezalı oyuncular dahil edilmez.",
                ))
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
                    ui.horizontal(|ui| {
                        ui.scope(|ui| {
                            ui.set_width(90.0);
                            ui.label(theme::label_strong(pl.line.label(pl.side)));
                        });
                        ui.label(theme::dim(now.map(|p| p.name.as_str()).unwrap_or("boş")));
                        ui.label(theme::ic(icon::ARROW_RIGHT).color(theme::TEXT_FAINT));
                        ui.label(theme::label_strong(&b.name));
                        let (bf, nf) = (tx::fit(b, pl), now.map(|p| tx::fit(p, pl)));
                        theme::pill(ui, &match nf { Some(nf) => format!("{nf:.0} → {bf:.0}"), None => format!("{bf:.0}") }, theme::rgb(level_color(bf)));
                        if in_eleven.contains(&b.uid) {
                            ui.label(theme::faint("(mevki değiştirir)"));
                        }
                    });
                }
            }
            if changes == 0 {
                ui.label(theme::dim("Bu hesaplamaya göre mevcut ilk 11'iniz zaten en iyisi."));
            }
        });

        // een rol die beter bij de speler past
        let role_tips: Vec<(String, &'static str, &'static str, &'static str, f32, f32)> = order
            .iter()
            .filter_map(|&i| {
                let pl = &places[i];
                let p = pl.uid.and_then(|u| by_uid.get(&u))?;
                let (r, now, then) = tx::better_role(p, pl)?;
                Some((p.name.clone(), tx::ip_role(pl.ip?)?.code, r.code, r.name, now, then))
            })
            .collect();
        theme::card(ui, |ui| {
            ui.label(theme::h2("Daha İyi Uyan Roller"));
            ui.add(
                egui::Label::new(theme::faint(
                    "Aynı hattaki oyuncunun özelliklerine belirgin şekilde daha iyi uyan başka bir topla oynama rolü.",
                ))
                .wrap(),
            );
            ui.add_space(6.0);
            if role_tips.is_empty() {
                ui.label(theme::dim("Yok: her oyuncu kendisine uygun bir rolde oynuyor."));
            }
            for (name, cur, code, full, now, then) in &role_tips {
                ui.horizontal(|ui| {
                    ui.label(theme::label_strong(name));
                    theme::pill(ui, &format!("{cur} {now:.0}"), theme::TEXT_DIM);
                    ui.label(theme::ic(icon::ARROW_RIGHT).color(theme::TEXT_FAINT));
                    theme::pill(ui, &format!("{code} {then:.0}"), theme::SUCCESS).on_hover_text(*full);
                    ui.label(theme::faint(*full));
                });
            }
        });

        ui.add(
            egui::Label::new(theme::faint(
                "Roller, mevkiler ve özellikler FM'den alınır. Hangi özelliklerin hangi role ait olduğu bizim tahminimizdir ve sol/sağ bilgisi mevkilerin sırasından türetilmiştir: puanı FM'nin resmi bir verisi değil, bir rehber olarak görün. FM içinde hiçbir şey değiştirilmez.",
            ))
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
            "Transferler",
            if buying {
                "Başka bir kulüpten bir oyuncuyu doğrudan A takımınıza getirin. Bonservis bedelini siz ödersiniz."
            } else {
                "Bir oyuncuyu istediğiniz bir kulübe doğrudan satın. Bonservis bedeli anında hesaba işlenir."
            },
            |ui| {
                if theme::button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Geri Al", connected && !busy).clicked() {
                    undo = true;
                }
            },
        );
        if undo {
            self.live.sell.plan = None;
            self.live.refresh(ctx, "geri al", Some(json!({"cmd": "undo"})));
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
        if theme::segmented(ui, &mut buy, &[(false, "Sat"), (true, "Satın Al")]) {
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
                ui.label(theme::h2("1 · Oyuncu"));
                ui.label(theme::faint("Satmak istediğiniz oyuncuyu seçin."));
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
                            let label = if coach { format!("{name} · oyuncu-antrenör") } else { name.to_string() };
                            let r = ui.add_enabled(!coach, egui::SelectableLabel::new(selected, RichText::new(label).size(14.0)));
                            if r.clicked() {
                                pick = Some(uid);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if let Some(y) = until {
                                    ui.label(theme::faint(format!("bitiş: {y}")));
                                }
                                ui.label(RichText::new(eur(value)).color(theme::TEXT));
                                if p.get("listed").and_then(|v| v.as_u64()).unwrap_or(0) & 1 == 1 {
                                    theme::pill(ui, "transfer listesinde", theme::WARNING);
                                }
                            });
                        });
                    }
                });
            });

            // 2. koper en 3. bedrag
            let ui = &mut cols[1];
            theme::card(ui, |ui| {
                ui.label(theme::h2("2 · Alıcı Kulüp"));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut self.live.sell.query).hint_text("Bir kulüp arayın, örn. AC Milan").desired_width(260.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        search = true;
                    }
                    if theme::small_button(ui, "Ara", !busy && self.live.sell.query.trim().len() >= 2).clicked() {
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
                            theme::pill(ui, "kadınlar", theme::INFO);
                        }
                    });
                }
                if clubs.is_empty() && !self.live.sell.searched.is_empty() && !self.live.sell_busy() {
                    ui.label(theme::faint("Kulüp bulunamadı."));
                }
            });
            ui.add_space(12.0);
            theme::card(ui, |ui| {
                ui.label(theme::h2("3 · Bonservis Bedeli"));
                ui.add_space(4.0);
                let value = chosen.as_ref().and_then(|p| p.get("value")).and_then(|v| v.as_i64()).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.live.sell.fee_input).hint_text("10").desired_width(90.0));
                    ui.label("Mn €");
                    for (label, f) in [("değeri", 1.0), ("×1.5", 1.5), ("×2", 2.0), ("×3", 3.0)] {
                        if theme::small_button(ui, label, value > 0).clicked() {
                            self.live.sell.fee_input = format!("{:.1}", value as f64 * f * rate / 1e6);
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Yeni maaş");
                    ui.add(egui::TextEdit::singleline(&mut self.live.sell.wage_input).hint_text("değişmez").desired_width(90.0));
                    ui.label("B € / hf");
                });
                let fee = self.live.sell.fee_input.replace(',', ".").trim().parse::<f64>().ok().map(|m| (m * 1e6 / rate).round() as i64);
                let wage = self.live.sell.wage_input.replace(',', ".").trim().parse::<f64>().ok().map(|k| (k * 1e3 / rate).round() as i64);
                let key = match (self.live.sell.player, &self.live.sell.club, fee) {
                    (Some(p), Some((c, _)), Some(f)) if f >= 0 => Some((p, *c, f, wage)),
                    _ => None,
                };
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if theme::button(ui, icon::MAGNIFYING_GLASS, "Kontrol Et", key.is_some() && !busy).clicked() {
                        check = key;
                    }
                    let ready = self.live.sell.plan.as_ref().is_some_and(|(v, k)| Some(*k) == key && v.get("allowed").and_then(|a| a.as_bool()) == Some(true));
                    let label = match (&chosen, &self.live.sell.club) {
                        (Some(p), Some((_, c))) => format!("{} oyuncusunu {} kulübüne sat", p.get("name").and_then(|v| v.as_str()).unwrap_or("?"), c),
                        _ => "Sat".to_string(),
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
                ui.label(theme::h2("1 · Kulüp"));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut self.live.sell.query).hint_text("Bir kulüp arayın, örn. Real Madrid").desired_width(260.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        search = true;
                    }
                    if theme::small_button(ui, "Ara", !busy && self.live.sell.query.trim().len() >= 2).clicked() {
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
                            theme::pill(ui, "kadınlar", theme::INFO);
                        }
                    });
                }
                if clubs.is_empty() && !self.live.sell.searched.is_empty() && !self.live.sell_busy() {
                    ui.label(theme::faint("Kulüp bulunamadı."));
                }
            });
            cols[0].add_space(12.0);
            // 2. speler
            theme::card(&mut cols[0], |ui| {
                ui.label(theme::h2("2 · Oyuncu"));
                match &club {
                    None => {
                        ui.label(theme::faint("Önce bir kulüp seçin."));
                    }
                    Some((_, name)) => {
                        ui.label(theme::faint(format!("{name} A takımı, en değerliler başta.")));
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
                                    let label = if coach { format!("{pname} · oyuncu-antrenör") } else { pname.to_string() };
                                    if ui.add_enabled(!coach, egui::SelectableLabel::new(selected, RichText::new(label).size(14.0))).clicked() {
                                        pick = Some((uid, value, wage));
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if let Some(y) = until {
                                            ui.label(theme::faint(format!("bitiş: {y}")));
                                        }
                                        if let Some(w) = wage {
                                            ui.label(theme::faint(format!("{} / hf", eur(w))));
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
                ui.label(theme::h2("3 · Bonservis Bedeli"));
                ui.add_space(4.0);
                let value = chosen.as_ref().and_then(|p| p.get("value")).and_then(|v| v.as_i64()).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.live.sell.fee_input).hint_text("10").desired_width(90.0));
                    ui.label("Mn €");
                    for (label, f) in [("değeri", 1.0), ("×0.5", 0.5), ("×1.5", 1.5), ("0", 0.0)] {
                        if theme::small_button(ui, label, value > 0).clicked() {
                            self.live.sell.fee_input = format!("{:.1}", value as f64 * f * rate / 1e6);
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Sizdeki maaşı");
                    ui.add(egui::TextEdit::singleline(&mut self.live.sell.wage_input).hint_text("değişmez").desired_width(90.0));
                    ui.label("B € / hf");
                });
                let fee = self.live.sell.fee_input.replace(',', ".").trim().parse::<f64>().ok().map(|m| (m * 1e6 / rate).round() as i64);
                let wage = self.live.sell.wage_input.replace(',', ".").trim().parse::<f64>().ok().map(|k| (k * 1e3 / rate).round() as i64);
                let key = match (self.live.sell.buy_player, &self.live.sell.club, fee) {
                    (Some(p), Some((c, _)), Some(f)) if f >= 0 => Some((p, *c, f, wage)),
                    _ => None,
                };
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if theme::button(ui, icon::MAGNIFYING_GLASS, "Kontrol Et", key.is_some() && !busy).clicked() {
                        check = key;
                    }
                    let ready = self.live.sell.plan.as_ref().is_some_and(|(v, k)| Some(*k) == key && v.get("allowed").and_then(|a| a.as_bool()) == Some(true));
                    let label = match (&chosen, &club) {
                        (Some(p), Some((_, c))) => format!("{} oyuncusunu {} kulübünden satın al", p.get("name").and_then(|v| v.as_str()).unwrap_or("?"), c),
                        _ => "Satın Al".to_string(),
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
        theme::page_header(ui, "Ayarlar", "", |_| {});
        let mut dirty = false;
        ui.columns(2, |cols| {
            theme::card(&mut cols[0], |ui| {
                ui.label(theme::h2("Genel"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(theme::ic(icon::TRANSLATE).color(theme::ACCENT));
                    ui.label("Uygulama Dili:");
                    theme::pill(ui, "Türkçe (Tam Sürüm)", theme::SUCCESS);
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(theme::ic(icon::COINS).color(theme::ACCENT));
                    ui.label("£ Başına € Kuru");
                    dirty |= ui.add(egui::DragValue::new(&mut self.settings.rate).range(0.5..=2.0).speed(0.01).fixed_decimals(2)).changed();
                });
                dirty |= ui.checkbox(&mut self.settings.capture_delay, "Okumadan önce FM penceresini öne getir").changed();
            });
            theme::card(&mut cols[1], |ui| {
                ui.label(theme::h2("Planlayıcı & Taktik Antrenmanı"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Taktik Tarzı:");
                    egui::ComboBox::from_id_salt("settings_tactic_mode")
                        .selected_text(self.settings.planner.tactic_mode.label())
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for m in planner::TacticMode::ALL {
                                if ui.selectable_value(&mut self.settings.planner.tactic_mode, m, m.label()).changed() {
                                    dirty = true;
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Oyuncu Benimsemesi şu değerin altındayken Takım Kaynaşması ekle:");
                    dirty |= ui.add(egui::Slider::new(&mut self.settings.planner.bonding_below, 0.0..=100.0).fixed_decimals(0)).changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Duran toplar kaç haftada bir (0 = kapalı)");
                    dirty |= ui.add(egui::DragValue::new(&mut self.settings.planner.set_pieces_every).range(0..=8)).changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Hedefleri kaç haftada bir güncelle");
                    dirty |= ui.add(egui::DragValue::new(&mut self.settings.planner.replan_every_weeks).range(1..=8)).changed();
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Dinlenme Yükü");
                    use planner::Load;
                    dirty |= theme::segmented(
                        ui,
                        &mut self.settings.planner.load,
                        &[(Load::Light, "Daha Fazla Dinlenme"), (Load::Balanced, "Dengeli"), (Load::Intense, "Daha Az Dinlenme")],
                    );
                });
                ui.add(
                    egui::Label::new(theme::dim(match self.settings.planner.load {
                        planner::Load::Light => "Her antrenman günü ve maçtan önceki gün Dinlenme ile biter.",
                        planner::Load::Balanced => "Standart dengeli plan.",
                        planner::Load::Intense => "Maçtan sonraki gün Yenilenme sonrasında bir de taktik seansı eklenir.",
                    }))
                    .wrap(),
                );
                ui.add_space(6.0);
                dirty |= ui
                    .checkbox(&mut self.settings.planner.no_tactical, "Taktiksel seans ekleme (Sadece Oyuncu Gelişimi)")
                    .changed();
                ui.add(
                    egui::Label::new(theme::dim(
                        "Bu yuvalar seçilen taktik tarzına uygun oyuncu gelişimine ayrılır; maç hazırlığı kalır. Taktik anlayışını \"%100'de Sabit Tut\" (Kadro) özelliğiyle kullananlar içindir.",
                    ))
                    .wrap(),
                );
            });
        });
        ui.add_space(14.0);
        if agent::LIVE_SUPPORTED {
            theme::card(ui, |ui| {
                ui.label(theme::h2("FM İçinde Canlı Bağlantı"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let mut on = self.settings.allow_untested;
                    if theme::toggle(ui, &mut on).changed() {
                        self.settings.allow_untested = on;
                        self.live.allow_untested = on;
                        dirty = true;
                        if self.live.connected {
                            self.live.refresh(ui.ctx(), "", None);
                        }
                    }
                    ui.label(RichText::new("Tüm FM27 sürümlerinde canlı düzenlemeye izin ver").strong());
                });
                ui.add(egui::Label::new(theme::dim(
                    "FM27 güncellemelerinden sonra da tüm canlı özelliklerin (antrenman, bütçe, moral, sakatlık, transfer) kesintisiz çalışmasını sağlar. Her değişiklik yazılmadan önce kontrol edilir, geri alınabilir ve yalnızca FM içinde kaydettiğinizde kalıcı olur.",
                ))
                .wrap());
                if self.live.connected {
                    ui.add_space(4.0);
                    let (c, txt) = if !self.live.untested_build {
                        (theme::SUCCESS, "Bu FM sürümü tam uyumlu ve doğrulanmıştır.")
                    } else if self.settings.allow_untested {
                        (theme::SUCCESS, "Canlı düzenleme aktif: tüm özellikler kullanılabilir.")
                    } else {
                        (theme::WARNING, "Salt okunur mod: değişiklik yapmak için yukarıdaki anahtarı açın.")
                    };
                    ui.horizontal(|ui| {
                        theme::status_dot(ui, c);
                        ui.label(RichText::new(txt).color(theme::TEXT));
                    });
                }
            });
            ui.add_space(14.0);
        }
        theme::card(ui, |ui| {
            ui.label(theme::h2("Klasörler ve İzinler"));
            ui.add_space(6.0);
            ui.label(theme::dim(format!("Dışa Aktarım: {}", export::output_dir().display())));
            ui.label(theme::dim(format!("Ayarlar: {}", crate::data_dir().display())));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if theme::button(ui, icon::EXPORT, "Dışa aktarım klasörünü aç", true).clicked() {
                    let _ = std::fs::create_dir_all(export::output_dir());
                    open_path(&export::output_dir());
                }
                if theme::button(ui, icon::EXPORT, "Uygulama klasörünü aç (son ekran görüntüsü ve okuma)", true).clicked() {
                    let _ = std::fs::create_dir_all(crate::data_dir());
                    open_path(&crate::data_dir());
                }
                if cfg!(target_os = "macos") && theme::button(ui, icon::CAMERA, "Ekran Kaydı İzni", true).clicked() {
                    open_path(std::path::Path::new("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"));
                }
            });
        });
        ui.add_space(14.0);
        theme::card(ui, |ui| {
            ui.label(theme::h2("İletişim"));
            ui.add_space(6.0);
            ui.add(egui::Label::new(theme::dim(
                "Sorularınız, geri bildirimleriniz veya destek için e-posta üzerinden iletişime geçebilir ya da Kick kanalımı takip edebilirsiniz.",
            ))
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
            });
        });
        ui.add_space(14.0);
        theme::card(ui, |ui| {
            ui.label(theme::h2("Hakkında"));
            ui.add(egui::Label::new(theme::dim(
                "Seans adları, limitler ve şablonlar FM27'nin kendi oyun verilerinden alınmıştır. Modül yalnızca kontrolden sonra okur ve yazar, geri alma (undo) desteklidir. Yalnızca tek oyunculu (single-player) kullanım içindir. Yazı tipi: Inter (SIL OFL). İkonlar: Phosphor (MIT).",
            ))
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
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(1400.0));
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
    match m {
        0..=4 => "Berbat",
        5..=7 => "Çok Kötü",
        8..=10 => "Kötü",
        11..=13 => "İdare Eder",
        14..=16 => "İyi",
        17..=18 => "Çok İyi",
        _ => "Mükemmel",
    }
}

/// Kısa, okunabilir Türkçe antrenman seansı adı (kartlar ve etiketler için).
fn chip_name(name: &str) -> String {
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
        ui.horizontal(|ui| {
            ui.label(theme::h2(format!("{} Haftası", i18n::date_short(w.start))));
            if w.rest {
                theme::pill(ui, "Dinlenme haftası: FM'ye bırakıldı", theme::WARNING);
                return;
            }
            theme::pill(ui, &format!("{} maç", w.matches), theme::DANGER);
            theme::pill(ui, crate::model::focus_tr(&w.focus), theme::ACCENT);
            let pending = (0..7).filter(|d| w.current[*d] != w.planned[*d]).count();
            if w.written > 0 {
                theme::pill(ui, &format!("{} gün geri yüklendi", w.written), theme::SUCCESS);
            } else if pending > 0 {
                theme::pill(ui, &format!("{pending} gün planınızdan farklı"), theme::WARNING);
            } else {
                theme::pill(ui, "plana uygun", theme::SUCCESS);
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
                // wat FM wilde: nu afwijkend (wordt aangepast) of eerder teruggezet
                let differs = w.current[d] != w.planned[d];
                let was = if differs { Some(w.current[d]) } else { fm_had.get(&date).copied().filter(|h| *h != w.planned[d]) };
                for code in w.planned[d] {
                    let name = session_by_code(code).unwrap_or("?");
                    chip(ui, name, was.is_some() && !past, past || !w.editable[d]);
                }
                if let Some(h) = was.filter(|_| !past && w.editable[d]) {
                    ui.add(egui::Label::new(RichText::new(format!("FM: {}", codes_text(&h))).size(10.5).color(theme::TEXT_FAINT).strikethrough()).truncate())
                        .on_hover_text(format!("FM'nin planladığı: {}", codes_text(&h)));
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
        ui.label(theme::faint("Çerçeveli = FM27 Trainer tarafından ayarlandı · üstü çizili = FM'nin istediği ·"));
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
    let names = [(Weekday::Fri, "Cuma"), (Weekday::Sat, "Cumartesi"), (Weekday::Sun, "Pazar")];
    let cur = names.iter().find(|(d, _)| d == day).map(|(_, n)| *n).unwrap_or("—");
    egui::ComboBox::from_id_salt(id).selected_text(cur).show_ui(ui, |ui| {
        for (d, n) in names {
            ui.selectable_value(day, d, n);
        }
    });
}
