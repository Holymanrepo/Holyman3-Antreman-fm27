#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent;
mod export;
mod gui;
mod i18n;
mod labels;
mod live;
mod live_training;
mod model;
mod planner;
mod tactics;
mod theme;
mod vision;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use model::Effectiveness;

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into()))
}

/// Instellingen en de socket van de module.
fn data_dir() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("FM27 Trainer")
    } else {
        home().join("Library/Application Support/FM27 Trainer")
    }
}

/// Map van FM27 met saves, tactieken en schema's.
fn fm_user_dir() -> PathBuf {
    if cfg!(windows) {
        home().join("Documents").join("Sports Interactive").join("Football Manager 27")
    } else {
        home().join("Library/Application Support/Sports Interactive/Football Manager 27")
    }
}

fn effectiveness_file() -> PathBuf {
    data_dir().join("effectiveness.json")
}

fn load_effectiveness() -> Option<Effectiveness> {
    let s = std::fs::read_to_string(effectiveness_file()).ok()?;
    serde_json::from_str(&s).ok()
}

fn save_effectiveness(e: &Effectiveness) -> Result<()> {
    std::fs::create_dir_all(data_dir())?;
    std::fs::write(effectiveness_file(), serde_json::to_string_pretty(e)?)?;
    Ok(())
}

fn print_analysis(a: &vision::Analysis) {
    for g in &a.gauges {
        let doubt = if g.doubt { format!("  ?? doluluk {:.0} % renk tonu {:.0}° ile uyuşmuyor", g.fill * 100.0, g.hue) } else { String::new() };
        println!("{:<36} {:>5.1}  ({}){doubt}", g.factor.label(), g.score, model::level_name(g.score));
    }
    for (c, v) in &a.components {
        println!("[{}] {:.1}", c.label(), v);
    }
    if !a.missing.is_empty() {
        println!("bulunamadı: {:?}", a.missing);
    }
    if !a.lang.starts_with("english") {
        println!("FM dili: {} (OCR {})", a.lang, a.ocr);
    }
}

fn cli(args: &[String]) -> Result<bool> {
    match args.get(1).map(String::as_str) {
        Some("analyze") => {
            let (a, lines) = match args.get(2).filter(|a| !a.starts_with("--")) {
                Some(p) => vision::analyze_file(Path::new(p))?,
                None => {
                    let img = vision::capture_fm_window()?;
                    vision::analyze_image(&img)?
                }
            };
            if args.iter().any(|a| a == "--ocr") {
                print!("{}", vision::report(&a, &lines));
                return Ok(true);
            }
            print_analysis(&a);
            if args.iter().any(|a| a == "--save") {
                save_effectiveness(&a.to_effectiveness("Dağılım ekran görüntüsü"))?;
                println!("kaydedildi: {}", effectiveness_file().display());
            }
            Ok(true)
        }
        Some("windows") => {
            #[cfg(target_os = "macos")]
            print!("{}", vision::mac_windows());
            if let Some(p) = args.get(2) {
                let img = vision::capture_fm_window()?;
                img.save(p)?;
                println!("görüntü {}x{} → {p}", img.width(), img.height());
            }
            Ok(true)
        }
        Some("year") => {
            let eff = load_effectiveness().context("henüz okunmuş değer yok; önce `analyze <png> --save` çalıştırın")?;
            let mut opts = planner::CalendarOptions::default();
            if let Some(d) = args.get(2).and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()) {
                opts.start = d;
            }
            let cal = planner::default_calendar(&opts);
            let plans = planner::plan_year(&cal, &eff, &planner::PlannerSettings::default());
            let html = export::write_all(&export::output_dir(), "yillik-plan", "FM27 Yıllık Antrenman Planı", &eff, &plans)?;
            println!("{} hafta, {} benzersiz program → {}", plans.len(), planner::unique_schedules(&plans).len(), html.display());
            Ok(true)
        }
        Some("agent") => {
            let req: serde_json::Value = serde_json::from_str(args.get(2).context("JSON komutu eksik")?)?;
            println!("{}", serde_json::to_string_pretty(&agent::request(req)?)?);
            Ok(true)
        }
        Some("agent-serve") => {
            use std::io::{BufRead, Write};
            let stdin = std::io::stdin();
            let mut out = std::io::stdout().lock();
            for line in stdin.lock().lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let reply = match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(req) => match agent::request(req) {
                        Ok(v) => serde_json::json!({"ok": true, "data": v}),
                        Err(e) => serde_json::json!({"ok": false, "error": format!("{e:#}")}),
                    },
                    Err(e) => serde_json::json!({"ok": false, "error": format!("geçersiz istek: {e}")}),
                };
                writeln!(out, "{reply}")?;
                out.flush()?;
            }
            Ok(true)
        }
        Some("train") => {
            let eff = load_effectiveness().context("henüz okunmuş değer yok")?;
            let write = args.iter().any(|a| a == "--write");
            let o = live_training::run(&eff, &planner::PlannerSettings::default(), write, &Default::default())?;
            println!("oyun tarihi {:?}, {} gün yazıldı", o.today, o.written_days);
            for w in &o.weeks {
                println!("hafta {} ({}M, {})", w.start, w.matches, w.focus);
                for d in 0..7 {
                    let f = |x: &[u16; 3]| x.iter().map(|c| live_training::short(*c)).collect::<Vec<_>>().join("/");
                    let mark = if w.current[d] != w.planned[d] { "→" } else { " " };
                    println!("   {} {:<16} {} {}", d, f(&w.current[d]), mark, if w.current[d] != w.planned[d] { f(&w.planned[d]) } else { String::new() });
                }
            }
            Ok(true)
        }
        Some("start-fm") => {
            println!("{}", agent::start_fm()?);
            Ok(true)
        }
        Some("help" | "-h" | "--help") => {
            println!("fm27-trainer            uygulamayı başlat\nfm27-trainer analyze [png] [--ocr] [--save]\nfm27-trainer year [YYYY-AA-GG]");
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if cli(&args)? {
        return Ok(());
    }
    gui::run()
}
