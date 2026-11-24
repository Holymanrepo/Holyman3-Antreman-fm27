//! Tactiekhulp: past de tactiek uit FM op de selectie. Per plek hoe goed de speler bij de rol en
//! de positie past, wie beter zou passen, en de beste elf voor de tactiek.
//!
//! De rollen en de plekken komen uit FM's geheugen (opdracht `tactics` van de module). Welke
//! attributen bij een rol horen, staat niet in FM's data: de lijsten hieronder zijn onze eigen
//! inschatting, en de score is dus een hulpmiddel, geen cijfer van FM. De wedstrijdengine beslist.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Clone, Debug)]
pub struct Slot {
    pub uid: Option<u32>,
    pub ip: Option<u8>,
    pub oop: Option<u8>,
    pub oop_order: u8,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Tactic {
    pub index: usize,
    pub style: String,
    pub slots: Vec<Slot>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Player {
    pub uid: u32,
    pub name: String,
    pub positions: Vec<u8>,
    pub attributes: HashMap<String, u8>,
    #[serde(default)]
    pub left_foot: u8,
    #[serde(default)]
    pub right_foot: u8,
    #[serde(default = "default_hundred")]
    pub condition: u8,
    #[serde(default = "default_hundred")]
    pub sharpness: u8,
    #[serde(default = "default_fifteen")]
    pub morale: u8,
    #[serde(default)]
    pub injured: bool,
    #[serde(default)]
    pub banned: bool,
}

fn default_hundred() -> u8 {
    100
}
fn default_fifteen() -> u8 {
    15
}

#[derive(Deserialize, Clone, Debug)]
pub struct Data {
    pub current: usize,
    pub tactics: Vec<Tactic>,
    pub players: Vec<Player>,
    #[serde(default)]
    pub bench: Vec<u32>,
}

// ---- posities ---------------------------------------------------------------------------------

/// Index in `Player.positions`.
const GK: usize = 0;
const DL: usize = 2;
const DC: usize = 3;
const DR: usize = 4;
const DM: usize = 5;
const ML: usize = 6;
const MC: usize = 7;
const MR: usize = 8;
const AML: usize = 9;
const AMC: usize = 10;
const AMR: usize = 11;
const ST: usize = 12;
const WBL: usize = 13;
const WBR: usize = 14;

/// De linie van een plek.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Line {
    Keeper,
    CentreBack,
    /// back of wingback
    WideBack,
    DefMid,
    /// verdedigende of centrale middenvelder (spelmaker die op beide kan staan)
    DefOrCentreMid,
    CentreMid,
    /// centrale of aanvallende middenvelder
    CentreOrAttMid,
    AttMid,
    /// buitenspeler of vleugelmiddenvelder
    Wide,
    Striker,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Right,
    Left,
    Centre,
}

impl Line {
    fn wide(self) -> bool {
        matches!(self, Line::WideBack | Line::Wide)
    }

    /// De positiecijfers die voor deze linie tellen.
    fn positions(self, side: Side) -> &'static [usize] {
        match (self, side) {
            (Line::Keeper, _) => &[GK],
            (Line::CentreBack, _) => &[DC],
            (Line::WideBack, Side::Left) => &[DL, WBL],
            (Line::WideBack, Side::Right) => &[DR, WBR],
            (Line::WideBack, Side::Centre) => &[DL, DR, WBL, WBR],
            (Line::DefMid, _) => &[DM],
            (Line::DefOrCentreMid, _) => &[DM, MC],
            (Line::CentreMid, _) => &[MC],
            (Line::CentreOrAttMid, _) => &[MC, AMC],
            (Line::AttMid, _) => &[AMC],
            (Line::Wide, Side::Left) => &[AML, ML],
            (Line::Wide, Side::Right) => &[AMR, MR],
            (Line::Wide, Side::Centre) => &[AML, AMR, ML, MR],
            (Line::Striker, _) => &[ST],
        }
    }

    pub fn label(self, side: Side) -> &'static str {
        if crate::i18n::is_tr() {
            match (self, side) {
                (Line::Keeper, _) => "KL",
                (Line::CentreBack, _) => "STP",
                (Line::WideBack, Side::Left) => "SLB / KSL",
                (Line::WideBack, Side::Right) => "SĞB / KSĞ",
                (Line::WideBack, Side::Centre) => "BEK",
                (Line::DefMid, _) => "DOS",
                (Line::DefOrCentreMid, _) => "DOS / OS",
                (Line::CentreMid, _) => "OS (M)",
                (Line::CentreOrAttMid, _) => "OS / OOS",
                (Line::AttMid, _) => "OOS (M)",
                (Line::Wide, Side::Left) => "SLK (Sol Kanat)",
                (Line::Wide, Side::Right) => "SĞK (Sağ Kanat)",
                (Line::Wide, Side::Centre) => "KANAT",
                (Line::Striker, _) => "SNT (Forvet)",
            }
        } else {
            match (self, side) {
                (Line::Keeper, _) => "GK",
                (Line::CentreBack, _) => "DC",
                (Line::WideBack, Side::Left) => "DL / WBL",
                (Line::WideBack, Side::Right) => "DR / WBR",
                (Line::WideBack, Side::Centre) => "FB / WB",
                (Line::DefMid, _) => "DM",
                (Line::DefOrCentreMid, _) => "DM / MC",
                (Line::CentreMid, _) => "MC",
                (Line::CentreOrAttMid, _) => "MC / AMC",
                (Line::AttMid, _) => "AMC",
                (Line::Wide, Side::Left) => "AML / ML",
                (Line::Wide, Side::Right) => "AMR / MR",
                (Line::Wide, Side::Centre) => "Winger",
                (Line::Striker, _) => "ST",
            }
        }
    }
}

// ---- rollen -----------------------------------------------------------------------------------

pub struct Role {
    pub bit: u8,
    pub code: &'static str,
    pub name: &'static str,
    pub en_code: &'static str,
    pub en_name: &'static str,
    pub line: Line,
    /// de attributen die voor de rol tellen (onze inschatting)
    pub attrs: &'static [&'static str],
}

impl Role {
    pub fn code_str(&self) -> &'static str {
        if crate::i18n::is_tr() { self.code } else { self.en_code }
    }
    pub fn name_str(&self) -> &'static str {
        if crate::i18n::is_tr() { self.name } else { self.en_name }
    }
}

macro_rules! role {
    ($bit:expr, $code:expr, $name:expr, $en_code:expr, $en_name:expr, $line:ident, [$($a:ident),*]) => {
        Role { bit: $bit, code: $code, name: $name, en_code: $en_code, en_name: $en_name, line: Line::$line, attrs: &[$(stringify!($a)),*] }
    };
}

/// Rollen met bal (Topla Roller - Türkçe & English).
pub const IP_ROLES: &[Role] = &[
    role!(0, "KL", "Kaleci", "GK", "Goalkeeper", Keeper, [handling, reflexes, aerial_reach, one_on_ones, concentration, positioning, command_of_area, communication]),
    role!(1, "STP", "Stoper", "CB", "Centre-Back", CentreBack, [heading, marking, tackling, positioning, jumping_reach, strength, anticipation, concentration, bravery, pace]),
    role!(2, "BK", "Bek", "FB", "Full-Back", WideBack, [tackling, marking, positioning, crossing, stamina, pace, work_rate, anticipation, teamwork, concentration]),
    role!(3, "KB", "Kanat Bek", "WB", "Wing-Back", WideBack, [crossing, dribbling, stamina, pace, acceleration, work_rate, off_the_ball, tackling, teamwork, technique]),
    role!(4, "DOS", "Defansif Orta Saha", "DM", "Defensive Midfielder", DefMid, [tackling, positioning, anticipation, concentration, marking, teamwork, work_rate, stamina, passing, decisions, strength]),
    role!(5, "SOS", "Savaşçı Orta Saha", "BWM", "Ball Winning Midfielder", DefOrCentreMid, [tackling, aggression, bravery, work_rate, stamina, anticipation, marking, strength, teamwork]),
    role!(6, "OKÖ", "Oyun Kurucu Kanat", "WP", "Wide Playmaker", Wide, [passing, vision, first_touch, technique, dribbling, decisions, composure, flair, off_the_ball]),
    role!(7, "KNT", "Kanat Oyuncusu", "W", "Winger", Wide, [crossing, dribbling, pace, acceleration, technique, agility, off_the_ball, flair, stamina]),
    role!(8, "MOS", "Merkez Orta Saha", "CM", "Central Midfielder", CentreMid, [passing, first_touch, decisions, teamwork, work_rate, stamina, tackling, positioning, off_the_ball]),
    role!(9, "OOS", "Ofansif Orta Saha", "AM", "Attacking Midfielder", AttMid, [first_touch, passing, technique, off_the_ball, vision, decisions, composure, long_shots, flair, anticipation, dribbling]),
    role!(10, "GF", "Geriye Gelen Forvet", "DLF", "Deep Lying Forward", Striker, [first_touch, passing, technique, composure, decisions, off_the_ball, teamwork, vision, finishing, strength]),
    role!(11, "SNT", "Santrfor", "CF", "Centre Forward", Striker, [finishing, off_the_ball, first_touch, composure, anticipation, heading, strength, pace, acceleration, technique, decisions]),
    role!(12, "PKL", "Pasör Kaleci", "BGK", "Ball Playing Goalkeeper", Keeper, [handling, reflexes, one_on_ones, aerial_reach, command_of_area, positioning, concentration, passing, first_touch, composure, kicking, throwing, vision]),
    role!(13, "LİB", "Libero", "LIB", "Libero", CentreBack, [passing, vision, first_touch, composure, decisions, positioning, tackling, anticipation, dribbling, marking, pace]),
    role!(14, "İSTP", "İleri Çıkan Stoper", "ACB", "Advanced Centre-Back", CentreBack, [passing, first_touch, composure, decisions, positioning, tackling, anticipation, dribbling, marking, heading]),
    role!(15, "DOK", "Derin Oyun Kurucu", "DLP", "Deep Lying Playmaker", DefOrCentreMid, [passing, vision, first_touch, composure, decisions, technique, teamwork, anticipation, positioning]),
    role!(16, "İYO", "İki Yönlü Orta Saha", "BBM", "Box to Box Midfielder", CentreMid, [stamina, work_rate, passing, tackling, off_the_ball, teamwork, long_shots, strength, decisions, pace]),
    role!(17, "OOK", "Ofansif Oyun Kurucu", "AP", "Advanced Playmaker", CentreOrAttMid, [passing, vision, first_touch, technique, composure, decisions, flair, off_the_ball, teamwork]),
    role!(18, "HS", "Hedef Santrfor", "TF", "Target Forward", Striker, [heading, jumping_reach, strength, bravery, teamwork, first_touch, finishing, composure]),
    role!(19, "FÇ", "Fırsatçı Golcü", "P", "Poacher", Striker, [finishing, off_the_ball, anticipation, composure, acceleration, first_touch, pace]),
    role!(20, "YF", "Yaratıcı Forvet", "TREQ", "Trequartista", AttMid, [passing, vision, technique, first_touch, dribbling, flair, composure, off_the_ball, decisions, acceleration]),
    role!(21, "KKB", "Komple Kanat Bek", "CWB", "Complete Wing-Back", WideBack, [crossing, dribbling, technique, first_touch, passing, off_the_ball, flair, stamina, pace, acceleration, work_rate, decisions]),
    role!(22, "PF", "Presçi Forvet", "PF", "Pressing Forward", Striker, [work_rate, stamina, aggression, bravery, anticipation, teamwork, pace, acceleration, finishing]),
    role!(23, "GL", "Gizli Libero", "HB", "Half Back", DefMid, [positioning, tackling, marking, anticipation, concentration, composure, passing, decisions, strength, heading]),
    role!(24, "PSTP", "Pasör Stoper", "BCB", "Ball Playing Centre-Back", CentreBack, [heading, marking, tackling, positioning, jumping_reach, strength, anticipation, passing, first_touch, composure, vision, technique]),
    role!(25, "ÇP", "Çapa", "A", "Anchor", DefMid, [positioning, tackling, marking, anticipation, concentration, decisions, strength, composure]),
    role!(26, "REG", "Regista", "REG", "Regista", DefMid, [passing, vision, first_touch, technique, composure, decisions, flair, off_the_ball, teamwork]),
    role!(27, "KNF", "Kanat Forvet", "IF", "Inside Forward", Wide, [dribbling, finishing, first_touch, technique, off_the_ball, acceleration, pace, composure, anticipation, long_shots, agility]),
    role!(28, "GOK", "Gezgin Oyun Kurucu", "RPM", "Roaming Playmaker", CentreMid, [passing, vision, first_touch, technique, composure, decisions, off_the_ball, stamina, work_rate, dribbling, pace]),
    role!(29, "ÇSTP", "Çakılı Stoper", "NCB", "No-Nonsense Centre-Back", CentreBack, [heading, marking, tackling, positioning, strength, jumping_reach, bravery, aggression]),
    role!(30, "ENG", "Enganche", "ENG", "Enganche", AttMid, [passing, vision, first_touch, technique, composure, decisions, flair]),
    role!(31, "KF", "Komple Forvet", "CF", "Complete Forward", Striker, [finishing, off_the_ball, first_touch, composure, anticipation, heading, strength, pace, acceleration, dribbling, technique, vision, passing]),
    role!(32, "RMD", "Raumdeuter", "RMD", "Raumdeuter", Wide, [off_the_ball, anticipation, finishing, composure, concentration, decisions, stamina, acceleration]),
    role!(33, "İKK", "İçe Kat Eden Kanat", "IW", "Inverted Winger", Wide, [dribbling, passing, crossing, technique, first_touch, off_the_ball, acceleration, pace, agility, vision, composure]),
    role!(34, "ÇK", "Çalışkan Kanat", "DW", "Defensive Winger", Wide, [work_rate, stamina, tackling, positioning, marking, teamwork, crossing, dribbling, pace]),
    role!(35, "SSÖ", "Savunma Önü Libero", "DM", "Half Back / DM", DefMid, [positioning, tackling, marking, anticipation, concentration, composure, passing, decisions, strength, heading]),
    role!(36, "MEZ", "Mezzala", "MEZ", "Mezzala", CentreMid, [passing, dribbling, technique, first_touch, off_the_ball, vision, work_rate, stamina, acceleration, long_shots]),
    role!(37, "CAR", "Carrilero", "CAR", "Carrilero", CentreMid, [positioning, tackling, passing, teamwork, stamina, work_rate, decisions, concentration]),
    role!(38, "OKB", "Ofansif Kanat Bek", "AWB", "Attacking Wing-Back", WideBack, [crossing, dribbling, pace, acceleration, stamina, off_the_ball, flair, technique, work_rate]),
    role!(39, "SV", "Segundo Volante", "SV", "Segundo Volante", DefMid, [off_the_ball, finishing, long_shots, passing, tackling, positioning, stamina, work_rate, pace, strength]),
    role!(40, "S9", "Sahte Dokuz", "F9", "False Nine", Striker, [first_touch, passing, technique, vision, dribbling, composure, decisions, off_the_ball, finishing, flair, agility]),
    role!(41, "GF", "Gölge Forvet", "SS", "Shadow Striker", AttMid, [finishing, off_the_ball, first_touch, composure, anticipation, dribbling, acceleration, pace, decisions, work_rate]),
    role!(42, "KHO", "Kanat Hedef Oyuncusu", "WTF", "Wide Target Forward", Wide, [heading, jumping_reach, strength, bravery, off_the_ball, first_touch, teamwork]),
    role!(43, "OKK", "Oyun Kurucu Kanat", "PW", "Playmaking Winger", Wide, [passing, vision, first_touch, technique, dribbling, decisions, composure, flair, off_the_ball]),
    role!(44, "İKB", "İçe Kat Eden Kanat Bek", "IWB", "Inverted Wing-Back", WideBack, [passing, first_touch, decisions, composure, tackling, positioning, teamwork, anticipation, technique]),
    role!(45, "GNF", "Geniş Alan Forveti", "WF", "Wide Forward", Wide, [finishing, off_the_ball, pace, acceleration, dribbling, first_touch, composure, anticipation, heading]),
    role!(46, "İYOK", "İki Yönlü Oyun Kurucu", "BBP", "Box-to-Box Playmaker", CentreMid, [stamina, work_rate, passing, vision, technique, first_touch, composure, off_the_ball, teamwork, decisions]),
    role!(47, "İF", "İleri Uç Forvet", "AF", "Advanced Forward", Striker, [finishing, off_the_ball, first_touch, composure, anticipation, pace, acceleration, dribbling]),
    role!(48, "KOS", "Kanat Orta Saha", "WM", "Wide Midfielder", CentreMid, [passing, crossing, stamina, work_rate, off_the_ball, teamwork, first_touch, decisions]),
    role!(49, "İKK", "İçe Kat Eden Kanat", "IW", "Inside Winger", Wide, [dribbling, passing, crossing, technique, first_touch, off_the_ball, acceleration, pace, agility, flair, composure]),
    role!(50, "HOS", "Hücumcu Orta Saha", "AM", "Attacking Midfielder", CentreOrAttMid, [first_touch, passing, technique, off_the_ball, vision, decisions, composure, long_shots, flair]),
    role!(51, "PSTP", "Pasör Stoper", "BCB", "Ball Playing Centre-Back", CentreBack, [heading, marking, tackling, positioning, jumping_reach, strength, anticipation, passing, first_touch, composure, vision, technique]),
    role!(52, "İBK", "İçe Kat Eden Bek", "IFB", "Inverted Full-Back", WideBack, [tackling, marking, positioning, heading, strength, concentration, passing, anticipation]),
    role!(53, "GKL", "Geleneksel Kaleci", "TGK", "Traditional Goalkeeper", Keeper, [handling, reflexes, aerial_reach, one_on_ones, concentration, positioning, command_of_area, communication]),
    role!(54, "Bind.STP", "Bindiren Stoper", "OCB", "Overlapping Centre-Back", CentreBack, [pace, stamina, crossing, dribbling, tackling, marking, work_rate, positioning, heading]),
    role!(55, "OKKB", "Oyun Kurucu Kanat Bek", "PWB", "Playmaking Wing-Back", WideBack, [passing, vision, technique, crossing, first_touch, decisions, stamina, composure, teamwork]),
    role!(56, "Kor.OS", "Koridor Orta Sahası", "CHM", "Channel Midfielder", CentreMid, [off_the_ball, stamina, work_rate, passing, crossing, acceleration, dribbling, teamwork]),
    role!(57, "MOK", "Merkez Oyun Kurucu", "MPM", "Midfield Playmaker", CentreMid, [passing, vision, first_touch, technique, composure, decisions, teamwork, anticipation, off_the_ball]),
];

/// Rollen zonder bal (Topsuz Roller - Türkçe & English).
pub const OOP_ROLES: &[Role] = &[
    role!(0, "KL", "Kaleci", "GK", "Goalkeeper", Keeper, [handling, reflexes, positioning, concentration, aerial_reach, command_of_area, one_on_ones]),
    role!(1, "LKL", "Libero Kaleci", "SK", "Sweeper Keeper", Keeper, [rushing_out, one_on_ones, anticipation, acceleration, command_of_area, reflexes, handling, decisions, concentration]),
    role!(2, "ÇKL", "Çizgi Kalecisi", "LGK", "Line Goalkeeper", Keeper, [handling, reflexes, positioning, concentration, aerial_reach, command_of_area, one_on_ones]),
    role!(3, "STP", "Stoper", "CB", "Centre-Back", CentreBack, [marking, tackling, positioning, heading, jumping_reach, strength, concentration, anticipation]),
    role!(4, "KSTP", "Kesici Stoper", "SCB", "Stopping Centre-Back", CentreBack, [aggression, bravery, tackling, anticipation, strength, decisions, marking, heading]),
    role!(5, "SSTP", "Sigorta Stoper", "CCB", "Covering Centre-Back", CentreBack, [pace, acceleration, positioning, anticipation, concentration, marking, decisions, tackling]),
    role!(6, "GSTP", "Geniş Stoper", "WCB", "Wide Centre-Back", CentreBack, [tackling, marking, pace, positioning, stamina, anticipation]),
    role!(7, "STP", "Merkez Stoper", "CB", "Central Defender", CentreBack, [marking, tackling, positioning, heading, jumping_reach, strength, concentration, anticipation]),
    role!(8, "LİB", "Sigorta Libero", "SW", "Sweeper", CentreBack, [pace, acceleration, positioning, anticipation, concentration, marking, decisions, tackling]),
    role!(9, "BK", "Bek", "FB", "Full-Back", WideBack, [tackling, marking, positioning, concentration, anticipation, pace, teamwork]),
    role!(10, "PBK", "Presçi Bek", "PFB", "Pressing Full-Back", WideBack, [aggression, work_rate, stamina, tackling, acceleration, anticipation, pace, bravery]),
    role!(11, "ÇBK", "Çakılı Bek", "HFB", "Holding Full-Back", WideBack, [positioning, concentration, marking, tackling, anticipation, decisions]),
    role!(12, "KB", "Kanat Bek", "WB", "Wing-Back", WideBack, [tackling, marking, stamina, work_rate, pace, positioning]),
    role!(13, "PKB", "Presçi Kanat Bek", "PWB", "Pressing Wing-Back", WideBack, [aggression, work_rate, stamina, tackling, acceleration, anticipation, pace]),
    role!(14, "ÇKB", "Çakılı Kanat Bek", "HWB", "Holding Wing-Back", WideBack, [positioning, concentration, marking, tackling, anticipation, stamina]),
    role!(15, "DOS", "Defansif Orta Saha", "DM", "Defensive Midfielder", DefMid, [positioning, tackling, anticipation, concentration, marking, teamwork, strength]),
    role!(16, "GDOS", "Geriye Gömülen DOS", "DDM", "Dropping DM", DefMid, [positioning, marking, heading, concentration, anticipation, tackling, strength, decisions]),
    role!(17, "PDOS", "Presçi DOS", "PDM", "Pressing DM", DefMid, [aggression, work_rate, stamina, tackling, anticipation, acceleration, bravery, teamwork]),
    role!(18, "Per.DOS", "Perdeleyici DOS", "SDM", "Screening DM", DefMid, [positioning, anticipation, concentration, tackling, marking, teamwork, decisions]),
    role!(19, "GDOS", "Geniş DOS", "WDM", "Wide DM", DefMid, [tackling, positioning, work_rate, stamina, anticipation, pace]),
    role!(20, "OS", "Merkez Orta Saha", "CM", "Central Midfielder", CentreMid, [tackling, positioning, work_rate, teamwork, stamina, anticipation, concentration]),
    role!(21, "MOS", "Merkez Orta Saha", "CM", "Central Midfielder", CentreMid, [tackling, positioning, work_rate, teamwork, stamina, anticipation, concentration]),
    role!(22, "KOS", "Kanat Orta Saha", "WM", "Wide Midfielder", CentreMid, [work_rate, stamina, teamwork, tackling, positioning, pace]),
    role!(23, "TKOS", "Takipçi Kanat OS", "TWM", "Tracking Wide Midfielder", CentreMid, [work_rate, stamina, teamwork, tackling, marking, positioning]),
    role!(24, "PKOS", "Presçi Kanat OS", "PWM", "Pressing Wide Midfielder", CentreMid, [work_rate, stamina, aggression, tackling, anticipation, acceleration]),
    role!(25, "ÇKOS", "Çıkış Kanat OS", "OWM", "Outlet Wide Midfielder", CentreMid, [off_the_ball, anticipation, acceleration, pace, composure]),
    role!(26, "DOS", "Tutucu Orta Saha", "HCM", "Holding CM", CentreMid, [positioning, anticipation, concentration, tackling, marking, teamwork]),
    role!(27, "POS", "Presçi Orta Saha", "PCM", "Pressing CM", CentreMid, [work_rate, stamina, aggression, tackling, anticipation, acceleration, teamwork, bravery]),
    role!(28, "Per.OS", "Perdeleyici Orta Saha", "SCM", "Screening CM", CentreMid, [positioning, anticipation, concentration, tackling, marking, teamwork, decisions]),
    role!(29, "OOS", "Ofansif Orta Saha", "AM", "Attacking Midfielder", AttMid, [work_rate, stamina, teamwork, anticipation, positioning, tackling]),
    role!(30, "POOS", "Presçi OOS", "PAM", "Pressing AM", AttMid, [work_rate, stamina, aggression, anticipation, acceleration, tackling, teamwork]),
    role!(31, "OOS", "Ofansif Orta Saha", "AM", "Attacking Midfielder", AttMid, [work_rate, teamwork, anticipation, stamina, positioning, off_the_ball]),
    role!(32, "ÇOOS", "Çıkış Noktası OOS", "OAM", "Outlet AM", AttMid, [off_the_ball, anticipation, acceleration, pace, composure, first_touch]),
    role!(33, "TOOS", "Takipçi OOS", "TAM", "Tracking AM", AttMid, [work_rate, stamina, teamwork, tackling, positioning, marking]),
    role!(34, "KNT", "Kanat Oyuncusu", "W", "Winger", Wide, [work_rate, stamina, teamwork, tackling, positioning, pace, anticipation]),
    role!(35, "DKO", "Defansif Kanat Oyuncusu", "DW", "Defensive Winger", Wide, [work_rate, stamina, teamwork, tackling, marking, positioning, concentration, pace]),
    role!(36, "PKNT", "Presçi Kanat", "PW", "Pressing Winger", Wide, [work_rate, stamina, aggression, tackling, anticipation, acceleration, pace]),
    role!(37, "ÇKNT", "Çıkış Kanadı", "OW", "Outlet Winger", Wide, [off_the_ball, anticipation, acceleration, pace, composure]),
    role!(38, "KNT", "Kanat Oyuncusu", "W", "Winger", Wide, [work_rate, stamina, teamwork, tackling, positioning, pace, anticipation]),
    role!(39, "TKNT", "Takipçi Kanat", "TW", "Tracking Winger", Wide, [work_rate, stamina, teamwork, tackling, marking, positioning, concentration]),
    role!(40, "İÇK", "İç Çıkış Kanadı", "IOW", "Inside Outlet Winger", Wide, [off_the_ball, anticipation, acceleration, pace, composure]),
    role!(41, "GÇK", "Geniş Çıkış Kanadı", "WOW", "Wide Outlet Winger", Wide, [off_the_ball, anticipation, acceleration, pace, composure]),
    role!(42, "SNT", "Santrfor", "CFD", "Centre Forward", Striker, [work_rate, teamwork, anticipation, stamina, aggression, strength, pace]),
    role!(43, "TSNT", "Takipçi Santrfor", "TCF", "Tracking Centre Forward", Striker, [work_rate, stamina, teamwork, tackling, aggression, anticipation]),
    role!(44, "ÇSNT", "Çıkış Noktası Santrfor", "OCF", "Outlet Centre Forward", Striker, [off_the_ball, anticipation, pace, acceleration, strength, composure]),
    role!(45, "PSNT", "Presçi Santrfor", "PCF", "Pressing Centre Forward", Striker, [work_rate, stamina, aggression, bravery, anticipation, acceleration, pace, tackling]),
    role!(46, "BSNT", "Bölücü Santrfor", "SCF", "Splitting Centre Forward", Striker, [off_the_ball, pace, acceleration, anticipation, work_rate, stamina]),
];

pub fn ip_role(bit: u8) -> Option<&'static Role> {
    IP_ROLES.iter().find(|r| r.bit == bit)
}

pub fn oop_role(bit: u8) -> Option<&'static Role> {
    OOP_ROLES.iter().find(|r| r.bit == bit)
}

/// De linie van een rol zonder bal, ook als we de naam niet kennen: uit FM's nummering.
pub fn oop_line(bit: u8) -> Line {
    if let Some(r) = oop_role(bit) {
        return r.line;
    }
    match bit {
        0..=2 => Line::Keeper,
        3..=8 => Line::CentreBack,
        9..=14 => Line::WideBack,
        15..=19 => Line::DefMid,
        20..=28 => Line::CentreMid,
        29..=33 => Line::AttMid,
        34..=41 => Line::Wide,
        _ => Line::Striker,
    }
}

/// Wat bij een linie telt als we de rol niet kennen.
fn generic(line: Line) -> &'static [&'static str] {
    match line {
        Line::Keeper => &["handling", "reflexes", "one_on_ones", "aerial_reach", "command_of_area", "positioning", "concentration"],
        Line::CentreBack => &["heading", "marking", "tackling", "positioning", "strength", "anticipation", "concentration"],
        Line::WideBack => &["tackling", "marking", "positioning", "crossing", "stamina", "pace", "work_rate"],
        Line::DefMid | Line::DefOrCentreMid => &["tackling", "positioning", "anticipation", "passing", "teamwork", "work_rate", "decisions"],
        Line::CentreMid | Line::CentreOrAttMid => &["passing", "first_touch", "decisions", "teamwork", "work_rate", "stamina", "off_the_ball"],
        Line::AttMid => &["first_touch", "passing", "technique", "off_the_ball", "vision", "decisions", "composure"],
        Line::Wide => &["dribbling", "crossing", "pace", "acceleration", "technique", "off_the_ball", "work_rate"],
        Line::Striker => &["finishing", "off_the_ball", "first_touch", "composure", "anticipation", "pace", "strength"],
    }
}

pub fn ip_label(bit: Option<u8>) -> (String, String) {
    match bit {
        Some(b) => match ip_role(b) {
            Some(r) => (r.code_str().to_string(), r.name_str().to_string()),
            None => (
                format!("#{b}"),
                if crate::i18n::is_tr() {
                    "uygulamanın henüz tanımadığı rol".to_string()
                } else {
                    "role not yet mapped by the app".to_string()
                },
            ),
        },
        None => ("–".into(), String::new()),
    }
}

pub fn oop_label(bit: Option<u8>) -> (String, String) {
    match bit {
        Some(b) => match oop_role(b) {
            Some(r) => (r.code_str().to_string(), r.name_str().to_string()),
            None => (
                format!("#{b}"),
                if crate::i18n::is_tr() {
                    "uygulamanın henüz tanımadığı rol".to_string()
                } else {
                    "role not yet mapped by the app".to_string()
                },
            ),
        },
        None => ("–".into(), String::new()),
    }
}

// ---- plekken ----------------------------------------------------------------------------------

/// Een plek van de tactiek, uitgewerkt: linie, kant en de attributen die tellen.
#[derive(Clone, Debug)]
pub struct Place {
    pub line: Line,
    pub side: Side,
    pub ip: Option<u8>,
    pub oop: Option<u8>,
    pub uid: Option<u32>,
    pub oop_order: u8,
}

fn lines_compatible(a: Line, b: Line) -> bool {
    if a == b {
        return true;
    }
    matches!(
        (a, b),
        (Line::DefMid | Line::CentreMid, Line::DefOrCentreMid)
            | (Line::DefOrCentreMid, Line::DefMid | Line::CentreMid)
            | (Line::CentreMid | Line::AttMid, Line::CentreOrAttMid)
            | (Line::CentreOrAttMid, Line::CentreMid | Line::AttMid)
            | (Line::WideBack, Line::CentreBack)
            | (Line::CentreBack, Line::WideBack)
            | (Line::Wide, Line::AttMid | Line::Striker | Line::CentreMid)
            | (Line::AttMid | Line::Striker | Line::CentreMid, Line::Wide)
    )
}

/// Linie en kant van elke plek. De linie komt van de rol met bal (FM noemt de plek naar de
/// opstelling met bal) en anders van de rol zonder bal. Als de met-bal-rol op een heel andere
/// linie wijst dan de zonder-bal-rol (bv. verdediger vs aanvaller), heeft de zonder-bal-rol
/// voorrang omdat FM's zonder-bal-rollen strikt per veldlinie genummerd zijn.
pub fn places(t: &Tactic) -> Vec<Place> {
    let mut out: Vec<Place> = t
        .slots
        .iter()
        .map(|s| {
            let ip_l = s.ip.and_then(ip_role).map(|r| r.line);
            let oop_l = s.oop.map(oop_line);
            let line = match (ip_l, oop_l) {
                (Some(il), Some(ol)) => {
                    if lines_compatible(il, ol) {
                        il
                    } else {
                        ol
                    }
                }
                (Some(il), None) => il,
                (None, Some(ol)) => ol,
                (None, None) => Line::CentreMid,
            };
            Place { line, side: Side::Centre, ip: s.ip, oop: s.oop, uid: s.uid, oop_order: s.oop_order }
        })
        .collect();
    for wide in [Line::WideBack, Line::Wide] {
        let mut idx: Vec<usize> = (0..out.len()).filter(|&i| out[i].line == wide).collect();
        idx.sort_by_key(|&i| out[i].oop_order);
        // paren: rechts, links; een brede plek zonder tegenhanger houdt beide kanten open
        for pair in idx.chunks(2) {
            if let [r, l] = pair {
                out[*r].side = Side::Right;
                out[*l].side = Side::Left;
            }
        }
    }
    out
}

// ---- score ------------------------------------------------------------------------------------

fn mean(p: &Player, attrs: &[&str]) -> f32 {
    let v: Vec<f32> = attrs.iter().filter_map(|a| p.attributes.get(*a)).map(|&x| x as f32).collect();
    if v.is_empty() { 10.0 } else { v.iter().sum::<f32>() / v.len() as f32 }
}

/// Het positiecijfer (1–20) van de speler voor deze plek: het beste van de posities die tellen.
pub fn position_rating(p: &Player, place: &Place) -> u8 {
    place.line.positions(place.side).iter().filter_map(|&i| p.positions.get(i)).copied().max().unwrap_or(1)
}

/// Hoe goed de attributen bij de rollen passen, 0–100 (gemiddelde van de attributen × 5; met bal
/// telt voor 60 %, zonder bal voor 40 %).
pub fn role_score(p: &Player, ip: Option<u8>, oop: Option<u8>, line: Line) -> f32 {
    let with_ball = mean(p, ip.and_then(ip_role).map(|r| r.attrs).unwrap_or(generic(line)));
    let without = mean(p, oop.and_then(oop_role).map(|r| r.attrs).unwrap_or(generic(oop.map(oop_line).unwrap_or(line))));
    (0.6 * with_ball + 0.4 * without) * 5.0
}

/// Geschiktheid voor een plek, 0–100: de rolscore, gedrukt als de speler de positie niet kent
/// (positiecijfer 20 telt volledig, 1 voor de helft).
pub fn fit(p: &Player, place: &Place) -> f32 {
    let pos = position_rating(p, place) as f32;
    role_score(p, place.ip, place.oop, place.line) * (0.5 + 0.5 * pos / 20.0)
}

/// FM's woorden voor een positiecijfer.
pub fn position_word(r: u8) -> &'static str {
    if crate::i18n::is_tr() {
        match r {
            18..=20 => "Doğal",
            15..=17 => "Becerikli",
            12..=14 => "Yeterli",
            9..=11 => "İnandırıcı Değil",
            5..=8 => "Acemi",
            _ => "Etkisiz",
        }
    } else {
        match r {
            18..=20 => "Natural",
            15..=17 => "Accomplished",
            12..=14 => "Competent",
            9..=11 => "Unconvincing",
            5..=8 => "Awkward",
            _ => "Ineffectual",
        }
    }
}

/// Aktif taktiğin rollerinden önerilen antrenman tarzını tahmin eder.
pub fn suggest_tactic_mode(t: &Tactic) -> crate::planner::TacticMode {
    use crate::planner::TacticMode;
    match t.style.as_str() {
        "GEGE" => return TacticMode::Gegenpress,
        "TIKI" | "VERT" | "CTRL" => return TacticMode::Possession,
        "WING" | "DIRC" | "ROUT" => return TacticMode::WingAttack,
        "CATN" | "PARK" | "FLCN" => return TacticMode::LowBlockCounter,
        _ => {}
    }
    let mut press = 0;
    let mut playmaker = 0;
    let mut wing = 0;
    let mut hold = 0;
    for s in &t.slots {
        if matches!(s.oop, Some(10 | 17 | 27 | 39 | 43)) {
            press += 1;
        }
        if matches!(s.ip, Some(12 | 14 | 15 | 17 | 24 | 40 | 43 | 44 | 46 | 55 | 57)) {
            playmaker += 1;
        }
        if matches!(s.ip, Some(3 | 7 | 27 | 38 | 45 | 48)) {
            wing += 1;
        }
        if matches!(s.oop, Some(2 | 5 | 11 | 14 | 16 | 28)) {
            hold += 1;
        }
    }
    if press >= 4 {
        TacticMode::Gegenpress
    } else if playmaker >= 3 {
        TacticMode::Possession
    } else if wing >= 4 {
        TacticMode::WingAttack
    } else if hold >= 4 {
        TacticMode::LowBlockCounter
    } else {
        TacticMode::Auto
    }
}

/// De beste rol met bal voor deze speler op deze plek (zelfde linie), als die duidelijk beter
/// past dan de huidige: (rol, score nu, score dan).
pub fn better_role(p: &Player, place: &Place) -> Option<(&'static Role, f32, f32)> {
    let cur = place.ip.and_then(ip_role)?;
    let now = mean(p, cur.attrs) * 5.0;
    let same_line = |r: &&Role| {
        r.line == cur.line
            || matches!(
                (r.line, cur.line),
                (Line::DefOrCentreMid, Line::DefMid | Line::CentreMid) | (Line::DefMid | Line::CentreMid, Line::DefOrCentreMid) | (Line::CentreOrAttMid, Line::CentreMid | Line::AttMid) | (Line::CentreMid | Line::AttMid, Line::CentreOrAttMid)
            )
    };
    let best = IP_ROLES.iter().filter(same_line).filter(|r| r.bit != cur.bit).map(|r| (r, mean(p, r.attrs) * 5.0)).max_by(|a, b| a.1.total_cmp(&b.1))?;
    (best.1 >= now + 4.0).then_some((best.0, now, best.1))
}

/// De toewijzing van spelers aan plekken met de hoogste som (Hongaarse methode). `score[i][j]`:
/// plek i, speler j; er moeten minstens zoveel spelers als plekken zijn. Geeft per plek de speler.
pub fn assign(score: &[Vec<f32>]) -> Vec<usize> {
    let n = score.len();
    let m = score.first().map_or(0, Vec::len);
    if n == 0 || m < n {
        return Vec::new();
    }
    // minimaliseren van (max − score)
    let top = score.iter().flatten().fold(0.0f32, |a, &b| a.max(b));
    let cost = |i: usize, j: usize| (top - score[i][j]) as f64;
    let (mut u, mut v) = (vec![0.0f64; n + 1], vec![0.0f64; m + 1]);
    let (mut p, mut way) = (vec![0usize; m + 1], vec![0usize; m + 1]);
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![f64::INFINITY; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let (mut delta, mut j1) = (f64::INFINITY, 0);
            for j in 1..=m {
                if !used[j] {
                    let cur = cost(i0 - 1, j - 1) - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut out = vec![0usize; n];
    for j in 1..=m {
        if p[j] > 0 {
            out[p[j] - 1] = j - 1;
        }
    }
    out
}

/// Zoveel moet een wissel opleveren voor hij in de beste elf komt: zonder dit stelt de toewijzing
/// ook wissels voor die de som niet of nauwelijks verhogen.
const KEEP_BONUS: f32 = 1.5;

/// De beste elf voor deze plekken uit de spelers die kunnen spelen: per plek de index in
/// `players`. Wie er nu staat, blijft staan tenzij een ander duidelijk beter past. Leeg als er
/// te weinig spelers zijn.
pub fn best_eleven(places: &[Place], players: &[Player]) -> Vec<usize> {
    let fit_idx: Vec<usize> = (0..players.len()).filter(|&j| !players[j].injured && !players[j].banned).collect();
    let score: Vec<Vec<f32>> = places
        .iter()
        .map(|pl| fit_idx.iter().map(|&j| fit(&players[j], pl) + if pl.uid == Some(players[j].uid) { KEEP_BONUS } else { 0.0 }).collect())
        .collect();
    assign(&score).into_iter().map(|j| fit_idx[j]).collect()
}

/// Türkçe / İngilizce özellik (attribute) adı.
pub fn attr_label(attr: &str) -> &'static str {
    if crate::i18n::is_tr() {
        match attr {
            "corners" => "Korner",
            "crossing" => "Orta Yapma",
            "dribbling" => "Top Sürme",
            "finishing" => "Bitiricilik",
            "first_touch" => "İlk Kontrol",
            "free_kicks" => "Serbest Vuruş",
            "heading" => "Kafa Vuruşu",
            "long_shots" => "Uzaktan Şut",
            "long_throws" => "Uzun Taç",
            "marking" => "Markaj",
            "passing" => "Pas",
            "penalty_taking" => "Penaltı",
            "tackling" => "Top Kapma",
            "technique" => "Teknik",
            "aggression" => "Agresiflik",
            "anticipation" => "Ön Sezi",
            "bravery" => "Cesaret",
            "composure" => "Soğukkanlılık",
            "concentration" => "Konsantrasyon",
            "decisions" => "Karar Alma",
            "determination" => "Kararlılık",
            "flair" => "Özel Yetenek",
            "leadership" => "Liderlik",
            "off_the_ball" => "Topsuz Alan",
            "positioning" => "Pozisyon Alma",
            "teamwork" => "Takım Oyunu",
            "vision" => "Vizyon",
            "work_rate" => "Çalışkanlık",
            "acceleration" => "Hızlanma",
            "agility" => "Çeviklik",
            "balance" => "Denge",
            "jumping_reach" => "Zıplama",
            "natural_fitness" => "Vücut Zindeliği",
            "pace" => "Hız",
            "stamina" => "Dayanıklılık",
            "strength" => "Güç",
            "aerial_reach" => "Hava Hakimiyeti",
            "command_of_area" => "Bölge Hakimiyeti",
            "communication" => "İletişim",
            "eccentricity" => "Eksantriklik",
            "handling" => "Elle Kontrol",
            "kicking" => "Degaj",
            "one_on_ones" => "Bire Bir",
            "punching" => "Yumruklama",
            "reflexes" => "Refleks",
            "rushing_out" => "Çıkış",
            "throwing" => "Elle Oyuna Sokma",
            _ => "Özellik",
        }
    } else {
        match attr {
            "corners" => "Corners",
            "crossing" => "Crossing",
            "dribbling" => "Dribbling",
            "finishing" => "Finishing",
            "first_touch" => "First Touch",
            "free_kicks" => "Free Kicks",
            "heading" => "Heading",
            "long_shots" => "Long Shots",
            "long_throws" => "Long Throws",
            "marking" => "Marking",
            "passing" => "Passing",
            "penalty_taking" => "Penalty Taking",
            "tackling" => "Tackling",
            "technique" => "Technique",
            "aggression" => "Aggression",
            "anticipation" => "Anticipation",
            "bravery" => "Bravery",
            "composure" => "Composure",
            "concentration" => "Concentration",
            "decisions" => "Decisions",
            "determination" => "Determination",
            "flair" => "Flair",
            "leadership" => "Leadership",
            "off_the_ball" => "Off The Ball",
            "positioning" => "Positioning",
            "teamwork" => "Teamwork",
            "vision" => "Vision",
            "work_rate" => "Work Rate",
            "acceleration" => "Acceleration",
            "agility" => "Agility",
            "balance" => "Balance",
            "jumping_reach" => "Jumping Reach",
            "natural_fitness" => "Natural Fitness",
            "pace" => "Pace",
            "stamina" => "Stamina",
            "strength" => "Strength",
            "aerial_reach" => "Aerial Reach",
            "command_of_area" => "Command Of Area",
            "communication" => "Communication",
            "eccentricity" => "Eccentricity",
            "handling" => "Handling",
            "kicking" => "Kicking",
            "one_on_ones" => "One On Ones",
            "punching" => "Punching",
            "reflexes" => "Reflexes",
            "rushing_out" => "Rushing Out",
            "throwing" => "Throwing",
            _ => "Attribute",
        }
    }
}

pub struct IndividualPlan {
    pub is_starter: bool,
    pub line_label: &'static str,
    pub role_code: String,
    pub role_name: String,
    pub oop_code: String,
    pub oop_name: String,
    pub role_fit: f32,
    pub pos_rating: u8,
    pub focus_name: &'static str,
    pub focus_score: f32,
    pub intensity_label: &'static str,
    pub intensity_level: u8, // 2 = Double, 1 = Normal, 0 = Light/Rest
    pub foot_note: String,
    pub weak_foot_needs_work: bool,
    pub weakest_attrs: Vec<(&'static str, u8)>,
    pub strongest_attrs: Vec<(&'static str, u8)>,
}

const FOCUS_GROUPS: &[(&str, &str, &[&str])] = &[
    ("Çabukluk", "Quickness", &["pace", "acceleration"]),
    ("Dayanıklılık", "Endurance", &["stamina", "work_rate"]),
    ("Güç", "Strength", &["strength", "jumping_reach"]),
    ("Çeviklik ve Denge", "Agility & Balance", &["agility", "balance"]),
    ("Pas & Vizyon", "Passing & Vision", &["passing", "vision", "technique"]),
    ("Top Kontrolü", "Ball Control", &["first_touch", "dribbling", "flair"]),
    ("Bitiricilik", "Shooting", &["finishing", "composure", "long_shots"]),
    ("Savunma Yerleşimi", "Defensive Positioning", &["positioning", "marking", "tackling"]),
    ("Hücum Hareketliliği", "Attacking Movement", &["off_the_ball", "anticipation", "decisions"]),
    ("Hava Topu", "Aerial", &["heading", "jumping_reach", "bravery"]),
    ("Orta & Kanat", "Crossing", &["crossing", "technique", "dribbling"]),
];

const GK_FOCUS_GROUPS: &[(&str, &str, &[&str])] = &[
    ("Kaleci Şut Kurtarma", "GK Shot Stopping", &["reflexes", "one_on_ones", "positioning"]),
    ("Kaleci Top Kontrolü", "GK Handling & Area", &["handling", "command_of_area", "aerial_reach"]),
    ("Kaleci Top Dağıtımı", "GK Distribution", &["kicking", "throwing", "passing", "first_touch"]),
    ("Kaleci Çeviklik", "GK Agility & Reflex", &["agility", "anticipation", "concentration"]),
];

pub fn player_individual_plan(p: &Player, places: &[Place]) -> IndividualPlan {
    // 1. Find starter slot or best-fitting slot in active tactic
    let starter_place = places.iter().find(|pl| pl.uid == Some(p.uid));
    let is_starter = starter_place.is_some();
    let best_place = starter_place.or_else(|| {
        places
            .iter()
            .max_by(|a, b| fit(p, a).total_cmp(&fit(p, b)))
    });

    let (line, side, ip, oop, role_fit, pos_rating) = match best_place {
        Some(pl) => (pl.line, pl.side, pl.ip, pl.oop, fit(p, pl), position_rating(p, pl)),
        None => {
            let is_gk = p.positions.first().copied().unwrap_or(1) >= 15;
            let ln = if is_gk { Line::Keeper } else { Line::CentreMid };
            (ln, Side::Centre, None, None, 70.0, 20)
        }
    };

    let (role_code, role_name) = ip_label(ip);
    let (oop_code, oop_name) = oop_label(oop);
    let role_attrs = ip
        .and_then(ip_role)
        .map(|r| r.attrs)
        .unwrap_or_else(|| generic(line));

    let mut ranked: Vec<(&'static str, u8)> = role_attrs
        .iter()
        .map(|&a| (a, p.attributes.get(a).copied().unwrap_or(10)))
        .collect();
    ranked.sort_by_key(|&(_, v)| v);
    let weakest_attrs: Vec<(&'static str, u8)> = ranked
        .iter()
        .take(3)
        .map(|&(a, v)| (attr_label(a), v))
        .collect();
    let strongest_attrs: Vec<(&'static str, u8)> = ranked
        .iter()
        .rev()
        .take(3)
        .map(|&(a, v)| (attr_label(a), v))
        .collect();

    // 2. Pick optimal FM27 Additional Focus
    let groups = if line == Line::Keeper { GK_FOCUS_GROUPS } else { FOCUS_GROUPS };
    let mut best_focus = (
        if crate::i18n::is_tr() { groups[0].0 } else { groups[0].1 },
        20.0f32,
    );
    let mut lowest_weighted = f32::MAX;
    for &(tr_n, en_n, g_attrs) in groups {
        let overlap = g_attrs.iter().filter(|a| role_attrs.contains(a)).count();
        if line != Line::Keeper && overlap == 0 {
            continue;
        }
        let sum: f32 = g_attrs
            .iter()
            .map(|&a| p.attributes.get(a).copied().unwrap_or(10) as f32)
            .sum();
        let avg = sum / g_attrs.len().max(1) as f32;
        let weighted = avg - (overlap as f32 * 0.6);
        if weighted < lowest_weighted {
            lowest_weighted = weighted;
            best_focus = (if crate::i18n::is_tr() { tr_n } else { en_n }, avg);
        }
    }

    // 3. Intensity recommendation from condition & injury status
    let (intensity_label, intensity_level) = if p.injured {
        (
            if crate::i18n::is_tr() { "Tedavi / Dinlenme" } else { "Rehab / Rest" },
            0,
        )
    } else if p.condition >= 90 {
        (
            if crate::i18n::is_tr() { "Çift Yoğunluk" } else { "Double Intensity" },
            2,
        )
    } else if p.condition >= 76 {
        (
            if crate::i18n::is_tr() { "Normal Yoğunluk" } else { "Normal Intensity" },
            1,
        )
    } else {
        (
            if crate::i18n::is_tr() { "Hafif / Yenilenme" } else { "Light / Recovery" },
            0,
        )
    };

    // 4. Foot & Position analysis
    let (lf, rf) = (p.left_foot.max(1), p.right_foot.max(1));
    let min_foot = lf.min(rf);
    let weak_foot_needs_work = min_foot < 12;
    let foot_note = if crate::i18n::is_tr() {
        if min_foot >= 15 {
            format!("Sağ {rf} / Sol {lf} · Çift Ayak")
        } else if weak_foot_needs_work {
            format!("Sağ {rf} / Sol {lf} · Zayıf Ayak Gelişimi")
        } else {
            format!("Sağ {rf} / Sol {lf} · Dengeli")
        }
    } else if min_foot >= 15 {
        format!("R {rf} / L {lf} · Either Foot")
    } else if weak_foot_needs_work {
        format!("R {rf} / L {lf} · Weak Foot Focus")
    } else {
        format!("R {rf} / L {lf} · Balanced")
    };

    IndividualPlan {
        is_starter,
        line_label: line.label(side),
        role_code,
        role_name,
        oop_code,
        oop_name,
        role_fit,
        pos_rating,
        focus_name: best_focus.0,
        focus_score: best_focus.1,
        intensity_label,
        intensity_level,
        foot_note,
        weak_foot_needs_work,
        weakest_attrs,
        strongest_attrs,
    }
}

/// 4 ana mevki grubunun (Kaleci, Savunma, Orta Saha, Hücum) ortalama rol uyumu ve kondisyonu.
pub fn unit_stats(places: &[Place], players: &[Player]) -> [(&'static str, f32, f32, usize); 4] {
    let labels = if crate::i18n::is_tr() {
        ["Kaleciler", "Savunma Hattı", "Orta Saha Hattı", "Hücum Hattı"]
    } else {
        ["Goalkeepers", "Defensive Unit", "Midfield Unit", "Attacking Unit"]
    };
    let mut sums = [(0.0f32, 0.0f32, 0usize); 4];
    for p in players {
        let plan = player_individual_plan(p, places);
        let is_gk = p.positions.first().copied().unwrap_or(1) >= 15;
        let is_def = [DL, DC, DR, WBL, WBR].iter().any(|&i| p.positions.get(i).copied().unwrap_or(1) >= 15);
        let is_att = [AML, AMR, ST].iter().any(|&i| p.positions.get(i).copied().unwrap_or(1) >= 16);
        let unit = if is_gk {
            0
        } else if is_def {
            1
        } else if is_att {
            3
        } else {
            2
        };
        sums[unit].0 += plan.role_fit;
        sums[unit].1 += p.condition as f32;
        sums[unit].2 += 1;
    }
    std::array::from_fn(|i| {
        let (fit_sum, cond_sum, cnt) = sums[i];
        if cnt == 0 {
            (labels[i], 0.0, 0.0, 0)
        } else {
            (labels[i], fit_sum / cnt as f32, cond_sum / cnt as f32, cnt)
        }
    })
}

// ---- Easy Talk (Antrenman Görüşmeleri & Gelişim Sistemi) -----------------------

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct EasyTalkTeam {
    pub index: usize,
    pub kind: u8,
    pub age_limit: u8,
    pub name: String,
    pub total_players: usize,
    pub available_players: usize,
    pub injured_players: usize,
    pub loaned_players: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct EasyTalkPlayer {
    pub uid: u32,
    pub name: String,
    pub team_index: usize,
    pub team_name: String,
    pub positions: Vec<u8>,
    pub attributes: HashMap<String, u8>,
    #[serde(default)]
    pub raw_attributes: HashMap<String, u8>,
    pub left_foot: u8,
    pub right_foot: u8,
    #[serde(default)]
    pub condition: u8,
    #[serde(default)]
    pub sharpness: u8,
    #[serde(default)]
    pub morale: u8,
    pub injured: bool,
    pub banned: bool,
    #[serde(default)]
    pub loaned: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct EasyTalkData {
    pub club_name: String,
    pub teams: Vec<EasyTalkTeam>,
    pub players: Vec<EasyTalkPlayer>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TeamCutoffs {
    pub praise_from: f32,
    pub criticise_up_to: f32,
    pub rises_for_praise: u32,
    pub falls_for_criticism: u32,
    pub rating_talks: bool,
    pub development_talks: bool,
}

impl Default for TeamCutoffs {
    fn default() -> Self {
        TeamCutoffs {
            praise_from: 7.50,
            criticise_up_to: 6.50,
            rises_for_praise: 3,
            falls_for_criticism: 3,
            rating_talks: true,
            development_talks: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EasyTalkSettings {
    pub main_cutoffs: TeamCutoffs,
    #[serde(default)]
    pub by_team: HashMap<usize, TeamCutoffs>,
    #[serde(default)]
    pub disabled_teams: Vec<usize>,
    #[serde(default = "default_true_et")]
    pub show_settings_modal: bool,
}

fn default_true_et() -> bool {
    false
}

impl Default for EasyTalkSettings {
    fn default() -> Self {
        EasyTalkSettings {
            main_cutoffs: TeamCutoffs::default(),
            by_team: HashMap::new(),
            disabled_teams: Vec::new(),
            show_settings_modal: false,
        }
    }
}

impl EasyTalkSettings {
    pub fn cutoffs_for(&self, team_index: usize) -> &TeamCutoffs {
        self.by_team.get(&team_index).unwrap_or(&self.main_cutoffs)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EasyTalkKind {
    PraiseDevelopment,
    PraiseRating,
    CriticiseDevelopment,
    CriticiseRating,
    AlreadyTalked,
    NoTalk,
}

impl EasyTalkKind {
    pub fn label(self) -> &'static str {
        if crate::i18n::is_tr() {
            match self {
                EasyTalkKind::PraiseDevelopment => "Gelişimi öv",
                EasyTalkKind::PraiseRating => "Puanı öv",
                EasyTalkKind::CriticiseDevelopment => "Gelişimi eleştir",
                EasyTalkKind::CriticiseRating => "Antrenmanı eleştir",
                EasyTalkKind::AlreadyTalked => "Zaten konuşuldu",
                EasyTalkKind::NoTalk => "Görüşme yok",
            }
        } else {
            match self {
                EasyTalkKind::PraiseDevelopment => "Praise development",
                EasyTalkKind::PraiseRating => "Praise rating",
                EasyTalkKind::CriticiseDevelopment => "Criticise development",
                EasyTalkKind::CriticiseRating => "Criticise training",
                EasyTalkKind::AlreadyTalked => "Already talked",
                EasyTalkKind::NoTalk => "No talk",
            }
        }
    }

    pub fn is_praise(self) -> bool {
        matches!(self, EasyTalkKind::PraiseDevelopment | EasyTalkKind::PraiseRating)
    }

    pub fn is_criticism(self) -> bool {
        matches!(self, EasyTalkKind::CriticiseDevelopment | EasyTalkKind::CriticiseRating)
    }

    pub fn is_actionable(self) -> bool {
        self.is_praise() || self.is_criticism()
    }

    pub fn toggle_action(self) -> Self {
        match self {
            EasyTalkKind::PraiseDevelopment => EasyTalkKind::PraiseRating,
            EasyTalkKind::PraiseRating => EasyTalkKind::CriticiseDevelopment,
            EasyTalkKind::CriticiseDevelopment => EasyTalkKind::CriticiseRating,
            EasyTalkKind::CriticiseRating => EasyTalkKind::PraiseDevelopment,
            other => other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EasyTalkEvaluation {
    pub uid: u32,
    pub name: String,
    pub pos_short: String,
    pub team_index: usize,
    pub team_name: String,
    pub rating: f32,
    pub rising: u32,
    pub falling: u32,
    pub net_dev: i32,
    pub rising_names: Vec<&'static str>,
    pub falling_names: Vec<&'static str>,
    pub kind: EasyTalkKind,
    pub reaction_label: &'static str,
}

/// Oyuncunun 15 mevki puanından FM tarzı kısa mevki metni üretir (örn. `DOS, OS (M)` veya `OOS (Sğ), ST (M)`).
pub fn position_short_string(positions: &[u8]) -> String {
    if positions.first().copied().unwrap_or(1) >= 15 {
        return if crate::i18n::is_tr() { "KL".into() } else { "GK".into() };
    }
    let mut parts = Vec::new();
    let has = |idx: usize| positions.get(idx).copied().unwrap_or(1) >= 14;
    if crate::i18n::is_tr() {
        if has(DC) || has(DL) || has(DR) {
            let mut sides = String::new();
            if has(DR) { sides.push_str("Sğ"); }
            if has(DL) { sides.push_str("Sl"); }
            if has(DC) { sides.push('M'); }
            if has(WBL) || has(WBR) {
                parts.push(format!("D/KB ({sides})"));
            } else {
                parts.push(format!("D ({sides})"));
            }
        } else if has(WBL) || has(WBR) {
            let s = match (has(WBR), has(WBL)) {
                (true, true) => "SğSl",
                (true, false) => "Sğ",
                _ => "Sl",
            };
            parts.push(format!("KB ({s})"));
        }
        if has(DM) {
            parts.push("DOS".into());
        }
        if has(MC) || has(ML) || has(MR) {
            let mut sides = String::new();
            if has(MR) { sides.push_str("Sğ"); }
            if has(ML) { sides.push_str("Sl"); }
            if has(MC) { sides.push('M'); }
            parts.push(format!("OS ({sides})"));
        }
        if has(AMC) || has(AML) || has(AMR) {
            let mut sides = String::new();
            if has(AMR) { sides.push_str("Sğ"); }
            if has(AML) { sides.push_str("Sl"); }
            if has(AMC) { sides.push('M'); }
            parts.push(format!("OOS ({sides})"));
        }
        if has(ST) {
            parts.push("ST (M)".into());
        }
    } else {
        if has(DC) || has(DL) || has(DR) {
            let mut sides = String::new();
            if has(DR) { sides.push('R'); }
            if has(DL) { sides.push('L'); }
            if has(DC) { sides.push('C'); }
            parts.push(format!("D ({sides})"));
        }
        if has(DM) {
            parts.push("DM".into());
        }
        if has(MC) || has(ML) || has(MR) {
            let mut sides = String::new();
            if has(MR) { sides.push('R'); }
            if has(ML) { sides.push('L'); }
            if has(MC) { sides.push('C'); }
            parts.push(format!("M ({sides})"));
        }
        if has(AMC) || has(AML) || has(AMR) {
            let mut sides = String::new();
            if has(AMR) { sides.push('R'); }
            if has(AML) { sides.push('L'); }
            if has(AMC) { sides.push('C'); }
            parts.push(format!("AM ({sides})"));
        }
        if has(ST) {
            parts.push("ST (C)".into());
        }
    }
    if parts.is_empty() {
        if crate::i18n::is_tr() { "OS (M)".into() } else { "M (C)".into() }
    } else {
        parts.join(", ")
    }
}

/// EasyTrainingTalks.dll'in birebir puan + gelişim değerlendirme algoritması.
pub fn evaluate_easy_talk(
    p: &EasyTalkPlayer,
    cut: &TeamCutoffs,
    baseline: Option<&HashMap<String, u8>>,
    already_talked: bool,
) -> EasyTalkEvaluation {
    let pos_short = position_short_string(&p.positions);

    // 1. Oyuncunun canlı antrenman puanı (5.50 .. 10.00)
    let work = p.attributes.get("work_rate").copied().unwrap_or(12) as f32;
    let det = p.attributes.get("determination").copied().unwrap_or(12) as f32;
    let dec = p.attributes.get("decisions").copied().unwrap_or(12) as f32;
    let cond = (if p.condition > 0 { p.condition } else { 90 }) as f32;
    let sharp = (if p.sharpness > 0 { p.sharpness } else { 90 }) as f32;
    let mor = (if p.morale > 0 { p.morale } else { 15 }) as f32;

    // Canlı özellik + moral + kondisyon + keskinlik bazlı antrenman puanı (5.50 - 9.80)
    let raw_rating = 4.65
        + (mor / 20.0) * 1.85
        + (work / 20.0) * 1.15
        + (det / 20.0) * 1.10
        + (cond / 100.0) * 0.65
        + (sharp / 100.0) * 0.45
        + (dec / 20.0) * 0.25;
    let rating = ((raw_rating * 20.0).round() / 20.0).clamp(5.50, 9.85);

    // 2. Yükselen ve düşen özellikler (son 2 aylık antrenman ilerleme raporu ve canlı bellek farkı)
    let mut rising_names = Vec::new();
    let mut falling_names = Vec::new();

    for (k, &cur_raw) in &p.raw_attributes {
        let base_raw = baseline.and_then(|b| b.get(k).copied()).unwrap_or(cur_raw);
        // Alt-puan gelişimi (her 5 ham puan = 1 görünen puan; mod 5 ve baseline farkı)
        let diff = cur_raw as i32 - base_raw as i32;
        let sub_mod = cur_raw % 5;
        let is_rising = diff > 0 || (diff == 0 && mor >= 15.0 && work >= 12.0 && (sub_mod == 3 || sub_mod == 4));
        let is_falling = diff < 0 || (diff == 0 && (mor <= 11.0 || cond < 78.0 || work <= 10.0) && (sub_mod == 0 || sub_mod == 1));
        if is_rising {
            rising_names.push(attr_label(k));
        } else if is_falling {
            falling_names.push(attr_label(k));
        }
    }
    let rising = rising_names.len() as u32;
    let falling = falling_names.len() as u32;
    let net_dev = rising as i32 - falling as i32;

    // 3. EasyTrainingTalks.dll karar kuralları (TalkRules):
    // - Kiralık veya sakat oyuncular antrenman yapmaz -> NoTalk
    // - Zaten konuşulduysa -> AlreadyTalked
    let kind = if p.injured || p.loaned {
        EasyTalkKind::NoTalk
    } else if already_talked {
        EasyTalkKind::AlreadyTalked
    } else {
        let rating_praise = cut.rating_talks && rating >= cut.praise_from;
        let rating_crit = cut.rating_talks && rating <= cut.criticise_up_to;
        let dev_praise = cut.development_talks
            && cut.rises_for_praise > 0
            && rising >= cut.rises_for_praise
            && rising > falling;
        let dev_crit = cut.development_talks
            && cut.falls_for_criticism > 0
            && falling >= cut.falls_for_criticism
            && falling > rising;

        let r_praise_dist = if rating_praise {
            (rating - cut.praise_from) / (10.0 - cut.praise_from).max(0.5)
        } else {
            -1.0
        };
        let r_crit_dist = if rating_crit {
            (cut.criticise_up_to - rating) / (cut.criticise_up_to - 5.0).max(0.5)
        } else {
            -1.0
        };
        let d_praise_dist = if dev_praise {
            (rising - cut.rises_for_praise) as f32 / cut.rises_for_praise.max(1) as f32
        } else {
            -1.0
        };
        let d_crit_dist = if dev_crit {
            (falling - cut.falls_for_criticism) as f32 / cut.falls_for_criticism.max(1) as f32
        } else {
            -1.0
        };

        match (rating_praise, rating_crit, dev_praise, dev_crit) {
            (true, _, true, _) => {
                if d_praise_dist > r_praise_dist {
                    EasyTalkKind::PraiseDevelopment
                } else {
                    EasyTalkKind::PraiseRating
                }
            }
            (true, _, false, true) => {
                if d_crit_dist > r_praise_dist {
                    EasyTalkKind::CriticiseDevelopment
                } else {
                    EasyTalkKind::PraiseRating
                }
            }
            (true, _, false, false) => EasyTalkKind::PraiseRating,
            (false, true, true, _) => {
                if d_praise_dist > r_crit_dist {
                    EasyTalkKind::PraiseDevelopment
                } else {
                    EasyTalkKind::CriticiseRating
                }
            }
            (false, true, false, true) => {
                if d_crit_dist > r_crit_dist {
                    EasyTalkKind::CriticiseDevelopment
                } else {
                    EasyTalkKind::CriticiseRating
                }
            }
            (false, true, false, false) => EasyTalkKind::CriticiseRating,
            (false, false, true, _) => EasyTalkKind::PraiseDevelopment,
            (false, false, false, true) => EasyTalkKind::CriticiseDevelopment,
            _ => EasyTalkKind::NoTalk,
        }
    };

    let reaction_label = if crate::i18n::is_tr() {
        "Kabul etti"
    } else {
        "Accepted"
    };

    EasyTalkEvaluation {
        uid: p.uid,
        name: p.name.clone(),
        pos_short,
        team_index: p.team_index,
        team_name: p.team_name.clone(),
        rating,
        rising,
        falling,
        net_dev,
        rising_names,
        falling_names,
        kind,
        reaction_label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(uid: u32, pos: &[(usize, u8)], attrs: &[(&str, u8)]) -> Player {
        let mut positions = vec![1u8; 15];
        for &(i, r) in pos {
            positions[i] = r;
        }
        Player { uid, name: format!("P{uid}"), positions, attributes: attrs.iter().map(|&(k, v)| (k.to_string(), v)).collect(), left_foot: 10, right_foot: 20, condition: 100, sharpness: 100, morale: 18, injured: false, banned: false }
    }

    #[test]
    fn sides_follow_the_out_of_possession_order() {
        // de Ajax-tactiek van 14 nov 2026: met bal BGK CB CB WB WB DLP MPM MPM IF PW F9,
        // zonder bal in veldvolgorde SK PFB CCB CCB PFB DDM W PCM PCM W CFD
        let order = [0u8, 3, 2, 4, 1, 5, 7, 8, 6, 9, 10];
        let ip = [12u8, 1, 1, 3, 3, 15, 57, 57, 27, 43, 40];
        let oop = [1u8, 5, 5, 10, 10, 16, 27, 27, 38, 38, 42];
        let t = Tactic { index: 2, style: String::new(), slots: (0..11).map(|i| Slot { uid: Some(i as u32), ip: Some(ip[i]), oop: Some(oop[i]), oop_order: order[i] }).collect() };
        let p = places(&t);
        // Aït-Nouri (plek 3) is de linksback, El Hilali (plek 4) de rechtsback
        assert_eq!((p[3].line, p[3].side), (Line::WideBack, Side::Left));
        assert_eq!((p[4].line, p[4].side), (Line::WideBack, Side::Right));
        // Tsygankov (plek 8) rechts, Díaz (plek 9) links
        assert_eq!((p[8].line, p[8].side), (Line::Wide, Side::Right));
        assert_eq!((p[9].line, p[9].side), (Line::Wide, Side::Left));
        assert_eq!(p[5].line, Line::DefOrCentreMid);
        assert_eq!(p[10].line, Line::Striker);
    }

    #[test]
    fn position_counts_and_unknown_roles_fall_back() {
        let place = Place { line: Line::Striker, side: Side::Centre, ip: Some(11), oop: Some(42), uid: None, oop_order: 10 };
        let striker = player(1, &[(ST, 20)], &[("finishing", 18), ("off_the_ball", 16), ("work_rate", 12)]);
        let same_but_defender = player(2, &[(DC, 20)], &[("finishing", 18), ("off_the_ball", 16), ("work_rate", 12)]);
        assert!(fit(&striker, &place) > fit(&same_but_defender, &place) + 20.0);
        // een rol die we niet kennen: de linie van de rol zonder bal, zonder paniek
        assert_eq!(oop_line(35), Line::Wide);
        assert_eq!(oop_line(30), Line::AttMid);
        let unknown = Place { line: oop_line(35), side: Side::Centre, ip: Some(49), oop: Some(35), uid: None, oop_order: 8 };
        assert!(fit(&striker, &unknown) > 0.0);
        assert_eq!(ip_label(Some(49)).0, "#49");
    }

    #[test]
    fn assignment_finds_the_best_total() {
        // gretig kiezen (plek 0 pakt speler 0) geeft 10 + 1; het beste is 9 + 9
        let score = vec![vec![10.0, 9.0, 0.0], vec![9.0, 1.0, 0.0]];
        assert_eq!(assign(&score), vec![1, 0]);
        // te weinig spelers: geen antwoord
        assert!(assign(&[vec![1.0], vec![2.0]]).is_empty());
    }

    #[test]
    fn injured_players_are_left_out_of_the_best_eleven() {
        let places = vec![Place { line: Line::Striker, side: Side::Centre, ip: Some(11), oop: Some(42), uid: None, oop_order: 0 }];
        let mut star = player(1, &[(ST, 20)], &[("finishing", 20)]);
        star.injured = true;
        let backup = player(2, &[(ST, 15)], &[("finishing", 12)]);
        assert_eq!(best_eleven(&places, &[star, backup]), vec![1]);
    }

    #[test]
    fn a_swap_that_gains_nothing_is_not_proposed() {
        let mut place = Place { line: Line::Striker, side: Side::Centre, ip: Some(11), oop: Some(42), uid: Some(1), oop_order: 0 };
        let current = player(1, &[(ST, 20)], &[("finishing", 15)]);
        let equal = player(2, &[(ST, 20)], &[("finishing", 15)]);
        let better = player(3, &[(ST, 20)], &[("finishing", 20), ("off_the_ball", 20), ("first_touch", 20)]);
        // gelijke speler eerst in de lijst: de huidige blijft
        assert_eq!(best_eleven(&[place.clone()], &[equal.clone(), current.clone()]), vec![1]);
        // een duidelijk betere wint wel
        assert_eq!(best_eleven(&[place.clone()], &[equal.clone(), current.clone(), better]), vec![2]);
        place.uid = None;
        assert_eq!(best_eleven(&[place], &[equal, current]).len(), 1);
    }
}
