//! Dil desteği: Türkçe (varsayılan) ve İngilizce (Ayarlar'dan değiştirilebilir).

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    #[serde(alias = "Nl", alias = "Other")]
    Tr,
    En,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(l: Lang) {
    let v = match l {
        Lang::Tr => 0,
        Lang::En => 1,
    };
    CURRENT.store(v, Ordering::Relaxed);
}

pub fn get() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::Tr,
    }
}

pub fn is_tr() -> bool {
    get() == Lang::Tr
}

pub fn detect() -> Lang {
    Lang::Tr
}

#[macro_export]
macro_rules! t {
    ($tr:expr $(,)?) => {
        $tr
    };
    ($tr:expr, $en:expr $(,)?) => {
        if $crate::i18n::is_tr() { $tr } else { $en }
    };
    ($nl:expr, $en:expr, $tr:expr $(,)?) => {
        if $crate::i18n::is_tr() { $tr } else { $en }
    };
}

pub fn month_short(m: u32) -> &'static str {
    const TR: [&str; 12] = ["Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara"];
    const EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let i = (m.clamp(1, 12) - 1) as usize;
    if is_tr() { TR[i] } else { EN[i] }
}

pub fn weekday_short(i: usize) -> &'static str {
    const TR: [&str; 7] = ["Pzt", "Sal", "Çar", "Per", "Cum", "Cmt", "Paz"];
    const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    if is_tr() { TR[i % 7] } else { EN[i % 7] }
}

pub fn weekday_initial(i: usize) -> &'static str {
    const TR: [&str; 7] = ["P", "S", "Ç", "P", "C", "C", "P"];
    const EN: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];
    if is_tr() { TR[i % 7] } else { EN[i % 7] }
}

pub fn date(d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    format!("{} {} {}", d.day(), month_short(d.month()), d.year())
}

pub fn date_short(d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    format!("{} {}", d.day(), month_short(d.month()))
}
