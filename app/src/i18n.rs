//! Yalnızca Türkçe (TR). Tüm arayüz ve FM terimleri Türkçe olarak gösterilir.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Tr,
    #[serde(alias = "Nl", alias = "En")]
    Other,
}

pub fn set(_l: Lang) {}

pub fn get() -> Lang {
    Lang::Tr
}

pub fn detect() -> Lang {
    Lang::Tr
}

#[macro_export]
macro_rules! t {
    ($tr:expr $(,)?) => {
        $tr
    };
    ($nl:expr, $en:expr $(,)?) => {
        $en
    };
    ($nl:expr, $en:expr, $tr:expr $(,)?) => {
        $tr
    };
}

/// Türkçe kısa ay adları.
pub fn month_short(m: u32) -> &'static str {
    const TR: [&str; 12] = ["Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara"];
    let i = (m.clamp(1, 12) - 1) as usize;
    TR[i]
}

pub fn weekday_short(i: usize) -> &'static str {
    const TR: [&str; 7] = ["Pzt", "Sal", "Çar", "Per", "Cum", "Cmt", "Paz"];
    TR[i % 7]
}

pub fn weekday_initial(i: usize) -> &'static str {
    const TR: [&str; 7] = ["P", "S", "Ç", "P", "C", "C", "P"];
    TR[i % 7]
}

pub fn date(d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    format!("{} {} {}", d.day(), month_short(d.month()), d.year())
}

pub fn date_short(d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    format!("{} {}", d.day(), month_short(d.month()))
}
