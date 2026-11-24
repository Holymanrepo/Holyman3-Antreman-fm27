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
    /// Her güncellemede bireysel antrenmanları (mevki uyumu, zayıf ayak, rol odakları, kondisyon) otomatik uygula.
    #[serde(default = "yes")]
    auto_individual: bool,
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
    /// Easy Talk (Antrenman Görüşmeleri) eşikleri ve takım ayarları.
    #[serde(default)]
    easy_talk: crate::tactics::EasyTalkSettings,
    /// Oyuncu yüzlerinin (Facepack) bulunduğu klasör.
    #[serde(default = "default_faces_dir")]
    faces_dir: String,
    /// FM27 graphics ana klasörü (kulüp logoları, formalar, yüzler vb. için).
    #[serde(default = "default_graphics_dir")]
    graphics_dir: String,
    /// İlk kurulum / ilk açılış dil seçimi tamamlandı mı?
    #[serde(default)]
    lang_chosen: bool,
    /// Kayıtlı İlk 11 Şablonları (en fazla 10 şablon).
    #[serde(default)]
    saved_lineups: Vec<crate::tactics::SavedLineup>,
    planner: PlannerSettings,
}

fn user_documents_dir() -> PathBuf {
    for var in ["OneDriveConsumer", "OneDrive"] {
        if let Ok(od) = std::env::var(var) {
            let docs = PathBuf::from(&od).join("Documents");
            if docs.join("Sports Interactive").exists() {
                return docs;
            }
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return PathBuf::from(profile).join("Documents");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join("Documents");
    }
    PathBuf::from("Documents")
}

fn default_graphics_dir() -> String {
    let docs = user_documents_dir();
    let p27 = docs.join("Sports Interactive\\Football Manager 27\\graphics");
    if p27.exists() {
        return p27.to_string_lossy().into_owned();
    }
    let p26 = docs.join("Sports Interactive\\Football Manager 26\\graphics");
    if p26.exists() {
        return p26.to_string_lossy().into_owned();
    }
    p27.to_string_lossy().into_owned()
}

fn default_faces_dir() -> String {
    let docs = user_documents_dir();
    let p27 = docs.join("Sports Interactive\\Football Manager 27\\graphics\\faces");
    if p27.exists() {
        return p27.to_string_lossy().into_owned();
    }
    let p26 = docs.join("Sports Interactive\\Football Manager 26\\graphics\\faces");
    if p26.exists() {
        return p26.to_string_lossy().into_owned();
    }
    p27.to_string_lossy().into_owned()
}

fn pick_folder_dialog(initial: &str) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let escaped = initial.replace('\'', "''");
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.SelectedPath = '{}'; if ($f.ShowDialog() -eq 'OK') {{ [Console]::Out.Write($f.SelectedPath) }}",
            escaped
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

impl Default for Settings {
    fn default() -> Self {
        Settings { lang: i18n::detect(), rate: 1.17, capture_delay: true, auto_training: true, allow_untested: true, keep_understanding: false, auto_individual: true, auto_read: false, auto_read_secs: 10, manager_club: None, training_team: None, easy_talk: crate::tactics::EasyTalkSettings::default(), faces_dir: default_faces_dir(), graphics_dir: default_graphics_dir(), lang_chosen: false, saved_lineups: Vec::new(), planner: PlannerSettings::default() }
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
    EasyTalk,
    Planning,
    Squad,
    Tactics,
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
    selected_player: Option<u32>,
    easy_talk_settings_open: bool,
    easy_talk_cutoff_team: usize,
    easy_talk_show_summary: bool,
    easy_talk_show_role_focus: bool,
    face_textures: std::collections::HashMap<u32, Option<egui::TextureHandle>>,
    club_logo_textures: std::collections::HashMap<u32, Option<egui::TextureHandle>>,
    new_lineup_name: String,
    swap_training_slot: Option<(NaiveDate, usize, usize)>,
    frames: u32,
    logo_tex: Option<egui::TextureHandle>,
    update_rx: Option<mpsc::Receiver<Result<Option<ReleaseInfo>, String>>>,
    latest_release: Option<ReleaseInfo>,
    update_status: String,
    last_update_check: std::time::Instant,
    dismissed_release_tag: Option<String>,
    self_update_rx: Option<mpsc::Receiver<Result<(), String>>>,
    self_updating: bool,
}

#[derive(Clone, Debug)]
struct ReleaseInfo {
    tag: String,
    html_url: String,
    exe_url: Option<String>,
    notes: String,
}

const GITHUB_REPO_URL: &str = "https://github.com/Holymanrepo/Holyman3-Antreman-fm27";
const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/Holymanrepo/Holyman3-Antreman-fm27/releases/latest";

fn find_club_logo_in_graphics(graphics_dir: &str, uid: u32) -> Option<PathBuf> {
    let base = PathBuf::from(graphics_dir.trim());
    if !base.is_dir() {
        return None;
    }
    let fname = format!("{uid}.png");
    let direct_candidates = [
        base.join("Logos TCM24\\1-Team\\Clubs\\normal").join(&fname),
        base.join("Logos TCM24\\1-Team\\Clubs\\small").join(&fname),
        base.join("logos\\normal").join(&fname),
        base.join("logos").join(&fname),
    ];
    for c in direct_candidates {
        if c.is_file() {
            return Some(c);
        }
    }

    fn search_dir(dir: &std::path::Path, target_name: &str, depth: usize, fallback: &mut Option<PathBuf>) -> Option<PathBuf> {
        if depth > 6 {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if path.is_file() {
                if name_str.eq_ignore_ascii_case(target_name) {
                    let full = path.to_string_lossy().to_ascii_lowercase();
                    if full.contains("clubs") && full.contains("normal") && !full.contains("@2x") {
                        return Some(path);
                    }
                    if fallback.is_none() || (full.contains("clubs") && !full.contains("small")) {
                        *fallback = Some(path);
                    }
                }
            } else if path.is_dir() {
                let low = name_str.to_ascii_lowercase();
                if matches!(
                    low.as_str(),
                    "faces"
                        | "iconfaces"
                        | "kits"
                        | "city pics"
                        | "stadium"
                        | "trophies"
                        | "fmve stadium"
                        | "fmve stadium links"
                        | "fmve trophies"
                        | "fmve trophies links"
                        | "sortitoutsi"
                ) {
                    continue;
                }
                subdirs.push(path);
            }
        }
        for sub in subdirs {
            if let Some(found) = search_dir(&sub, target_name, depth + 1, fallback) {
                return Some(found);
            }
        }
        None
    }

    let mut fallback = None;
    let tcm = base.join("Logos TCM24");
    if tcm.is_dir() {
        if let Some(p) = search_dir(&tcm, &fname, 0, &mut fallback) {
            return Some(p);
        }
        if fallback.is_some() {
            return fallback;
        }
    }
    search_dir(&base, &fname, 0, &mut fallback).or(fallback)
}

fn is_newer_version(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .trim()
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|p| p.parse::<u32>().ok())
            .collect()
    };
    let r = parse(remote);
    let l = parse(local);
    let len = r.len().max(l.len());
    for i in 0..len {
        let rv = r.get(i).copied().unwrap_or(0);
        let lv = l.get(i).copied().unwrap_or(0);
        if rv > lv {
            return true;
        }
        if rv < lv {
            return false;
        }
    }
    false
}

fn spawn_update_check() -> mpsc::Receiver<Result<Option<ReleaseInfo>, String>> {
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
                        let html_url = v
                            .get("html_url")
                            .and_then(|u| u.as_str())
                            .unwrap_or("https://github.com/Holymanrepo/Holyman3-Antreman-fm27/releases/latest")
                            .to_string();
                        let notes = v
                            .get("body")
                            .and_then(|b| b.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        let exe_url = v
                            .get("assets")
                            .and_then(|a| a.as_array())
                            .and_then(|arr| {
                                arr.iter()
                                    .find_map(|asset| {
                                        let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("").to_ascii_lowercase();
                                        if name.ends_with(".exe") && !name.contains("setup") {
                                            asset.get("browser_download_url").and_then(|u| u.as_str()).map(|s| s.to_string())
                                        } else {
                                            None
                                        }
                                    })
                                    .or_else(|| {
                                        arr.iter().find_map(|asset| {
                                            let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("");
                                            if name.to_ascii_lowercase().ends_with(".exe") {
                                                asset.get("browser_download_url").and_then(|u| u.as_str()).map(|s| s.to_string())
                                            } else {
                                                None
                                            }
                                        })
                                    })
                            });
                        if !clean.is_empty() && is_newer_version(clean, current) {
                            Ok(Some(ReleaseInfo {
                                tag: tag.to_string(),
                                html_url,
                                exe_url,
                                notes,
                            }))
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

fn spawn_self_update(exe_url: String) -> mpsc::Receiver<Result<(), String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let res = (|| -> Result<(), String> {
            let cur_exe = std::env::current_exe().map_err(|e| format!("Uygulama yolu alınamadı: {e}"))?;
            let new_exe = cur_exe.with_extension("new.exe");
            let old_exe = cur_exe.with_extension("old.exe");

            #[cfg(windows)]
            use std::os::windows::process::CommandExt;
            let mut cmd = std::process::Command::new("curl");
            cmd.args([
                "-fL",
                "--max-time",
                "120",
                "-H",
                "User-Agent: FM27-Trainer-Holyman3",
                "-o",
            ]);
            cmd.arg(&new_exe);
            cmd.arg(&exe_url);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);

            let out = cmd.output().map_err(|e| format!("İndirme başlatılamadı: {e}"))?;
            if !out.status.success() {
                let _ = std::fs::remove_file(&new_exe);
                return Err(t!("Yeni sürüm indirilemedi.", "Failed to download new version.").into());
            }
            let meta = std::fs::metadata(&new_exe).map_err(|e| format!("İndirilen dosya okunamadı: {e}"))?;
            if meta.len() < 500_000 {
                let _ = std::fs::remove_file(&new_exe);
                return Err(t!("İndirilen dosya eksik veya bozuk.", "Downloaded file is incomplete or corrupted.").into());
            }

            let _ = std::fs::remove_file(&old_exe);
            std::fs::rename(&cur_exe, &old_exe).map_err(|e| format!("Mevcut dosya yedeklenemedi: {e}"))?;
            if let Err(e) = std::fs::rename(&new_exe, &cur_exe) {
                let _ = std::fs::rename(&old_exe, &cur_exe);
                return Err(format!("Yeni sürüm yerleştirilemedi: {e}"));
            }

            let _ = std::process::Command::new(&cur_exe).spawn();
            Ok(())
        })();
        let _ = tx.send(res);
    });
    rx
}

impl App {
    fn new() -> Self {
        if let Ok(cur_exe) = std::env::current_exe() {
            let _ = std::fs::remove_file(cur_exe.with_extension("old.exe"));
        }
        let mut settings: Settings = std::fs::read_to_string(settings_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        if settings.graphics_dir.trim().is_empty() || !PathBuf::from(&settings.graphics_dir).exists() {
            settings.graphics_dir = default_graphics_dir();
        }
        if settings.faces_dir.trim().is_empty() || !PathBuf::from(&settings.faces_dir).exists() {
            let sub = PathBuf::from(&settings.graphics_dir).join("faces");
            if sub.exists() {
                settings.faces_dir = sub.to_string_lossy().into_owned();
            } else {
                settings.faces_dir = default_faces_dir();
            }
        }
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
            "easytalk" => Page::EasyTalk,
            "planning" => Page::Planning,
            "squad" => Page::Squad,
            "tactics" => Page::Tactics,
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
            selected_player: None,
            easy_talk_settings_open: false,
            easy_talk_cutoff_team: 0,
            easy_talk_show_summary: false,
            easy_talk_show_role_focus: false,
            face_textures: std::collections::HashMap::new(),
            club_logo_textures: std::collections::HashMap::new(),
            new_lineup_name: String::new(),
            swap_training_slot: None,
            live: LiveState::default(),
            frames: 0,
            logo_tex: None,
            update_rx: Some(spawn_update_check()),
            latest_release: None,
            update_status: String::new(),
            last_update_check: std::time::Instant::now(),
            dismissed_release_tag: None,
            self_update_rx: None,
            self_updating: false,
        }
    }

    fn poll_update(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.self_update_rx {
            if let Ok(res) = rx.try_recv() {
                self.self_update_rx = None;
                self.self_updating = false;
                match res {
                    Ok(()) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                    Err(e) => {
                        self.update_status = e;
                    }
                }
            }
        }

        // Her 10 dakikada bir otomatik arka plan güncelleme kontrolü
        if self.update_rx.is_none()
            && !self.self_updating
            && std::env::var_os("FM27_SNAPSHOT").is_none()
            && self.last_update_check.elapsed().as_secs() >= 600
        {
            self.last_update_check = std::time::Instant::now();
            self.update_rx = Some(spawn_update_check());
        }

        let Some(rx) = &self.update_rx else { return };
        if let Ok(res) = rx.try_recv() {
            self.update_rx = None;
            match res {
                Ok(Some(info)) => {
                    self.update_status = t!(format!("Yeni sürüm mevcut: {}", info.tag), format!("New version available: {}", info.tag));
                    self.latest_release = Some(info);
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
                    (Page::Squad, icon::USERS_THREE, t!("Kadro", "Squad")),
                    (Page::Tactics, icon::STRATEGY, t!("Taktik", "Tactics")),
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
                    if let Some(rel) = self.latest_release.clone() {
                        ui.add_space(6.0);
                        let lbl = if self.self_updating {
                            t!("Güncelleniyor…", "Updating…").to_string()
                        } else {
                            t!(format!("Yeni sürüm ({}) · Güncelle", rel.tag), format!("New version ({}) · Update", rel.tag))
                        };
                        if theme::primary_button(
                            ui,
                            icon::ARROW_CLOCKWISE,
                            &lbl,
                            !self.self_updating,
                        )
                        .clicked()
                        {
                            self.dismissed_release_tag = None;
                            if let Some(exe) = rel.exe_url {
                                self.self_updating = true;
                                self.update_status = t!("Yeni sürüm indiriliyor ve kuruluyor…", "Downloading and installing new version…").into();
                                self.self_update_rx = Some(spawn_self_update(exe));
                            } else {
                                open_url(&rel.html_url);
                            }
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
                            theme::paint_discord_logo(ui.painter(), logo_rect);
                            if ui.add(egui::Label::new(RichText::new("Discord · Holyman3").size(12.5).color(Color32::from_rgb(138, 150, 255))).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                open_url(DISCORD_URL);
                            }
                        });
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
                    let club_uid = self.live.club_uid();
                    if let Some(cuid) = club_uid {
                        if !self.club_logo_textures.contains_key(&cuid) {
                            let loaded = (|| -> Option<egui::TextureHandle> {
                                let path = find_club_logo_in_graphics(&self.settings.graphics_dir, cuid)?;
                                let img = image::open(&path).ok()?.into_rgba8();
                                let (w, h) = img.dimensions();
                                let cimg = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
                                Some(ctx.load_texture(format!("club_logo_{cuid}"), cimg, egui::TextureOptions::LINEAR))
                            })();
                            self.club_logo_textures.insert(cuid, loaded);
                        }
                    }
                    let club_logo = club_uid.and_then(|u| self.club_logo_textures.get(&u)).and_then(|o| o.clone());

                    Frame::new().fill(theme::SURFACE).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(12)).show(ui, |ui| {
                        ui.set_width(sb_w2);
                        ui.set_max_width(sb_w2);
                        let (c, txt) = if !agent::LIVE_SUPPORTED {
                            (theme::TEXT_FAINT, t!("Canlı: desteklenmiyor", "Live: unsupported"))
                        } else if self.live.connected {
                            (theme::SUCCESS, t!("FM27'ye Bağlı", "Connected to FM27"))
                        } else {
                            (theme::TEXT_FAINT, t!("Bağlı değil", "Not connected"))
                        };
                        if let Some(tex) = club_logo {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                ui.add(egui::Image::from_texture(&tex).fit_to_exact_size(vec2(34.0, 34.0)));
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 3.0;
                                    if let Some(n) = self.live.club_name() {
                                        ui.add(egui::Label::new(RichText::new(n).size(13.0).strong().color(theme::TEXT)).truncate());
                                    }
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        theme::status_dot(ui, c);
                                        ui.label(RichText::new(txt).size(12.0).color(theme::TEXT_DIM));
                                    });
                                });
                            });
                        } else {
                            ui.horizontal(|ui| {
                                theme::status_dot(ui, c);
                                ui.label(RichText::new(txt).size(13.0).color(theme::TEXT));
                            });
                            if let Some(n) = self.live.club_name() {
                                ui.label(theme::faint(n));
                            }
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

        // ---- Taktikle Uyumlu Antrenman Programı & Özel Kamp Modu Seçici
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::ic(icon::STRATEGY).color(theme::ACCENT).size(20.0));
                ui.label(theme::h2(t!("Taktikle Uyumlu Antrenman & Kamp Modu", "Tactic-Compatible Training & Camp Mode")));
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
            ui.add_space(4.0);
            ui.add(egui::Label::new(theme::dim(mode.description())).wrap());
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::ic(icon::SPARKLE).color(theme::INFO).size(18.0));
                ui.label(theme::label_strong(t!("Özel Kamp Modu:", "Special Camp Mode:")));
                let mut camp = self.settings.planner.camp_mode;
                let camp_opts: Vec<(planner::CampMode, &str)> = planner::CampMode::ALL.iter().map(|c| (*c, c.short_label())).collect();
                if theme::segmented(ui, &mut camp, &camp_opts) {
                    self.settings.planner.camp_mode = camp;
                    self.save_settings();
                    self.week_plan = None;
                    self.year.clear();
                    if can {
                        self.live.training(ctx, &self.eff, &self.settings.planner, self.settings.auto_training, false);
                    }
                }
            });
            ui.add_space(4.0);
            ui.add(egui::Label::new(theme::dim(self.settings.planner.camp_mode.description())).wrap());
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if theme::toggle(ui, &mut self.settings.planner.auto_smart_load).changed() {
                    self.save_settings();
                    if can {
                        self.live.training(ctx, &self.eff, &self.settings.planner, self.settings.auto_training, false);
                    }
                }
                ui.label(theme::label_strong(t!(
                    "Kadro Kondisyonuna Göre Akıllı Yük Ayarı",
                    "Smart Load Adjustment by Squad Condition"
                )));
                if let Some(avg_c) = self.live.squad_avg_condition() {
                    theme::pill(
                        ui,
                        &t!(
                            format!("Kadro Ort. Kondisyon: %{avg_c:.0}"),
                            format!("Squad Avg Condition: {avg_c:.0}%")
                        ),
                        theme::rgb(level_color(avg_c)),
                    );
                }
                ui.label(theme::faint(t!(
                    "(Kondisyon %82 altına düştüğünde otomatik Daha Fazla Dinlenme moduna geçer)",
                    "(Automatically switches to More Rest when squad condition drops below 82%)"
                )));
            });
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
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::faint(t!(
                "💡 İpucu: İki seansın yerini değiştirmek için önce birine sonra diğerine sol tıklayın (aynı hafta içinde). Seansı başka bir antrenmanla değiştirmek için üzerine sağ tıklayın.",
                "💡 Tip: Left-click two sessions in the same week to swap them. Right-click any session to replace it with another training session.",
                "💡 Dica: Clique com o botão esquerdo em duas sessões na mesma semana para trocá-las. Clique com o botão direito para escolher outra sessão."
            )));
            if let Some((sw_start, sw_d, sw_s)) = self.swap_training_slot {
                theme::pill(
                    ui,
                    &t!(
                        format!("Seçili: {} {} {}. öğün (iptal için tekrar tıklayın)", i18n::date_short(sw_start), i18n::weekday_short(sw_d), sw_s + 1),
                        format!("Selected: {} {} slot {} (click again to cancel)", i18n::date_short(sw_start), i18n::weekday_short(sw_d), sw_s + 1)
                    ),
                    theme::ACCENT_HI,
                );
            }
        });
        ui.add_space(6.0);
        let mut schedule_update: Option<(NaiveDate, Vec<[u16; 3]>, String)> = None;
        for w in &o.weeks {
            if let Some(upd) = week_card(ui, w, o.today, &self.live.fm_had, &mut self.swap_training_slot) {
                schedule_update = Some(upd);
            }
            ui.add_space(10.0);
        }
        if let Some((st, days, msg)) = schedule_update {
            self.live.write_week_schedule(ctx, st, days, msg);
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
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::ic(icon::SPARKLE).color(theme::INFO).size(18.0));
                ui.label(theme::label_strong(t!("Kamp Modu:", "Camp Mode:")));
                let mut camp = self.settings.planner.camp_mode;
                let camp_opts: Vec<(planner::CampMode, &str)> = planner::CampMode::ALL.iter().map(|c| (*c, c.short_label())).collect();
                if theme::segmented(ui, &mut camp, &camp_opts) {
                    self.settings.planner.camp_mode = camp;
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

    // -------------------------------------------------------------------- easy talk

    fn page_easy_talk(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        use crate::tactics::{self as tx, EasyTalkKind};
        let from_file = std::env::var_os("FM27_TACTICS_JSON");
        if let (Some(f), None) = (&from_file, &self.live.tactics) {
            self.live.tactics = std::fs::read_to_string(f).ok().and_then(|t| serde_json::from_str(&t).ok());
        }
        let connected = self.live.connected || (from_file.is_some() && self.live.tactics.is_some());
        let busy = self.live.busy() || self.live.tactics_busy();
        if connected && !self.tactics_asked && from_file.is_none() {
            self.tactics_asked = true;
            self.live.load_tactics(ctx);
        }
        let mut action: Option<(String, Value)> = None;

        // EasyTalk renkleri (FMSeasonHub EasyTrainingTalks teması)
        let green_praise = Color32::from_rgb(56, 178, 92);
        let red_crit = Color32::from_rgb(209, 69, 69);
        let pink_confirm = Color32::from_rgb(232, 30, 118);
        let gray_between = Color32::from_rgb(122, 130, 148);

        theme::page_header(
            ui,
            t!("EASY TALK · Kadro > Antrenman", "EASY TALK · Squad > Training"),
            t!(
                "Kulüp takımlarının (A Takım, U-20, U-18) antrenman puanı ve özellik gelişimine göre toplu övgü & eleştiri görüşmeleri.",
                "Batch training praise & criticism talks by training rating and attribute development across all club teams (First Team, U-20, U-18)."
            ),
            |ui| {
                if theme::button(ui, icon::ARROW_COUNTER_CLOCKWISE, t!("Geri Al", "Undo"), connected && !busy).clicked() {
                    action = Some((t!("geri al", "undo").into(), json!({"cmd": "undo"})));
                }
                if theme::button(
                    ui,
                    icon::GEAR_SIX,
                    if self.easy_talk_settings_open {
                        t!("Görüşmelere Dön", "Back to Talks")
                    } else {
                        t!("... Ayarlar", "... Settings")
                    },
                    true,
                )
                .clicked()
                {
                    self.easy_talk_settings_open = !self.easy_talk_settings_open;
                }
                if busy {
                    ui.spinner();
                }
            },
        );

        if let Some((l, r)) = action.take() {
            self.live.refresh(ctx, &l, Some(r));
        }
        let (m, e) = (self.live.status.clone(), self.live.status_error);
        self.status_line(ui, &m, e);

        if !connected {
            self.connect_card(ui, ctx);
            return;
        }

        // EasyTalk verisi yoksa aktif taktik kadrosundan yedek oluştur
        let et_data = match &self.live.easy_talks {
            Some(d) if !d.players.is_empty() => d.clone(),
            _ => {
                if let Some(td) = &self.live.tactics {
                    let cname = self.live.club_name().unwrap_or_else(|| "A Takım".into());
                    let players: Vec<tx::EasyTalkPlayer> = td
                        .players
                        .iter()
                        .map(|p| tx::EasyTalkPlayer {
                            uid: p.uid,
                            name: p.name.clone(),
                            team_index: 0,
                            team_name: cname.clone(),
                            positions: p.positions.clone(),
                            attributes: p.attributes.clone(),
                            raw_attributes: p.attributes.iter().map(|(k, v)| (k.clone(), v.saturating_mul(5))).collect(),
                            left_foot: p.left_foot,
                            right_foot: p.right_foot,
                            condition: p.condition,
                            sharpness: p.sharpness,
                            morale: p.morale,
                            injured: p.injured,
                            banned: p.banned,
                            loaned: false,
                        })
                        .collect();
                    let avail = players.iter().filter(|p| !p.injured && !p.loaned).count();
                    let inj = players.iter().filter(|p| p.injured).count();
                    tx::EasyTalkData {
                        club_name: cname.clone(),
                        teams: vec![tx::EasyTalkTeam {
                            index: 0,
                            kind: 0,
                            age_limit: 100,
                            name: cname,
                            total_players: players.len(),
                            available_players: avail,
                            injured_players: inj,
                            loaned_players: 0,
                        }],
                        players,
                    }
                } else {
                    theme::card(ui, |ui| {
                        ui.label(theme::faint(t!("Kadro okunuyor…", "Reading squad…")));
                    });
                    return;
                }
            }
        };

        // Seçili takımlara göre oyuncuları değerlendir
        let mut evals: Vec<tx::EasyTalkEvaluation> = Vec::new();
        for p in &et_data.players {
            if self.settings.easy_talk.disabled_teams.contains(&p.team_index) {
                continue;
            }
            let cut = self.settings.easy_talk.cutoffs_for(p.team_index);
            let base = self.live.easy_talks_baseline.get(&p.uid);
            let talked = self.live.easy_talks_talked.contains(&p.uid);
            let mut ev = tx::evaluate_easy_talk(p, cut, base, talked);
            if let Some(&ov) = self.live.easy_talks_override.get(&p.uid) {
                if ev.kind != EasyTalkKind::AlreadyTalked {
                    ev.kind = ov;
                }
            }
            evals.push(ev);
        }

        // ---- AYARLAR MODU (Görsel 1: TAKIMLAR, EŞİKLER KADROYU NASIL BÖLÜYOR, PUAN, GELİŞİM)
        if self.easy_talk_settings_open {
            self.render_easy_talk_settings(ui, &et_data, &evals, green_praise, red_crit, pink_confirm, gray_between);
            return;
        }

        // İstatistik sayıları
        let mut cnt_praise_dev = 0usize;
        let mut cnt_praise_rat = 0usize;
        let mut cnt_crit_dev = 0usize;
        let mut cnt_crit_rat = 0usize;
        let mut cnt_talked = 0usize;
        let mut cnt_no_talk = 0usize;
        let mut batch_praise_uids: Vec<u32> = Vec::new();
        let mut batch_crit_uids: Vec<u32> = Vec::new();

        for ev in &evals {
            let left_out = self.live.easy_talks_left_out.contains(&ev.uid);
            match ev.kind {
                EasyTalkKind::PraiseDevelopment => {
                    cnt_praise_dev += 1;
                    if !left_out {
                        batch_praise_uids.push(ev.uid);
                    }
                }
                EasyTalkKind::PraiseRating => {
                    cnt_praise_rat += 1;
                    if !left_out {
                        batch_praise_uids.push(ev.uid);
                    }
                }
                EasyTalkKind::CriticiseDevelopment => {
                    cnt_crit_dev += 1;
                    if !left_out {
                        batch_crit_uids.push(ev.uid);
                    }
                }
                EasyTalkKind::CriticiseRating => {
                    cnt_crit_rat += 1;
                    if !left_out {
                        batch_crit_uids.push(ev.uid);
                    }
                }
                EasyTalkKind::AlreadyTalked => cnt_talked += 1,
                EasyTalkKind::NoTalk => cnt_no_talk += 1,
            }
        }
        let actionable_total = batch_praise_uids.len() + batch_crit_uids.len();
        let total_players_shown = evals.len();

        // ---- ÜST BAR: "76 oyuncu · bu turda 23 görüşme" + Takım Filtreleri + Dağılım Çubuğu
        theme::card(ui, |ui| {
            ui.label(
                RichText::new(t!(
                    format!("{total_players_shown} oyuncu · bu turda {actionable_total} görüşme"),
                    format!("{total_players_shown} players · {actionable_total} talks in this batch")
                ))
                .size(14.0)
                .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);

            // Takım Seçim Hapları: [✓ Tümü 76] [✓ Como 32] [✓ U-20 Takımı 19] [✓ U-18 Takımı 25]
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::dim(t!("Takımlar", "Teams")));
                let all_enabled = self.settings.easy_talk.disabled_teams.is_empty();
                let all_avail: usize = et_data.teams.iter().map(|t| t.available_players).sum();
                let all_lbl = t!(format!("✓ Tümü {all_avail}"), format!("✓ All {all_avail}"));
                if theme::clickable_pill(
                    ui,
                    &all_lbl,
                    if all_enabled { theme::INFO } else { theme::TEXT_FAINT },
                    false,
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                {
                    if all_enabled {
                        self.settings.easy_talk.disabled_teams = et_data.teams.iter().map(|t| t.index).collect();
                    } else {
                        self.settings.easy_talk.disabled_teams.clear();
                    }
                    self.save_settings();
                }

                for tm in &et_data.teams {
                    let enabled = !self.settings.easy_talk.disabled_teams.contains(&tm.index);
                    let mark = if enabled { "✓ " } else { "" };
                    let lbl = format!("{mark}{} {}", tm.name, tm.available_players);
                    if theme::clickable_pill(ui, &lbl, if enabled { theme::INFO } else { theme::TEXT_FAINT }, false)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        if enabled {
                            self.settings.easy_talk.disabled_teams.push(tm.index);
                        } else {
                            self.settings.easy_talk.disabled_teams.retain(|&x| x != tm.index);
                        }
                        self.save_settings();
                    }
                }
            });
            ui.add_space(8.0);

            // Çok renkli yatay dağılım çubuğu
            let bar_w = ui.available_width().max(100.0);
            let (bar_rect, _) = ui.allocate_exact_size(vec2(bar_w, 7.0), egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(bar_rect, CornerRadius::same(3), theme::TRACK);
            let tot = total_players_shown.max(1) as f32;
            let segments = [
                (cnt_praise_dev, green_praise),
                (cnt_praise_rat, Color32::from_rgb(42, 152, 78)),
                (cnt_crit_dev + cnt_crit_rat, red_crit),
                (cnt_talked, Color32::from_rgb(76, 82, 96)),
                (cnt_no_talk, Color32::from_rgb(48, 52, 64)),
            ];
            let mut x = bar_rect.left();
            for (cnt, col) in segments {
                if cnt > 0 {
                    let sw = (bar_w * (cnt as f32 / tot)).max(3.0).min(bar_rect.right() - x);
                    if sw > 0.0 {
                        let r = egui::Rect::from_min_size(egui::pos2(x, bar_rect.top()), vec2(sw, bar_rect.height()));
                        painter.rect_filled(r, CornerRadius::same(2), col);
                        x += sw;
                    }
                }
            }
            ui.add_space(6.0);

            // Renkli açıklama satırı: ■ Gelişimi öv 7  ■ Puanı öv 8  ■ Gelişimi eleştir 8  ■ Zaten konuşuldu 48  ■ Görüşme yok 5
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let legend_item = |ui: &mut Ui, col: Color32, label: &str, count: usize| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        theme::status_dot(ui, col);
                        ui.label(RichText::new(label).size(12.5).color(theme::TEXT_DIM));
                        ui.label(RichText::new(count.to_string()).size(12.5).strong().color(theme::TEXT));
                    });
                };
                legend_item(ui, green_praise, t!("Gelişimi öv", "Praise development"), cnt_praise_dev);
                legend_item(ui, Color32::from_rgb(42, 152, 78), t!("Puanı öv", "Praise rating"), cnt_praise_rat);
                legend_item(ui, red_crit, t!("Gelişimi eleştir", "Criticise development"), cnt_crit_dev);
                if cnt_crit_rat > 0 {
                    legend_item(ui, red_crit, t!("Antrenmanı eleştir", "Criticise training"), cnt_crit_rat);
                }
                legend_item(ui, gray_between, t!("Zaten konuşuldu", "Already talked"), cnt_talked);
                legend_item(ui, theme::TEXT_FAINT, t!("Görüşme yok", "No talk"), cnt_no_talk);
            });
        });
        ui.add_space(12.0);

        // ---- ANA GÖRÜNÜM: Sol (2 Sütunlu Görüşme Tablosu) + Sağ (KADRO, GEÇERLİ EŞİKLER, ZATEN KONUŞULDU, GÖRÜŞME YOK)
        let mut actionable_rows: Vec<tx::EasyTalkEvaluation> = evals
            .iter()
            .filter(|e| e.kind.is_actionable())
            .cloned()
            .collect();
        actionable_rows.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| b.rating.total_cmp(&a.rating))
                .then_with(|| b.net_dev.cmp(&a.net_dev))
        });

        let talked_rows: Vec<&tx::EasyTalkEvaluation> = evals
            .iter()
            .filter(|e| e.kind == EasyTalkKind::AlreadyTalked)
            .collect();

        let total_avail = ui.available_width();
        let right_w = 248.0f32.min(total_avail * 0.27);
        let left_w = (total_avail - right_w - 14.0).max(400.0);

        ui.horizontal_top(|ui| {
            // SOL PANEL: 2 sütunlu oyuncu görüşme tablosu + alt onay barı
            ui.vertical(|ui| {
                ui.set_width(left_w);
                ui.set_max_width(left_w);

                theme::card(ui, |ui| {
                    if actionable_rows.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(28.0);
                            ui.label(theme::h2(t!(
                                "Geçerli eşikler ve takımlarla görüşülecek kimse yok.",
                                "Nobody to talk to with the current cutoffs and teams."
                            )));
                            ui.add_space(6.0);
                            ui.label(theme::dim(t!(
                                "Sağdaki 'Ayarlar' düğmesinden övgü/eleştiri eşiklerini değiştirebilir veya 'Turu Sıfırla' ile yeni görüşme turu başlatabilirsiniz.",
                                "Use the 'Settings' button on the right to adjust praise/criticism cutoffs, or 'Reset Batch' to start a new round."
                            )));
                            ui.add_space(28.0);
                        });
                    } else {
                        let mid = (actionable_rows.len() + 1) / 2;
                        let (col_left, col_right) = actionable_rows.split_at(mid);
                        ui.columns(2, |cols| {
                            self.render_easy_talk_table_col(&mut cols[0], ctx, "et_col_l", col_left, green_praise, red_crit);
                            self.render_easy_talk_table_col(&mut cols[1], ctx, "et_col_r", col_right, green_praise, red_crit);
                        });
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.vertical(|ui| {
                            let min_s = (actionable_total / 7).max(2);
                            let max_s = (actionable_total / 2).max(5);
                            ui.label(theme::faint(t!(
                                format!("{min_s} ile {max_s} sn sürer."),
                                format!("Takes {min_s} to {max_s} s.")
                            )));
                            ui.label(theme::faint(t!(
                                "Bir satırı bu turun dışında bırakmak için adına tıklayın · gelişimi görmek için üzerine gelin · değiştirmek için konuşmaya tıklayın",
                                "Click a name to leave it out of this batch · hover to see development · click the talk badge to switch it"
                            )));
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Pembe "23 görüşmeyi onayla >>" düğmesi
                            let btn_lbl = t!(
                                format!("{} görüşmeyi onayla  »", actionable_total),
                                format!("Confirm {} talks  »", actionable_total)
                            );
                            let confirm_btn = egui::Button::new(
                                RichText::new(btn_lbl)
                                    .font(egui::FontId::new(14.0, theme::bold()))
                                    .color(Color32::WHITE),
                            )
                            .fill(if actionable_total > 0 && !busy {
                                pink_confirm
                            } else {
                                theme::SURFACE2
                            })
                            .corner_radius(CornerRadius::same(18))
                            .min_size(vec2(195.0, 36.0));

                            if ui
                                .add_enabled(actionable_total > 0 && !busy, confirm_btn)
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                let mut summary = Vec::new();
                                for ev in &actionable_rows {
                                    if !self.live.easy_talks_left_out.contains(&ev.uid) {
                                        self.live.easy_talks_talked.insert(ev.uid);
                                        summary.push((
                                            ev.name.clone(),
                                            ev.team_name.clone(),
                                            ev.kind.label(),
                                            ev.reaction_label,
                                        ));
                                    }
                                }
                                self.live.easy_talks_last_summary = summary;
                                action = Some((
                                    t!("Easy Talk görüşmeleri", "Easy Talk conversations").into(),
                                    json!({
                                        "cmd": "apply_easy_talks",
                                        "praise_uids": batch_praise_uids,
                                        "criticise_uids": batch_crit_uids,
                                    }),
                                ));
                            }

                            if (!self.live.easy_talks_talked.is_empty()
                                || !self.live.easy_talks_left_out.is_empty()
                                || !self.live.easy_talks_override.is_empty())
                                && theme::button(ui, icon::ARROW_COUNTER_CLOCKWISE, t!("Turu Sıfırla", "Reset Batch"), !busy).clicked()
                            {
                                self.live.easy_talks_talked.clear();
                                self.live.easy_talks_left_out.clear();
                                self.live.easy_talks_override.clear();
                            }
                        });
                    });
                });
            });

            ui.add_space(12.0);

            // SAĞ PANEL: KADRO, GEÇERLİ EŞİKLER, ZATEN KONUŞULDU, GÖRÜŞME YOK
            ui.vertical(|ui| {
                ui.set_width(right_w);
                ui.set_max_width(right_w);

                // 1. KADRO Kartı
                theme::card(ui, |ui| {
                    ui.label(theme::caps(t!("KADRO", "SQUAD")));
                    ui.add_space(6.0);
                    for tm in &et_data.teams {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&tm.name).color(theme::TEXT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(t!(
                                        format!("{} oyuncudan {}", tm.total_players, tm.available_players),
                                        format!("{} of {} players", tm.available_players, tm.total_players)
                                    ))
                                    .strong()
                                    .color(theme::TEXT),
                                );
                            });
                        });
                        let mut sub_parts = Vec::new();
                        if tm.loaned_players > 0 {
                            sub_parts.push(t!(format!("Kiralık: {}", tm.loaned_players), format!("On loan: {}", tm.loaned_players)));
                        }
                        if tm.injured_players > 0 {
                            sub_parts.push(t!(format!("Sakat: {}", tm.injured_players), format!("Injured: {}", tm.injured_players)));
                        }
                        if !sub_parts.is_empty() {
                            ui.label(theme::faint(sub_parts.join(" · ")));
                        }
                        ui.add_space(4.0);
                    }
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(theme::faint(t!(
                            "Kiralık ve sakat oyuncular antrenman yapmaz: onlarla görüşme yok.",
                            "Players on loan or injured do not train: no talk for them."
                        )))
                        .wrap(),
                    );
                });
                ui.add_space(10.0);

                // 2. GEÇERLİ EŞİKLER Kartı
                theme::card(ui, |ui| {
                    ui.label(theme::caps(t!("GEÇERLİ EŞİKLER", "CURRENT CUTOFFS")));
                    ui.add_space(6.0);
                    let mc = &self.settings.easy_talk.main_cutoffs;
                    ui.horizontal(|ui| {
                        ui.label(theme::dim(t!("Puana göre öv", "Praise rating")));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if mc.rating_talks {
                                theme::pill(
                                    ui,
                                    &t!(format!("{:.2} ve üzeri", mc.praise_from), format!("from {:.2}", mc.praise_from)),
                                    green_praise,
                                );
                            } else {
                                theme::pill(ui, t!("kapalı", "off"), theme::TEXT_FAINT);
                            }
                        });
                    });
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.label(theme::dim(t!("Puana göre eleştir", "Criticise rating")));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if mc.rating_talks {
                                theme::pill(
                                    ui,
                                    &t!(format!("{:.2} ve altı", mc.criticise_up_to), format!("up to {:.2}", mc.criticise_up_to)),
                                    red_crit,
                                );
                            } else {
                                theme::pill(ui, t!("kapalı", "off"), theme::TEXT_FAINT);
                            }
                        });
                    });
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.label(theme::dim(t!("Gelişimi öv", "Praise development")));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if mc.development_talks && mc.rises_for_praise > 0 {
                                theme::pill(
                                    ui,
                                    &t!(format!("{}+ yükselen", mc.rises_for_praise), format!("{}+ rising", mc.rises_for_praise)),
                                    green_praise,
                                );
                            } else {
                                theme::pill(ui, t!("kapalı", "off"), theme::TEXT_FAINT);
                            }
                        });
                    });
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.label(theme::dim(t!("Gelişimi eleştir", "Criticise development")));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if mc.development_talks && mc.falls_for_criticism > 0 {
                                theme::pill(
                                    ui,
                                    &t!(format!("{}+ düşen", mc.falls_for_criticism), format!("{}+ falling", mc.falls_for_criticism)),
                                    red_crit,
                                );
                            } else {
                                theme::pill(ui, t!("kapalı", "off"), theme::TEXT_FAINT);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    if ui
                        .add_sized(
                            vec2(ui.available_width(), 30.0),
                            egui::Button::new(RichText::new(t!("... Ayarlar", "... Settings")).color(theme::TEXT))
                                .fill(theme::SURFACE2)
                                .corner_radius(CornerRadius::same(8)),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.easy_talk_settings_open = true;
                    }
                });
                ui.add_space(10.0);

                // 3. ZATEN KONUŞULDU Kartı
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(theme::caps(t!("ZATEN KONUŞULDU", "ALREADY TALKED")));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(talked_rows.len().to_string()).strong().color(theme::TEXT));
                        });
                    });
                    ui.add_space(6.0);
                    if talked_rows.is_empty() {
                        ui.label(theme::faint(t!("Henüz kimseyle konuşulmadı.", "Nobody talked to yet.")));
                    } else {
                        for tr in talked_rows.iter().take(4) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                self.draw_player_avatar(ui, ctx, tr.uid, 20.0);
                                ui.add(egui::Label::new(RichText::new(&tr.name).size(12.0).color(theme::TEXT)).truncate());
                                ui.add(egui::Label::new(theme::faint(&tr.pos_short)).truncate());
                            });
                            ui.add_space(2.0);
                        }
                        if talked_rows.len() > 4 {
                            ui.label(theme::faint(t!(
                                format!("ve {} kişi daha", talked_rows.len() - 4),
                                format!("and {} more", talked_rows.len() - 4)
                            )));
                        }
                    }
                    if !self.live.easy_talks_last_summary.is_empty() {
                        ui.add_space(6.0);
                        if ui
                            .add_sized(
                                vec2(ui.available_width(), 28.0),
                                egui::Button::new(
                                    RichText::new(if self.easy_talk_show_summary {
                                        t!("Özeti gizle", "Hide summary")
                                    } else {
                                        t!("Son özeti gör", "See the last summary")
                                    })
                                    .color(theme::TEXT),
                                )
                                .fill(theme::SURFACE2)
                                .corner_radius(CornerRadius::same(8)),
                            )
                            .clicked()
                        {
                            self.easy_talk_show_summary = !self.easy_talk_show_summary;
                        }
                        if self.easy_talk_show_summary {
                            ui.add_space(6.0);
                            for (pname, tname, klabel, rlabel) in self.live.easy_talks_last_summary.iter().take(12) {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(pname).size(12.0).strong().color(theme::TEXT));
                                    ui.label(theme::faint(format!("({tname}) · {klabel} →")));
                                    ui.label(RichText::new(*rlabel).size(12.0).color(green_praise));
                                });
                            }
                        }
                    }
                });
                ui.add_space(10.0);

                // 4. GÖRÜŞME YOK Kartı
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(theme::caps(t!("GÖRÜŞME YOK", "NO TALK")));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(cnt_no_talk.to_string()).strong().color(theme::TEXT));
                        });
                    });
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(theme::faint(t!(
                            "Mevcut ayarlarla uygun görüşme yok (eşikler arasında kalan veya sakat/kiralık oyuncular).",
                            "No talk applies with the current settings (players between cutoffs or injured/on loan)."
                        )))
                        .wrap(),
                    );
                });
            });
        });

        if let Some((l, r)) = action {
            self.live.refresh(ctx, &l, Some(r));
        }
    }

    fn ensure_player_avatar_loaded(&mut self, ctx: &egui::Context, uid: u32) -> Option<egui::TextureHandle> {
        if !self.face_textures.contains_key(&uid) {
            let loaded = (|| -> Option<egui::TextureHandle> {
                let g_trim = self.settings.graphics_dir.trim();
                let f_trim = self.settings.faces_dir.trim();
                let mut search_dirs = Vec::new();
                if !g_trim.is_empty() {
                    let g_base = PathBuf::from(g_trim);
                    search_dirs.push(g_base.join("faces"));
                    search_dirs.push(g_base.join("Faces"));
                    search_dirs.push(g_base.join("players"));
                    search_dirs.push(g_base);
                }
                if !f_trim.is_empty() {
                    let f_dir = PathBuf::from(f_trim);
                    if !search_dirs.contains(&f_dir) {
                        search_dirs.push(f_dir);
                    }
                }
                let mut found_path = None;
                for dir in search_dirs {
                    if !dir.is_dir() {
                        continue;
                    }
                    let candidates = [
                        dir.join(format!("face_{uid}.png")),
                        dir.join(format!("{uid}.png")),
                        dir.join(format!("face_{uid} (1).png")),
                        dir.join(format!("{uid} (1).png")),
                    ];
                    if let Some(p) = candidates.into_iter().find(|p| p.is_file()) {
                        found_path = Some(p);
                        break;
                    }
                }
                let path = found_path?;
                let img = image::open(&path).ok()?.into_rgba8();
                let (w, h) = img.dimensions();
                let color_img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
                Some(ctx.load_texture(format!("face_{uid}"), color_img, egui::TextureOptions::LINEAR))
            })();
            self.face_textures.insert(uid, loaded);
        }
        self.face_textures.get(&uid).and_then(|o| o.clone())
    }

    fn draw_player_avatar(&mut self, ui: &mut Ui, ctx: &egui::Context, uid: u32, size: f32) {
        let tex_opt = self.ensure_player_avatar_loaded(ctx, uid);
        let (rect, _) = ui.allocate_exact_size(vec2(size, size), egui::Sense::hover());
        if let Some(tex) = tex_opt {
            ui.painter().rect_filled(rect, CornerRadius::same(5), Color32::from_rgb(20, 23, 30));
            egui::Image::from_texture(&tex)
                .corner_radius(CornerRadius::same(5))
                .paint_at(ui, rect);
        } else {
            theme::paint_silhouette_avatar(ui.painter(), rect);
        }
    }

    fn draw_tactics_pitch(
        &self,
        ui: &mut Ui,
        places: &[crate::tactics::Place],
        coords: &[(f32, f32)],
        by_uid: &std::collections::HashMap<u32, &crate::tactics::Player>,
        pitch_w: f32,
    ) {
        let pitch_w = pitch_w.max(250.0);
        let pitch_h = 465.0f32;
        let (rect, _) = ui.allocate_exact_size(vec2(pitch_w, pitch_h), egui::Sense::hover());
        let painter = ui.painter();

        // Çim saha arka planı + yatay çim şeritleri
        painter.rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(23, 56, 36));
        let stripes = 8;
        let sh = rect.height() / stripes as f32;
        for s in 0..stripes {
            if s % 2 == 1 {
                let sr = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.top() + s as f32 * sh), vec2(rect.width(), sh));
                painter.rect_filled(sr, CornerRadius::same(0), Color32::from_rgba_unmultiplied(255, 255, 255, 7));
            }
        }

        // Saha çizgileri
        let line_col = Color32::from_rgba_unmultiplied(210, 235, 215, 75);
        let line_stroke = Stroke::new(1.3_f32, line_col);
        let inner = rect.shrink(10.0);
        painter.rect_stroke(inner, CornerRadius::same(4), line_stroke, egui::StrokeKind::Inside);
        // Orta çizgi ve orta yuvarlak
        let mid_y = inner.center().y;
        painter.hline(inner.x_range(), mid_y, line_stroke);
        painter.circle_stroke(inner.center(), 36.0, line_stroke);
        // Üst ve alt ceza sahaları
        let box_w = inner.width() * 0.56;
        let box_h = 56.0;
        let top_box = egui::Rect::from_min_size(egui::pos2(inner.center().x - box_w * 0.5, inner.top()), vec2(box_w, box_h));
        let bot_box = egui::Rect::from_min_size(egui::pos2(inner.center().x - box_w * 0.5, inner.bottom() - box_h), vec2(box_w, box_h));
        painter.rect_stroke(top_box, CornerRadius::same(0), line_stroke, egui::StrokeKind::Inside);
        painter.rect_stroke(bot_box, CornerRadius::same(0), line_stroke, egui::StrokeKind::Inside);

        // 11 Oyuncu Kartı
        for (i, pl) in places.iter().enumerate() {
            let (nx, ny) = coords.get(i).copied().unwrap_or((0.5, 0.5));
            let cx = inner.left() + nx * inner.width();
            let cy = inner.top() + ny * inner.height();
            let center = egui::pos2(cx, cy);

            let (ic, _) = crate::tactics::ip_label(pl.ip);
            let (oc, _) = crate::tactics::oop_label(pl.oop);
            let player_opt = pl.uid.and_then(|u| by_uid.get(&u)).copied();

            // Avatar kutusu (32x32)
            let av_size = 32.0f32;
            let av_rect = egui::Rect::from_center_size(egui::pos2(center.x, center.y - 10.0), vec2(av_size, av_size));
            painter.rect_filled(av_rect.expand(1.5), CornerRadius::same(7), Color32::from_rgb(14, 18, 24));
            if let Some(p) = player_opt {
                if let Some(Some(tex)) = self.face_textures.get(&p.uid) {
                    egui::Image::from_texture(tex)
                        .corner_radius(CornerRadius::same(6))
                        .paint_at(ui, av_rect);
                } else {
                    theme::paint_silhouette_avatar(painter, av_rect);
                }
                let f = crate::tactics::fit(p, pl);
                let fit_col = theme::rgb(level_color(f));
                painter.rect_stroke(av_rect.expand(1.5), CornerRadius::same(7), Stroke::new(1.5_f32, fit_col), egui::StrokeKind::Inside);

                // Uyum puanı rozeti (sağ üst köşe)
                let badge_rect = egui::Rect::from_center_size(egui::pos2(av_rect.right() + 3.0, av_rect.top() + 3.0), vec2(21.0, 14.0));
                painter.rect_filled(badge_rect, CornerRadius::same(4), fit_col);
                painter.text(
                    badge_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{f:.0}"),
                    egui::FontId::new(9.5, theme::bold()),
                    Color32::from_rgb(12, 16, 20),
                );

                // Rol + İsim + Kondisyon etiketi (alt kısım)
                let short_name = p
                    .name
                    .split_whitespace()
                    .last()
                    .unwrap_or(p.name.as_str());
                let lbl_w = 72.0f32;
                let lbl_rect = egui::Rect::from_center_size(egui::pos2(center.x, av_rect.bottom() + 13.0), vec2(lbl_w, 22.0));
                painter.rect_filled(lbl_rect, CornerRadius::same(5), Color32::from_rgba_unmultiplied(13, 16, 23, 235));
                painter.rect_stroke(lbl_rect, CornerRadius::same(5), Stroke::new(1.0_f32, Color32::from_rgb(42, 48, 62)), egui::StrokeKind::Inside);

                let role_str = format!("{ic}/{oc}");
                painter.text(
                    egui::pos2(lbl_rect.center().x, lbl_rect.top() + 5.5),
                    egui::Align2::CENTER_CENTER,
                    role_str,
                    egui::FontId::new(8.5, theme::bold()),
                    theme::ACCENT_HI,
                );
                let disp_name: String = if short_name.chars().count() > 10 {
                    format!("{}…", short_name.chars().take(9).collect::<String>())
                } else {
                    short_name.to_string()
                };
                painter.text(
                    egui::pos2(lbl_rect.center().x, lbl_rect.bottom() - 6.0),
                    egui::Align2::CENTER_CENTER,
                    disp_name,
                    egui::FontId::new(10.0, theme::bold()),
                    Color32::WHITE,
                );

                // İnce kondisyon çubuğu (etiketin hemen altında)
                let cond_pct = (p.condition as f32 / 100.0).clamp(0.0, 1.0);
                let bar_bg = egui::Rect::from_min_size(egui::pos2(lbl_rect.left() + 4.0, lbl_rect.bottom() + 1.5), vec2(lbl_w - 8.0, 2.5));
                painter.rect_filled(bar_bg, CornerRadius::same(1), Color32::from_rgb(30, 35, 45));
                let bar_fg = egui::Rect::from_min_size(bar_bg.min, vec2(bar_bg.width() * cond_pct, bar_bg.height()));
                painter.rect_filled(bar_fg, CornerRadius::same(1), theme::rgb(level_color(p.condition as f32)));
            } else {
                theme::paint_silhouette_avatar(painter, av_rect);
                let lbl_rect = egui::Rect::from_center_size(egui::pos2(center.x, av_rect.bottom() + 11.0), vec2(60.0, 16.0));
                painter.rect_filled(lbl_rect, CornerRadius::same(4), Color32::from_rgba_unmultiplied(13, 16, 23, 210));
                painter.text(
                    lbl_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{ic}/{oc}"),
                    egui::FontId::new(9.0, theme::bold()),
                    theme::TEXT_FAINT,
                );
            }
        }
    }

    fn render_easy_talk_table_col(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        grid_id: &str,
        rows: &[crate::tactics::EasyTalkEvaluation],
        green_praise: Color32,
        red_crit: Color32,
    ) {
        let col_w = ui.available_width();
        let name_col_w = (col_w - 235.0).max(125.0);
        let team_col_w = 56.0f32;

        egui::Grid::new(grid_id)
            .num_columns(5)
            .spacing([8.0, 6.0])
            .min_col_width(24.0)
            .striped(true)
            .show(ui, |ui| {
                ui.label(theme::caps(t!("AD", "NAME")));
                ui.label(theme::caps(t!("TAKIM", "TEAM")));
                ui.label(theme::caps(t!("GÖRÜŞME", "TALK")));
                ui.label(theme::caps(t!("GEL.", "DEV.")));
                ui.label(theme::caps(t!("PUAN", "RATING")));
                ui.end_row();

                for ev in rows {
                    let left_out = self.live.easy_talks_left_out.contains(&ev.uid);
                    let name_col = if left_out { theme::TEXT_FAINT } else { theme::TEXT };

                    let hover_txt = {
                        let mut lines = Vec::new();
                        if !ev.rising_names.is_empty() {
                            lines.push(format!(
                                "{} ({}): {}",
                                t!("Yükselen", "Rising"),
                                ev.rising,
                                ev.rising_names.join(", ")
                            ));
                        }
                        if !ev.falling_names.is_empty() {
                            lines.push(format!(
                                "{} ({}): {}",
                                t!("Düşen", "Falling"),
                                ev.falling,
                                ev.falling_names.join(", ")
                            ));
                        }
                        if lines.is_empty() {
                            t!("Antrenman raporunda belirgin özellik değişimi yok.", "No attribute changes in training report.").to_string()
                        } else {
                            lines.join("\n")
                        }
                    };

                    // 1. AD (Yüz + Tek Satır İsim + Tek Satır Mevki)
                    ui.horizontal(|ui| {
                        ui.set_min_width(name_col_w);
                        ui.set_max_width(name_col_w);
                        ui.spacing_mut().item_spacing.x = 7.0;
                        self.draw_player_avatar(ui, ctx, ev.uid, 30.0);
                        let text_w = (name_col_w - 38.0).max(80.0);
                        ui.vertical(|ui| {
                            ui.set_min_width(text_w);
                            ui.set_max_width(text_w);
                            ui.spacing_mut().item_spacing.y = 1.0;
                            let mut name_rt = RichText::new(&ev.name).size(13.0).strong().color(name_col);
                            if left_out {
                                name_rt = name_rt.strikethrough();
                            }
                            if ui
                                .add(egui::Label::new(name_rt).truncate().sense(egui::Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(&hover_txt)
                                .clicked()
                            {
                                if left_out {
                                    self.live.easy_talks_left_out.remove(&ev.uid);
                                } else {
                                    self.live.easy_talks_left_out.insert(ev.uid);
                                }
                            }
                            ui.add(
                                egui::Label::new(RichText::new(&ev.pos_short).size(10.5).color(theme::TEXT_FAINT))
                                    .truncate(),
                            );
                        });
                    });

                    // 2. TAKIM (Tek satır, taşmadan)
                    ui.scope(|ui| {
                        ui.set_min_width(team_col_w);
                        ui.set_max_width(team_col_w);
                        ui.add(
                            egui::Label::new(RichText::new(&ev.team_name).size(12.0).color(theme::TEXT_DIM))
                                .truncate(),
                        )
                        .on_hover_text(&ev.team_name);
                    });

                    // 3. GÖRÜŞME (Sabit 22px yükseklikli dolgu hapı)
                    let badge_col = if left_out {
                        theme::TEXT_FAINT
                    } else if ev.kind.is_praise() {
                        green_praise
                    } else {
                        red_crit
                    };
                    let badge_lbl = if left_out {
                        t!("dışarıda", "left out")
                    } else {
                        ev.kind.label()
                    };
                    if theme::clickable_pill(ui, badge_lbl, badge_col, !left_out)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(&hover_txt)
                        .clicked()
                    {
                        if left_out {
                            self.live.easy_talks_left_out.remove(&ev.uid);
                        } else {
                            self.live.easy_talks_override.insert(ev.uid, ev.kind.toggle_action());
                        }
                    }

                    // 4. GEL. (+22 / -13)
                    if ev.net_dev > 0 {
                        ui.label(RichText::new(format!("+{}", ev.net_dev)).size(12.5).strong().color(green_praise))
                            .on_hover_text(&hover_txt);
                    } else if ev.net_dev < 0 {
                        ui.label(RichText::new(format!("{}", ev.net_dev)).size(12.5).strong().color(red_crit))
                            .on_hover_text(&hover_txt);
                    } else {
                        ui.label(theme::faint(""));
                    }

                    // 5. PUAN (Sabit 22px yükseklikli dikdörtgen rozet)
                    let r_col = if ev.rating >= 7.50 {
                        green_praise
                    } else if ev.rating <= 6.50 {
                        red_crit
                    } else {
                        Color32::from_rgb(88, 96, 114)
                    };
                    let r_str = if i18n::is_tr() {
                        format!("{:.2}", ev.rating).replace('.', ",")
                    } else {
                        format!("{:.2}", ev.rating)
                    };
                    theme::rating_badge(ui, &r_str, r_col);
                    ui.end_row();
                }
            });
    }

    fn render_easy_talk_settings(
        &mut self,
        ui: &mut Ui,
        et_data: &crate::tactics::EasyTalkData,
        evals: &[crate::tactics::EasyTalkEvaluation],
        green_praise: Color32,
        red_crit: Color32,
        pink_confirm: Color32,
        gray_between: Color32,
    ) {
        let mut dirty = false;

        theme::card(ui, |ui| {
            ui.label(theme::caps(t!("TAKIMLAR", "TEAMS")));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let all_enabled = self.settings.easy_talk.disabled_teams.is_empty();
                let all_avail: usize = et_data.teams.iter().map(|t| t.available_players).sum();
                let all_lbl = t!(format!("✓ Tümü {all_avail}"), format!("✓ All {all_avail}"));
                if theme::clickable_pill(ui, &all_lbl, if all_enabled { theme::INFO } else { theme::TEXT_FAINT }, false)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    if all_enabled {
                        self.settings.easy_talk.disabled_teams = et_data.teams.iter().map(|t| t.index).collect();
                    } else {
                        self.settings.easy_talk.disabled_teams.clear();
                    }
                    dirty = true;
                }
                for tm in &et_data.teams {
                    let enabled = !self.settings.easy_talk.disabled_teams.contains(&tm.index);
                    let mark = if enabled { "✓ " } else { "" };
                    let lbl = format!("{mark}{} {}", tm.name, tm.available_players);
                    if theme::clickable_pill(ui, &lbl, if enabled { theme::INFO } else { theme::TEXT_FAINT }, false)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        if enabled {
                            self.settings.easy_talk.disabled_teams.push(tm.index);
                        } else {
                            self.settings.easy_talk.disabled_teams.retain(|&x| x != tm.index);
                        }
                        dirty = true;
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(theme::faint(t!(
                "Görüşmelere dahil olan kulüp takımları.",
                "The club teams the talks go to."
            )));
        });
        ui.add_space(12.0);

        if !et_data.teams.iter().any(|t| t.index == self.easy_talk_cutoff_team) {
            self.easy_talk_cutoff_team = et_data.teams.first().map(|t| t.index).unwrap_or(0);
        }
        let cur_team_idx = self.easy_talk_cutoff_team;
        let is_first_team = cur_team_idx == et_data.teams.first().map(|t| t.index).unwrap_or(0);

        let active_cut = self.settings.easy_talk.cutoffs_for(cur_team_idx).clone();
        let team_evals: Vec<&crate::tactics::EasyTalkEvaluation> = evals
            .iter()
            .filter(|e| e.team_index == cur_team_idx)
            .collect();
        let talks_with_cutoffs = team_evals.iter().filter(|e| e.kind.is_actionable()).count();
        let in_crit_band = team_evals.iter().filter(|e| e.rating <= active_cut.criticise_up_to).count();
        let in_praise_band = team_evals.iter().filter(|e| e.rating >= active_cut.praise_from).count();
        let in_between = team_evals.len().saturating_sub(in_crit_band + in_praise_band);

        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::caps(t!("EŞİKLER KADROYU NASIL BÖLÜYOR", "HOW THE CUTOFFS SPLIT THE SQUAD")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(theme::dim(t!(
                        format!("Bu eşiklerle: {talks_with_cutoffs} görüşme"),
                        format!("With these cutoffs: {talks_with_cutoffs} talks")
                    )));
                });
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::dim(t!("Eşikler:", "Cutoffs:")));
                for tm in &et_data.teams {
                    let sel = tm.index == cur_team_idx;
                    if theme::clickable_pill(ui, &tm.name, if sel { theme::INFO } else { theme::TEXT_FAINT }, sel)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.easy_talk_cutoff_team = tm.index;
                    }
                }
                if !is_first_team {
                    let has_custom = self.settings.easy_talk.by_team.contains_key(&cur_team_idx);
                    if has_custom {
                        if theme::small_button(ui, t!("A takımınkini kullan", "Use the first team's"), true).clicked() {
                            self.settings.easy_talk.by_team.remove(&cur_team_idx);
                            dirty = true;
                        }
                    } else {
                        ui.label(theme::faint(t!("(A takımınkini kullanır)", "(uses the first team's)")));
                    }
                }
            });
            ui.add_space(12.0);

            let plot_w = ui.available_width().max(200.0);
            let plot_h = 64.0;
            let (rect, _) = ui.allocate_exact_size(vec2(plot_w, plot_h), egui::Sense::hover());
            let painter = ui.painter();
            let min_r = 5.50f32;
            let max_r = 9.50f32;
            let x_of = |r: f32| -> f32 {
                let t = ((r - min_r) / (max_r - min_r)).clamp(0.0, 1.0);
                rect.left() + t * rect.width()
            };
            let bar_y = rect.bottom() - 18.0;
            let x_crit = x_of(active_cut.criticise_up_to);
            let x_praise = x_of(active_cut.praise_from);

            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(rect.left(), bar_y), egui::pos2(x_crit, bar_y + 6.0)),
                CornerRadius::same(2),
                red_crit,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(x_crit, bar_y), egui::pos2(x_praise, bar_y + 6.0)),
                CornerRadius::same(0),
                Color32::from_rgb(76, 82, 96),
            );
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(x_praise, bar_y), egui::pos2(rect.right(), bar_y + 6.0)),
                CornerRadius::same(2),
                green_praise,
            );

            painter.vline(x_crit, (rect.top() + 4.0)..=bar_y, Stroke::new(1.0_f32, red_crit));
            painter.vline(x_praise, (rect.top() + 4.0)..=bar_y, Stroke::new(1.0_f32, green_praise));

            let mut buckets: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();
            for ev in &team_evals {
                let b = ((ev.rating.clamp(min_r, max_r) - min_r) / 0.10).round() as i32;
                let stack = buckets.entry(b).or_insert(0);
                let px = x_of(min_r + b as f32 * 0.10);
                let py = bar_y - 7.0 - (*stack as f32 * 8.0);
                if py >= rect.top() + 3.0 {
                    let dot_col = if ev.rating <= active_cut.criticise_up_to {
                        red_crit
                    } else if ev.rating >= active_cut.praise_from {
                        green_praise
                    } else {
                        gray_between
                    };
                    painter.circle_filled(egui::pos2(px, py), 3.2, dot_col);
                }
                *stack += 1;
            }

            let fmt_r = |v: f32| if i18n::is_tr() { format!("{v:.2}").replace('.', ",") } else { format!("{v:.2}") };
            let font_sm = egui::FontId::new(11.5, theme::bold());
            painter.text(egui::pos2(rect.left(), bar_y + 8.0), egui::Align2::LEFT_TOP, fmt_r(min_r), font_sm.clone(), theme::TEXT_FAINT);
            painter.text(egui::pos2(x_crit, bar_y + 8.0), egui::Align2::CENTER_TOP, fmt_r(active_cut.criticise_up_to), font_sm.clone(), red_crit);
            painter.text(egui::pos2(x_praise, bar_y + 8.0), egui::Align2::CENTER_TOP, fmt_r(active_cut.praise_from), font_sm.clone(), green_praise);
            painter.text(egui::pos2(rect.right(), bar_y + 8.0), egui::Align2::RIGHT_TOP, fmt_r(max_r), font_sm, theme::TEXT_FAINT);

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 24.0;
                for (col, cnt, lbl) in [
                    (red_crit, in_crit_band, t!("Eleştiri bandında", "In the criticism band")),
                    (gray_between, in_between, t!("Arada", "In between")),
                    (green_praise, in_praise_band, t!("Övgü bandında", "In the praise band")),
                ] {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            theme::status_dot(ui, col);
                            ui.label(RichText::new(cnt.to_string()).size(16.0).strong().color(theme::TEXT));
                        });
                        ui.label(theme::faint(lbl));
                    });
                }
            });
        });
        ui.add_space(12.0);

        let mut edit_cut = active_cut.clone();
        let mut cut_changed = false;

        ui.columns(2, |cols| {
            theme::card(&mut cols[0], |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::caps(t!("PUAN", "RATING")));
                    ui.add_space(6.0);
                    if theme::toggle(ui, &mut edit_cut.rating_talks).changed() {
                        cut_changed = true;
                    }
                });
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    theme::pill(ui, "↑", green_praise);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(t!("Övgü eşiği", "Praise from")).strong().color(theme::TEXT));
                        ui.label(theme::faint(t!("ortalama antrenman puanı", "average training rating")));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::small_button(ui, "+", edit_cut.rating_talks && edit_cut.praise_from < 9.50).clicked() {
                            edit_cut.praise_from = ((edit_cut.praise_from + 0.05) * 20.0).round() / 20.0;
                            cut_changed = true;
                        }
                        let s = if i18n::is_tr() {
                            format!("{:.2}", edit_cut.praise_from).replace('.', ",")
                        } else {
                            format!("{:.2}", edit_cut.praise_from)
                        };
                        ui.label(RichText::new(s).size(20.0).strong().color(theme::TEXT));
                        if theme::small_button(ui, "-", edit_cut.rating_talks && edit_cut.praise_from > edit_cut.criticise_up_to + 0.10).clicked() {
                            edit_cut.praise_from = ((edit_cut.praise_from - 0.05) * 20.0).round() / 20.0;
                            cut_changed = true;
                        }
                    });
                });
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    theme::pill(ui, "↓", red_crit);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(t!("Eleştiri eşiği", "Criticise up to")).strong().color(theme::TEXT));
                        ui.label(theme::faint(t!("ortalama antrenman puanı", "average training rating")));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::small_button(ui, "+", edit_cut.rating_talks && edit_cut.criticise_up_to + 0.10 < edit_cut.praise_from).clicked() {
                            edit_cut.criticise_up_to = ((edit_cut.criticise_up_to + 0.05) * 20.0).round() / 20.0;
                            cut_changed = true;
                        }
                        let s = if i18n::is_tr() {
                            format!("{:.2}", edit_cut.criticise_up_to).replace('.', ",")
                        } else {
                            format!("{:.2}", edit_cut.criticise_up_to)
                        };
                        ui.label(RichText::new(s).size(20.0).strong().color(theme::TEXT));
                        if theme::small_button(ui, "-", edit_cut.rating_talks && edit_cut.criticise_up_to > 5.50).clicked() {
                            edit_cut.criticise_up_to = ((edit_cut.criticise_up_to - 0.05) * 20.0).round() / 20.0;
                            cut_changed = true;
                        }
                    });
                });
            });

            theme::card(&mut cols[1], |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::caps(t!("GELİŞİM", "DEVELOPMENT")));
                    ui.add_space(6.0);
                    if theme::toggle(ui, &mut edit_cut.development_talks).changed() {
                        cut_changed = true;
                    }
                });
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    theme::pill(ui, "↑", green_praise);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(t!("Övmek için en az", "Praise with at least")).strong().color(theme::TEXT));
                        ui.label(theme::faint(t!("yükselen özellik", "rising attributes")));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::small_button(ui, "+", edit_cut.development_talks && edit_cut.rises_for_praise < 20).clicked() {
                            edit_cut.rises_for_praise += 1;
                            cut_changed = true;
                        }
                        ui.label(RichText::new(edit_cut.rises_for_praise.to_string()).size(20.0).strong().color(theme::TEXT));
                        if theme::small_button(ui, "-", edit_cut.development_talks && edit_cut.rises_for_praise > 0).clicked() {
                            edit_cut.rises_for_praise -= 1;
                            cut_changed = true;
                        }
                    });
                });
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    theme::pill(ui, "↓", red_crit);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(t!("Eleştirmek için en az", "Criticise with at least")).strong().color(theme::TEXT));
                        ui.label(theme::faint(t!("düşen özellik", "falling attributes")));
                        ui.label(theme::faint(t!("0 = bu görüşme kapalı", "0 = this talk is off")));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::small_button(ui, "+", edit_cut.development_talks && edit_cut.falls_for_criticism < 20).clicked() {
                            edit_cut.falls_for_criticism += 1;
                            cut_changed = true;
                        }
                        ui.label(RichText::new(edit_cut.falls_for_criticism.to_string()).size(20.0).strong().color(theme::TEXT));
                        if theme::small_button(ui, "-", edit_cut.development_talks && edit_cut.falls_for_criticism > 0).clicked() {
                            edit_cut.falls_for_criticism -= 1;
                            cut_changed = true;
                        }
                    });
                });
            });
        });

        if cut_changed {
            if is_first_team {
                self.settings.easy_talk.main_cutoffs = edit_cut;
            } else {
                self.settings.easy_talk.by_team.insert(cur_team_idx, edit_cut);
            }
            dirty = true;
        }
        if dirty {
            self.save_settings();
        }

        ui.add_space(10.0);
        ui.add(
            egui::Label::new(theme::faint(t!(
                "Gelişim, yükselen özellikler düşenlerden fazlaysa sayılır (eleştiride tersi). Puan ve gelişim ikisi de sayılırsa, kendi eşiğinden daha çok uzaklaşan kazanır. Eşikler arasında ya da puan konuşması kapalıyken gelişim karar verir: övgü veya eleştiri.",
                "Development counts when more attributes rise than fall (the reverse for criticism). If both rating and development apply, whichever is further past its cutoff wins. Between the cutoffs or when rating talks are off, development decides: praise or criticism."
            )))
            .wrap(),
        );
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let save_btn = egui::Button::new(
                RichText::new(t!("Kaydet  »", "Save  »"))
                    .font(egui::FontId::new(14.0, theme::bold()))
                    .color(Color32::WHITE),
            )
            .fill(pink_confirm)
            .corner_radius(CornerRadius::same(18))
            .min_size(vec2(130.0, 34.0));
            if ui.add(save_btn).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.save_settings();
                self.easy_talk_settings_open = false;
            }
            if theme::button(ui, "", t!("İptal", "Cancel"), true).clicked() {
                self.easy_talk_settings_open = false;
            }
        });
    }

    // -------------------------------------------------------------------- selectie

    fn page_squad(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let busy = self.live.busy();
        let connected = self.live.connected;
        let injured = self.live.squad.iter().filter(|p| p.get("injuries").and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty())).count();
        let banned = self.live.squad.iter().filter(|p| p.get("bans").and_then(|v| v.as_u64()).unwrap_or(0) > 0).count();
        let mut action: Option<(String, Value)> = None;
        let n = self.live.squad.len();
        let subtitle = t!(
            format!("{n} oyuncu · {injured} sakat · {banned} cezalı"),
            format!("{n} players · {injured} injured · {banned} suspended")
        );
        theme::page_header(ui, t!("Kadro", "Squad"), &subtitle, |ui| {
            if theme::button(ui, icon::ARROW_COUNTER_CLOCKWISE, t!("Geri Al", "Undo"), connected && !busy).clicked() {
                action = Some((t!("geri al", "undo").into(), json!({"cmd": "undo"})));
            }
            if busy {
                ui.spinner();
            }
        });
        let (m, e) = (self.live.status.clone(), self.live.status_error);
        self.status_line(ui, &m, e);
        if !connected {
            self.connect_card(ui, ctx);
            return;
        }
        theme::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let heal_lbl = if injured > 0 {
                    t!(format!("Herkesi İyileştir ({injured})"), format!("Heal Everyone ({injured})"))
                } else {
                    t!("Herkesi İyileştir".to_string(), "Heal Everyone".to_string())
                };
                if theme::primary_button(ui, icon::FIRST_AID, &heal_lbl, connected && !busy).clicked() {
                    action = Some((t!("iyileştirme", "heal").into(), json!({"cmd": "heal"})));
                }
                let ban_lbl = if banned > 0 {
                    t!(format!("Cezaları Kaldır ({banned})"), format!("Clear Suspensions ({banned})"))
                } else {
                    t!("Cezaları Kaldır".to_string(), "Clear Suspensions".to_string())
                };
                if theme::primary_button(ui, icon::WARNING, &ban_lbl, connected && !busy).clicked() {
                    action = Some((t!("cezalar", "suspensions").into(), json!({"cmd": "lift_bans"})));
                }
                if theme::primary_button(ui, icon::SMILEY, t!("Herkesin Moralini Maksimum Yap", "Max Everyone's Morale"), connected && !busy).clicked() {
                    action = Some((t!("moral", "morale").into(), json!({"cmd": "inspire", "morale": 20, "clear_unhappiness": true})));
                }
                if theme::primary_button(ui, icon::BRAIN, t!("Taktik Anlayışını Maksimum Yap", "Maximise Tactical Understanding"), connected && !busy).clicked() {
                    action = Some((t!("taktik anlayışı", "tactical understanding").into(), json!({"cmd": "max_understanding"})));
                }
            });
        });
        ui.add_space(12.0);
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
            ui.label(theme::h2(t!("Taktik Anlayışı", "Tactical Understanding")));
            ui.add(
                egui::Label::new(theme::faint(t!(
                    "Kadronun mevcut taktiğinizi alan bazında ne kadar iyi bildiği (oyuncuların ortalaması). Maksimuma çıkarma işlemi FM oyun içi editörünün yaptığını yapar: her şeyi %100'e çıkarır.",
                    "How well the squad knows your current tactic per area (player average). Maximising does what FM's in-game editor does: sets everything to 100%."
                )))
                .wrap(),
            );
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
                let squad_rows = self.live.squad.clone();
                for p in &squad_rows {
                    let uid = p.get("uid").and_then(|v| v.as_u64()).unwrap_or(0);
                    let m = p.get("morale").and_then(|v| v.as_u64()).unwrap_or(0);
                    let pname = p.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        self.draw_player_avatar(ui, ctx, uid as u32, 26.0);
                        ui.label(theme::label_strong(pname));
                    });
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
                    });
                    ui.end_row();
                }
            });
        });
        if let Some((l, r)) = action {
            self.live.refresh(ctx, &l, Some(r));
        }
    }

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
        // bij het openen één keer vanzelf ophalen, eğer henüz okunmadıysa
        if connected && !self.tactics_asked && self.live.tactics.is_none() && from_file.is_none() {
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
        ui.add_space(12.0);

        // ---- KAYITLI İLK 11 ŞABLONLARI & HIZLI DİZİLİŞ BARI ----
        let mut lineup_action: Option<(String, Vec<u32>)> = None;
        let current_uids: Vec<u32> = places.iter().map(|pl| pl.uid.unwrap_or(0)).collect();
        let best_uids: Vec<u32> = if best.len() == places.len() {
            best.iter().map(|&j| data.players[j].uid).collect()
        } else {
            Vec::new()
        };

        theme::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(theme::ic(icon::USERS_THREE).color(theme::ACCENT_HI).size(16.0));
                ui.label(theme::label_strong(t!(
                    "Kayıtlı İlk 11 Şablonları:",
                    "Saved Starting XI Templates:",
                    "Modelos de 11 Inicial Guardados:"
                )));

                if !best_uids.is_empty()
                    && theme::primary_button(
                        ui,
                        icon::SPARKLE,
                        t!("En İyi 11'i Oyuna Diz", "Apply Best XI to Game", "Aplicar Melhor 11 no Jogo"),
                        connected,
                    )
                    .clicked()
                {
                    lineup_action = Some((t!("En İyi 11", "Best XI", "Melhor 11").into(), best_uids.clone()));
                }

                let mut delete_idx: Option<usize> = None;
                let saved_clone = self.settings.saved_lineups.clone();
                for (idx, sl) in saved_clone.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        if theme::button(
                            ui,
                            icon::STRATEGY,
                            &sl.name,
                            connected,
                        )
                        .on_hover_text(t!(
                            "Bu kayıtlı 11'i FM27'de aktif taktiğe dizmek için tıklayın",
                            "Click to load this saved XI into FM27",
                            "Clique para carregar este 11 guardado no FM27"
                        ))
                        .clicked()
                        {
                            lineup_action = Some((sl.name.clone(), sl.uids.clone()));
                        }
                        if theme::small_button(ui, "×", true)
                            .on_hover_text(t!("Şablonu sil", "Delete template", "Eliminar modelo"))
                            .clicked()
                        {
                            delete_idx = Some(idx);
                        }
                    });
                }
                if let Some(d) = delete_idx {
                    self.settings.saved_lineups.remove(d);
                    self.save_settings();
                }

                ui.separator();
                let name_edit = egui::TextEdit::singleline(&mut self.new_lineup_name)
                    .hint_text(t!("Şablon adı (örn. As 11, Kupa 11)…", "Template name (e.g. Main XI, Cup XI)…", "Nome do modelo (ex. 11 Titular)…"))
                    .desired_width(175.0);
                ui.add(name_edit);
                let can_save = current_uids.iter().filter(|&&u| u != 0).count() >= 7
                    && self.settings.saved_lineups.len() < 10;
                if theme::button(
                    ui,
                    icon::CHECK_CIRCLE,
                    t!("Mevcut 11'i Kaydet", "Save Current XI", "Guardar 11 Atual"),
                    can_save,
                )
                .clicked()
                {
                    let name = if self.new_lineup_name.trim().is_empty() {
                        format!("11 #{}", self.settings.saved_lineups.len() + 1)
                    } else {
                        self.new_lineup_name.trim().to_string()
                    };
                    self.settings.saved_lineups.push(tx::SavedLineup {
                        name,
                        uids: current_uids.clone(),
                    });
                    self.new_lineup_name.clear();
                    self.save_settings();
                }
            });
        });
        if let Some((lbl, uids)) = lineup_action {
            self.live.apply_lineup(ctx, &lbl, uids);
        }
        ui.add_space(12.0);

        // ---- GÖRSEL SAHA DİZİLİŞİ (PITCH VIEW) + İLK 11 TABLOSU ----
        let pitch_coords = tx::pitch_coordinates(&places);
        for &i in &order {
            if let Some(uid) = places[i].uid {
                let _ = self.ensure_player_avatar_loaded(ctx, uid);
            }
        }

        let avail_w = ui.available_width();
        let pitch_w = 345.0f32.min(avail_w * 0.34).max(280.0);
        let table_w = (avail_w - pitch_w - 14.0).max(400.0);

        ui.horizontal_top(|ui| {
            // SOL: GÖRSEL SAHA DİZİLİŞİ (Pitch View)
            ui.vertical(|ui| {
                ui.set_width(pitch_w);
                ui.set_max_width(pitch_w);
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(theme::h2(t!("Saha Dizilişi", "Pitch View", "Vista do Campo")));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            theme::pill(ui, &format!("{avg:.0}%"), theme::rgb(level_color(avg)));
                        });
                    });
                    ui.add_space(6.0);
                    self.draw_tactics_pitch(ui, &places, &pitch_coords, &by_uid, pitch_w - 28.0);
                });
            });

            ui.add_space(12.0);

            // SAĞ: BU TAKTİKTEKİ İLK 11'İNİZ TABLOSU
            ui.vertical(|ui| {
                ui.set_width(table_w);
                ui.set_max_width(table_w);
                theme::card(ui, |ui| {
                    ui.label(theme::h2(t!("Bu Taktikteki İlk 11'iniz", "Your Starting XI in This Tactic")));
                    ui.add_space(6.0);
                    egui::Grid::new("tactic_slots").num_columns(6).spacing([14.0, 9.0]).striped(true).show(ui, |ui| {
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
                                ui.spacing_mut().item_spacing.x = 7.0;
                                self.draw_player_avatar(ui, ctx, p.uid, 26.0);
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
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        self.draw_player_avatar(ui, ctx, q.uid, 22.0);
                                        ui.label(RichText::new(format!("{} (+{:.0})", q.name, qf - f)).color(theme::SUCCESS));
                                    });
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
            });
        });
        ui.add_space(12.0);

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
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.scope(|ui| {
                            ui.set_width(90.0);
                            ui.label(theme::label_strong(pl.line.label(pl.side)));
                        });
                        if let Some(np) = now {
                            self.draw_player_avatar(ui, ctx, np.uid, 22.0);
                            ui.label(theme::dim(&np.name));
                        } else {
                            ui.label(theme::dim(t!("boş", "empty")));
                        }
                        ui.label(theme::ic(icon::ARROW_RIGHT).color(theme::TEXT_FAINT));
                        self.draw_player_avatar(ui, ctx, b.uid, 22.0);
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
        let role_tips: Vec<(u32, String, &'static str, &'static str, &'static str, f32, f32)> = order
            .iter()
            .filter_map(|&i| {
                let pl = &places[i];
                let p = pl.uid.and_then(|u| by_uid.get(&u))?;
                let (r, now, then) = tx::better_role(p, pl)?;
                Some((p.uid, p.name.clone(), tx::ip_role(pl.ip?)?.code_str(), r.code_str(), r.name_str(), now, then))
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
            for (puid, name, cur, code, full, now, then) in &role_tips {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    self.draw_player_avatar(ui, ctx, *puid, 22.0);
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
                    if theme::segmented(
                        ui,
                        &mut lang,
                        &[(Lang::Tr, "Türkçe"), (Lang::En, "English"), (Lang::Pt, "Português")],
                    ) {
                        self.settings.lang = lang;
                        self.settings.lang_chosen = true;
                        i18n::set(lang);
                        dirty = true;
                    }
                });
                ui.add_space(6.0);
                dirty |= ui
                    .checkbox(
                        &mut self.settings.capture_delay,
                        t!("Okumadan önce FM penceresini öne getir", "Bring FM window to front before reading"),
                    )
                    .changed();
                ui.add_space(10.0);
                ui.label(RichText::new(t!("FM27 Graphics Klasörü (Logolar, Formalar, Yüzler):", "FM27 Graphics Folder (Logos, Kits, Faces):")).color(theme::TEXT));
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    let avail = (ui.available_width() - 145.0).max(120.0);
                    if ui
                        .add_sized(
                            vec2(avail, 26.0),
                            egui::TextEdit::singleline(&mut self.settings.graphics_dir),
                        )
                        .changed()
                    {
                        self.settings.faces_dir = PathBuf::from(&self.settings.graphics_dir)
                            .join("faces")
                            .to_string_lossy()
                            .into_owned();
                        self.club_logo_textures.clear();
                        self.face_textures.clear();
                        dirty = true;
                    }
                    if theme::small_button(ui, t!("Gözat…", "Browse…"), true).clicked() {
                        if let Some(chosen) = pick_folder_dialog(&self.settings.graphics_dir) {
                            self.settings.graphics_dir = chosen.clone();
                            self.settings.faces_dir = PathBuf::from(&chosen).join("faces").to_string_lossy().into_owned();
                            self.face_textures.clear();
                            self.club_logo_textures.clear();
                            dirty = true;
                        }
                    }
                    if theme::small_button(ui, t!("Yenile", "Reload"), true).clicked() {
                        self.club_logo_textures.clear();
                        self.face_textures.clear();
                    }
                });
                ui.label(theme::faint(t!(
                    "FM'ye bağlı kulübün logosunu (logos) ve tüm oyuncu yüzlerini (faces) bu graphics klasörü içinden otomatik bulur ve yükler.",
                    "Automatically finds and loads the connected club's logo (logos) and all player portraits (faces) directly from this graphics folder."
                )));
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
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("Özel Kamp Modu:", "Special Camp Mode:"));
                    egui::ComboBox::from_id_salt("settings_camp_mode")
                        .selected_text(self.settings.planner.camp_mode.label())
                        .width(210.0)
                        .show_ui(ui, |ui| {
                            for c in planner::CampMode::ALL {
                                if ui.selectable_value(&mut self.settings.planner.camp_mode, c, c.label()).changed() {
                                    dirty = true;
                                }
                            }
                        });
                });
                ui.add_space(4.0);
                dirty |= ui
                    .checkbox(
                        &mut self.settings.planner.auto_smart_load,
                        t!(
                            "Kadro kondisyonuna göre akıllı dinlenme yükü ayarı",
                            "Smart rest load adjustment by squad condition"
                        ),
                    )
                    .changed();
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
                    self.update_rx.is_none() && !self.self_updating,
                )
                .clicked()
                {
                    self.dismissed_release_tag = None;
                    self.update_status = t!("GitHub kontrol ediliyor…", "Checking GitHub…").into();
                    self.update_rx = Some(spawn_update_check());
                }
                if let Some(rel) = self.latest_release.clone() {
                    ui.add_space(6.0);
                    let lbl = if self.self_updating {
                        t!("Güncelleniyor…", "Updating…").to_string()
                    } else {
                        t!(format!("Yeni sürüm ({}) · Şimdi Güncelle", rel.tag), format!("New Version ({}) · Update Now", rel.tag))
                    };
                    if theme::primary_button(
                        ui,
                        icon::ARROW_CLOCKWISE,
                        &lbl,
                        !self.self_updating,
                    )
                    .clicked()
                    {
                        self.dismissed_release_tag = None;
                        if let Some(exe) = rel.exe_url {
                            self.self_updating = true;
                            self.update_status = t!("Yeni sürüm indiriliyor ve kuruluyor…", "Downloading and installing new version…").into();
                            self.self_update_rx = Some(spawn_self_update(exe));
                        } else {
                            open_url(&rel.html_url);
                        }
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
            ui.label(theme::h2(t!("Hakkında & Teşekkürler", "About & Credits")));
            ui.add(egui::Label::new(theme::dim(t!(
                "Seans adları, limitler ve şablonlar FM27'nin kendi oyun verilerinden alınmıştır. Modül yalnızca kontrolden sonra okur ve yazar, geri alma (undo) desteklidir. Yalnızca tek oyunculu (single-player) kullanım içindir. Yazı tipi: Inter (SIL OFL). İkonlar: Phosphor (MIT).",
                "Session names, limits and templates come from FM27's own game data. The module only reads and writes after verification, with undo support. For single-player use only. Font: Inter (SIL OFL). Icons: Phosphor (MIT)."
            )))
            .wrap());
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::dim(t!(
                    "Orijinal FM27 Trainer mimarisi ve temel proje:",
                    "Original FM27 Trainer architecture & base project:"
                )));
                if ui
                    .add(
                        egui::Label::new(RichText::new("SemHiel2003 (github.com/SemHiel2003)").color(theme::ACCENT_HI))
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    open_url("https://github.com/SemHiel2003");
                }
            });
        });
        if dirty {
            self.save_settings();
            self.week_plan = None;
            self.year.clear();
        }
    }

    fn render_update_notice(&mut self, ui: &mut Ui) {
        let Some(rel) = self.latest_release.clone() else { return };
        if self.dismissed_release_tag.as_deref() == Some(rel.tag.as_str()) {
            return;
        }
        let inner_w = (ui.available_width() - 28.0).max(200.0);
        Frame::new()
            .fill(theme::tint(theme::ACCENT, 0.14))
            .stroke(Stroke::new(1.0_f32, theme::tint(theme::ACCENT_HI, 0.55)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_width(inner_w);
                ui.set_max_width(inner_w);
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::ic(icon::ARROW_CLOCKWISE).size(18.0).color(theme::ACCENT_HI));
                    ui.label(
                        RichText::new(t!(
                            format!("Yeni Güncelleme Mevcut: {}", rel.tag),
                            format!("New Update Available: {}", rel.tag)
                        ))
                        .size(14.5)
                        .strong()
                        .color(theme::TEXT),
                    );
                    theme::pill(
                        ui,
                        &t!(
                            format!("Mevcut: v{}", env!("CARGO_PKG_VERSION")),
                            format!("Current: v{}", env!("CARGO_PKG_VERSION"))
                        ),
                        theme::TEXT_DIM,
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::small_button(ui, t!("Gizle", "Dismiss"), !self.self_updating).clicked() {
                            self.dismissed_release_tag = Some(rel.tag.clone());
                        }
                        if theme::small_button(ui, t!("GitHub'da Gör", "View on GitHub"), true).clicked() {
                            open_url(&rel.html_url);
                        }
                        let btn_txt = if self.self_updating {
                            t!("İndiriliyor ve Kuruluyor…", "Downloading & Installing…")
                        } else if rel.exe_url.is_some() {
                            t!("Şimdi Güncelle ve Yeniden Başlat", "Update Now & Restart")
                        } else {
                            t!("Yeni Sürümü İndir", "Download New Version")
                        };
                        if theme::primary_button(ui, icon::ARROW_CLOCKWISE, btn_txt, !self.self_updating).clicked() {
                            if let Some(exe) = rel.exe_url.clone() {
                                self.self_updating = true;
                                self.update_status = t!(
                                    "Yeni sürüm indiriliyor, tamamlandığında uygulama otomatik yeniden başlayacak…",
                                    "Downloading new version, app will restart automatically when complete…"
                                )
                                .into();
                                self.self_update_rx = Some(spawn_self_update(exe));
                            } else {
                                open_url(&rel.html_url);
                            }
                        }
                        if self.self_updating {
                            ui.spinner();
                        }
                    });
                });

                if !rel.notes.is_empty() {
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("update_notes_scroll")
                        .max_height(95.0)
                        .show(ui, |ui| {
                            for line in rel.notes.lines().map(str::trim).filter(|l| !l.is_empty()).take(12) {
                                let clean = line
                                    .trim_start_matches('#')
                                    .trim_start_matches('*')
                                    .trim_start_matches('-')
                                    .trim();
                                if !clean.is_empty() {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 5.0;
                                        ui.label(RichText::new("•").size(12.0).color(theme::ACCENT_HI));
                                        ui.label(RichText::new(clean).size(12.0).color(theme::TEXT_DIM));
                                    });
                                }
                            }
                        });
                }
                if self.self_updating && !self.update_status.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&self.update_status).size(12.0).color(theme::ACCENT_HI));
                }
            });
        ui.add_space(12.0);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.snapshot(ctx);
        self.poll_analysis(ctx);
        self.poll_update(ctx);
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
        self.live.auto_individual = false;
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
                    self.render_update_notice(ui);
                    match self.page {
                        Page::Overview => self.page_overview(ui, ctx),
                        Page::Training => self.page_training(ui, ctx),
                        Page::EasyTalk => self.page_easy_talk(ui, ctx),
                        Page::Planning => self.page_planning(ui),
                        Page::Squad => self.page_squad(ui, ctx),
                        Page::Tactics => self.page_tactics(ui, ctx),
                        Page::Settings => self.page_settings(ui),
                    }
                    ui.add_space(20.0);
                });
            });
        if self.analysis_rx.is_some() || self.live.busy() || self.self_updating {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(1000));
        }
    }
}

// ------------------------------------------------------------------------ helpers

/// Contactgegevens.
const KICK_URL: &str = "https://kick.com/holyman3";
const DISCORD_URL: &str = "https://discord.gg/SYExYVpHP9";
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

fn chip(ui: &mut Ui, name: &str, highlight: bool, faded: bool) -> egui::Response {
    let col = if name == "Match" { theme::DANGER } else { theme::rgb(category_of(name).color()) };
    let col = if faded { theme::tint(col, 0.45) } else { col };
    theme::session_chip(ui, &chip_name(name), col, highlight).on_hover_text(crate::model::session_tr(name))
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

fn week_card(
    ui: &mut Ui,
    w: &live_training::WeekPreview,
    today: Option<NaiveDate>,
    fm_had: &std::collections::HashMap<NaiveDate, [u16; 3]>,
    swap_slot: &mut Option<(NaiveDate, usize, usize)>,
) -> Option<(NaiveDate, Vec<[u16; 3]>, String)> {
    let mut edited: Option<(NaiveDate, Vec<[u16; 3]>, String)> = None;
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
                let can_edit = !past && w.editable[d];
                ui.label(theme::caps(i18n::weekday_short(d)));
                ui.label(theme::faint(i18n::date_short(date)));
                let differs = w.current[d] != w.planned[d];
                let was = if differs { Some(w.current[d]) } else { fm_had.get(&date).copied().filter(|h| *h != w.planned[d]) };
                for (s_idx, &code) in w.planned[d].iter().enumerate() {
                    let name = session_by_code(code).unwrap_or("?");
                    let is_selected = *swap_slot == Some((w.start, d, s_idx));
                    let resp = chip(ui, name, (was.is_some() && !past) || is_selected, !can_edit);
                    if can_edit && code != 500 && code != 402 {
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        if is_selected {
                            ui.painter().rect_stroke(
                                resp.rect.expand(1.5),
                                CornerRadius::same(7),
                                Stroke::new(2.0_f32, theme::ACCENT_HI),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if resp.clicked() {
                            match *swap_slot {
                                Some((sw_start, sw_d, sw_s)) if sw_start == w.start => {
                                    if sw_d == d && sw_s == s_idx {
                                        *swap_slot = None;
                                    } else {
                                        let mut new_days = w.planned.clone();
                                        let tmp = new_days[sw_d][sw_s];
                                        new_days[sw_d][sw_s] = new_days[d][s_idx];
                                        new_days[d][s_idx] = tmp;
                                        *swap_slot = None;
                                        let msg = t!(
                                            format!("{} ile {} antrenman seansları yer değiştirildi ve FM27'ye yazıldı.", i18n::weekday_short(sw_d), i18n::weekday_short(d)),
                                            format!("Swapped training sessions between {} and {} and wrote to FM27.", i18n::weekday_short(sw_d), i18n::weekday_short(d))
                                        );
                                        edited = Some((w.start, new_days, msg));
                                    }
                                }
                                _ => {
                                    *swap_slot = Some((w.start, d, s_idx));
                                }
                            }
                        }
                        resp.context_menu(|ui| {
                            ui.set_min_width(210.0);
                            ui.label(theme::caps(t!("SEANSI DEĞİŞTİR", "CHANGE SESSION", "ALTERAR SESSÃO")));
                            ui.separator();
                            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                for sess in crate::model::SESSIONS {
                                    if matches!(
                                        sess.name,
                                        "Match"
                                            | "Travel"
                                            | "Not Available"
                                            | "Rest (IM)"
                                            | "Recovery (IM)"
                                            | "Light Conditioning (IM)"
                                            | "Match Preparation"
                                            | "Tactical Understanding IP"
                                            | "Tactical Understanding OOP"
                                    ) {
                                        continue;
                                    }
                                    if let Some(scode) = crate::model::session_code(sess.name) {
                                        let label = format!("{} ({})", chip_name(sess.name), sess.category.label());
                                        if ui.selectable_label(scode == code, label).clicked() {
                                            let mut new_days = w.planned.clone();
                                            new_days[d][s_idx] = scode;
                                            *swap_slot = None;
                                            let msg = t!(
                                                format!("{} {}. öğün '{}' olarak değiştirildi ve FM27'ye yazıldı.", i18n::weekday_short(d), s_idx + 1, chip_name(sess.name)),
                                                format!("Changed {} slot {} to '{}' and wrote to FM27.", i18n::weekday_short(d), s_idx + 1, chip_name(sess.name))
                                            );
                                            edited = Some((w.start, new_days, msg));
                                            ui.close_menu();
                                        }
                                    }
                                }
                            });
                        });
                    }
                }
                if let Some(h) = was.filter(|_| !past && w.editable[d]) {
                    ui.add(egui::Label::new(RichText::new(format!("FM: {}", codes_text(&h))).size(10.5).color(theme::TEXT_FAINT).strikethrough()).truncate())
                        .on_hover_text(t!(format!("FM'nin planladığı: {}", codes_text(&h)), format!("Planned by FM: {}", codes_text(&h))));
                }
            }
        });
    });
    edited
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
