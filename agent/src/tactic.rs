//! De tactieken van de menselijke manager en wat de tactiekhulp van de app nodig heeft: per
//! tactiek de elf plekken met hun rol met en zonder bal, en van elke speler van de selectie de
//! attributen en de posities. Alleen lezen.
//!
//! Feiten uit FMLibreEditor (`profiles/fm27/layout*.toml`, `docs/recon/fm27-tactics-and-value.md`),
//! op de Mac nagekeken tegen FM's tactiekscherm (Steam-build 26577676, Ajax-save, 14 nov 2026):
//!
//! - `simatch::PREPARED_TACTICS_CREATOR_ARRAY`: `+0x08` lijst van hoogstens drie tactiekblokken,
//!   `+0x20` i8 de tactiek in gebruik, `+0x28` 26 persoons-id's (basiself, dan de bank).
//! - Tactiekblok (0x800 bytes): `+0x100` u32 stijl als tekst achterstevoren (`GEGE`, `NULL` = eigen),
//!   elf plekken met bal en elf zonder bal, 0x48 bytes per plek, met het rolmasker (u64) op
//!   Windows vanaf `+0x158` / `+0x478`, op de Mac 8 verder. Een masker heeft één bit uit 21–23 en
//!   één bit dat de rol noemt. De plekken met bal staan in de volgorde van de opstelling; de
//!   plekken zonder bal in veldvolgorde, met een tabel (na een byte 0x1B, rond `+0x7A0`) die
//!   zegt welke plek zonder bal bij een plek met bal hoort.
//! - Speler: 15 positiecijfers (1–20: GK, SW, DL, DC, DR, DM, ML, MC, MR, AML, AMC, AMR, ST, WBL,
//!   WBR) en de attributen als u8 0–100 (in FM getoond als (x + 2) / 5).

use serde::Serialize;
use std::collections::HashMap;

use crate::game::{self, PlayerInfo};
use crate::{mem, rtti};

const SEL_LIST: u64 = 0x08;
const SEL_CURRENT: u64 = 0x20;
const SEL_SLOTS: u64 = 0x28;
const SEL_COUNT: usize = 26;
const EMPTY_SLOT: u32 = u32::MAX;
const BLOCK: usize = 0x800;
const STYLE: usize = 0x100;
const SLOT: usize = 0x48;
const SLOTS: usize = 11;
/// de bits die elk masker naast de rol heeft
const CLASS_BITS: u64 = 0b111 << 21;

/// Attributen: (naam, plek op de Mac). Op Windows ligt alles 8 verder (`Layout.p_positions`).
pub const ATTRIBUTES: [(&str, u64); 47] = [
    ("corners", 0x172),
    ("crossing", 0x157),
    ("dribbling", 0x158),
    ("finishing", 0x159),
    ("first_touch", 0x16D),
    ("free_kicks", 0x17A),
    ("heading", 0x15A),
    ("long_shots", 0x15B),
    ("long_throws", 0x175),
    ("marking", 0x15C),
    ("passing", 0x15E),
    ("penalty_taking", 0x15F),
    ("tackling", 0x160),
    ("technique", 0x16E),
    ("aggression", 0x184),
    ("anticipation", 0x168),
    ("bravery", 0x182),
    ("composure", 0x18B),
    ("concentration", 0x18C),
    ("decisions", 0x169),
    ("determination", 0x18A),
    ("flair", 0x171),
    ("leadership", 0x17F),
    ("off_the_ball", 0x15D),
    ("positioning", 0x16B),
    ("teamwork", 0x173),
    ("vision", 0x161),
    ("work_rate", 0x174),
    ("acceleration", 0x179),
    ("agility", 0x185),
    ("balance", 0x181),
    ("jumping_reach", 0x17E),
    ("natural_fitness", 0x189),
    ("pace", 0x17D),
    ("stamina", 0x17C),
    ("strength", 0x17B),
    ("aerial_reach", 0x163),
    ("command_of_area", 0x164),
    ("communication", 0x165),
    ("eccentricity", 0x176),
    ("handling", 0x162),
    ("kicking", 0x166),
    ("one_on_ones", 0x16A),
    ("punching", 0x178),
    ("reflexes", 0x16C),
    ("rushing_out", 0x177),
    ("throwing", 0x167),
];
const LEFT_FOOT: u64 = 0x16F;
const RIGHT_FOOT: u64 = 0x170;
const MAC_POSITIONS: u64 = 0x148;

#[derive(Serialize, Clone, Debug)]
pub struct Slot {
    /// uid van de speler op deze plek (None: leeg, of niet van de selectie)
    pub uid: Option<u32>,
    /// het bit dat de rol met bal noemt (None: geen rol leesbaar)
    pub ip: Option<u8>,
    /// het bit dat de rol zonder bal noemt
    pub oop: Option<u8>,
    /// plaats van deze plek in FM's veldvolgorde zonder bal (0 = keeper)
    pub oop_order: u8,
}

#[derive(Serialize, Clone, Debug)]
pub struct Tactic {
    pub index: usize,
    /// stijl van het sjabloon: "GEGE", "TIKI", …; leeg voor een eigen tactiek
    pub style: String,
    pub slots: Vec<Slot>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Player {
    pub uid: u32,
    pub name: String,
    /// GK, SW, DL, DC, DR, DM, ML, MC, MR, AML, AMC, AMR, ST, WBL, WBR (1–20)
    pub positions: Vec<u8>,
    /// attributen 1–20, zoals FM ze toont
    pub attributes: HashMap<&'static str, u8>,
    pub left_foot: u8,
    pub right_foot: u8,
    pub condition: u8,
    pub sharpness: u8,
    pub morale: u8,
    pub injured: bool,
    pub banned: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Tactics {
    /// de tactiek in gebruik (index in `tactics`)
    pub current: usize,
    pub tactics: Vec<Tactic>,
    pub players: Vec<Player>,
    /// uids van de bank, in FM's volgorde
    pub bench: Vec<u32>,
}

/// Het bit dat de rol noemt: het enige bit buiten 21–23. Iets anders is geen rolmasker.
pub fn role_bit(mask: u64) -> Option<u8> {
    let role = mask & !CLASS_BITS;
    (role.count_ones() == 1 && (mask & CLASS_BITS).count_ones() == 1).then(|| role.trailing_zeros() as u8)
}

/// De tabel plek-met-bal → plek-zonder-bal: elf verschillende getallen 0–10 direct na een byte
/// 0x1B, in het stuk van het blok na de plekken. Niet gevonden: dezelfde volgorde.
fn oop_table(block: &[u8], from: usize) -> [u8; SLOTS] {
    let end = (from + 0x80).min(block.len());
    for i in from..end.saturating_sub(SLOTS + 1) {
        if block[i] != 0x1B {
            continue;
        }
        let t = &block[i + 1..i + 1 + SLOTS];
        let mut seen = [false; SLOTS];
        if t.iter().all(|&x| (x as usize) < SLOTS && !std::mem::replace(&mut seen[x as usize], true)) {
            return t.try_into().unwrap();
        }
    }
    std::array::from_fn(|i| i as u8)
}

fn read_tactic(index: usize, block: u64, ids: &[u32], by_person: &HashMap<u32, u32>) -> Option<Tactic> {
    let b = mem::read_bytes(block, BLOCK)?;
    // de plekken liggen op de Mac 8 verder dan op Windows, net als de eigen velden van een speler
    let d = (game::layout().p_positions.wrapping_sub(MAC_POSITIONS)) as usize;
    let (ip0, oop0) = (0x160 - d, 0x480 - d);
    let mask = |at: usize| b.get(at..at + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap())).and_then(role_bit);
    let table = oop_table(&b, oop0 + SLOTS * SLOT - 0x10);
    let slots: Vec<Slot> = (0..SLOTS)
        .map(|i| {
            let o = table[i] as usize;
            Slot { uid: ids.get(i).and_then(|id| by_person.get(id)).copied(), ip: mask(ip0 + i * SLOT), oop: mask(oop0 + o * SLOT), oop_order: o as u8 }
        })
        .collect();
    // een blok zonder één leesbare rol is geen tactiek (vrijgegeven geheugen)
    if slots.iter().filter(|s| s.ip.is_some()).count() < 6 {
        return None;
    }
    let tag: String = b[STYLE..STYLE + 4].iter().rev().filter(|c| c.is_ascii_uppercase()).map(|&c| c as char).collect();
    Some(Tactic { index, style: if tag == "NULL" { String::new() } else { tag }, slots })
}

fn player(p: &PlayerInfo) -> Option<Player> {
    let d = game::layout().p_positions.wrapping_sub(MAC_POSITIONS);
    let positions = mem::read_bytes(p.addr + game::layout().p_positions, 15)?;
    if positions.iter().any(|&r| r > 20) {
        return None;
    }
    let lo = ATTRIBUTES.iter().map(|a| a.1).min().unwrap();
    let hi = ATTRIBUTES.iter().map(|a| a.1).max().unwrap();
    let raw = mem::read_bytes(p.addr + lo + d, (hi - lo + 1) as usize)?;
    let shown = |at: u64| ((raw[(at - lo) as usize] as u32 + 2) / 5).clamp(1, 20) as u8;
    if raw.iter().any(|&x| x > 100) {
        return None;
    }
    Some(Player {
        uid: p.uid,
        name: p.name.clone(),
        positions,
        attributes: ATTRIBUTES.iter().map(|&(n, at)| (n, shown(at))).collect(),
        left_foot: shown(LEFT_FOOT),
        right_foot: shown(RIGHT_FOOT),
        condition: (p.condition / 100).clamp(0, 100) as u8,
        sharpness: (p.sharpness / 100).clamp(0, 100) as u8,
        morale: p.morale.clamp(1, 20),
        injured: !p.injuries.is_empty(),
        banned: p.bans > 0,
    })
}

static CACHED_OBJS: std::sync::Mutex<(u64, u64)> = std::sync::Mutex::new((0, 0));

fn read_obj_raw(obj: u64, vts: &[u64]) -> Option<(u8, Vec<u32>, Option<Vec<u64>>)> {
    let vt = mem::u64_at(obj)?;
    if !vts.contains(&vt) {
        return None;
    }
    let h = mem::read_bytes(obj + SEL_LIST, 16)?;
    let (begin, end) = (
        u64::from_le_bytes(h[..8].try_into().unwrap()),
        u64::from_le_bytes(h[8..].try_into().unwrap()),
    );
    let raw = mem::read_bytes(obj + SEL_SLOTS, SEL_COUNT * 4)?;
    let ids: Vec<u32> = raw
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    let cur = mem::read_bytes(obj + SEL_CURRENT, 1)
        .map(|b| b[0])
        .unwrap_or(0);
    let blocks = if mem::is_ptr(begin)
        && end >= begin + 8
        && end <= begin + 24
        && (end - begin) % 8 == 0
    {
        mem::read_bytes(begin, (end - begin) as usize).map(|lb| {
            lb.chunks_exact(8)
                .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
                .collect::<Vec<u64>>()
        })
    } else {
        None
    };
    Some((cur, ids, blocks))
}

fn count_mine(ids: &[u32], by_person: &HashMap<u32, u32>) -> Option<usize> {
    let filled = ids[..SLOTS].iter().filter(|id| **id != EMPTY_SLOT).count();
    let mine = ids[..SLOTS]
        .iter()
        .filter(|id| by_person.contains_key(id))
        .count();
    (mine > 0 && 2 * mine > filled).then_some(mine)
}

fn valid_tactic_count(blocks: &[u64], ids: &[u32], by_person: &HashMap<u32, u32>) -> usize {
    blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| mem::is_ptr(**b))
        .filter_map(|(i, b)| read_tactic(i, *b, ids, by_person))
        .count()
}

fn find_club_tactic_objects(
    by_person: &HashMap<u32, u32>,
) -> Result<(u64, u64, u8, Vec<u32>, Vec<u64>), String> {
    let img = mem::game_plugin().ok_or("game_plugin yüklenmedi")?;
    let vts = rtti::vtables_of(&img, &rtti::decorated("simatch", "PREPARED_TACTICS_CREATOR_ARRAY"));
    if vts.is_empty() {
        return Err("Kadro seçimleri bulunamadı".into());
    }

    // Fast path: check cached (blocks_obj, sel_obj) pointers first (< 1ms)
    if let Ok(guard) = CACHED_OBJS.lock() {
        let (b_addr, s_addr) = *guard;
        if b_addr != 0 && s_addr != 0 {
            if let (Some((b_cur, _, Some(blocks))), Some((s_cur, s_ids, _))) =
                (read_obj_raw(b_addr, &vts), read_obj_raw(s_addr, &vts))
            {
                if count_mine(&s_ids, by_person).is_some()
                    && valid_tactic_count(&blocks, &s_ids, by_person) > 0
                {
                    let cur = if (s_cur as usize) < blocks.len() {
                        s_cur
                    } else {
                        b_cur
                    };
                    return Ok((b_addr, s_addr, cur, s_ids, blocks));
                }
            }
        }
    }

    let mut sel_candidates: Vec<(usize, u64, u8, Vec<u32>, Option<Vec<u64>>)> = Vec::new();
    let mut block_candidates: Vec<(bool, u64, u8, Vec<u64>)> = Vec::new();

    for obj in rtti::instances(&vts, 4_000) {
        let Some((cur, ids, blocks_opt)) = read_obj_raw(obj, &vts) else {
            continue;
        };
        let mine_opt = count_mine(&ids, by_person);
        if let Some(mine) = mine_opt {
            sel_candidates.push((mine, obj, cur, ids, blocks_opt.clone()));
        }
        if let Some(blocks) = blocks_opt {
            if blocks.iter().any(|b| mem::is_ptr(*b)) {
                block_candidates.push((mine_opt.is_some(), obj, cur, blocks));
            }
        }
    }

    // Prefer the TEAM live selection object (`blocks.is_none()`) with highest `mine` for active `cur` & `ids`
    sel_candidates.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.4.is_some().cmp(&b.4.is_some()))
            .then_with(|| a.1.cmp(&b.1))
    });

    let (_, s_addr, s_cur, s_ids, _) = sel_candidates.into_iter().next().ok_or(
        "Kulübünüzün kadro dizilişi bulunamadı: FM içinde Taktik ekranını açıp ilk 11'inizi seçin",
    )?;

    // Choose the tactic blocks container:
    // 1) An object that also has our squad (`is_sel == true`),
    // 2) Highest number of valid tactics (human manager has up to 3 prepared tactics; AI has 1),
    // 3) Closest address distance to `s_addr`.
    let mut best_block: Option<((bool, usize, std::cmp::Reverse<u64>), u64, u8, Vec<u64>)> = None;
    for (is_sel, b_obj, b_cur, blocks) in block_candidates {
        let cnt = valid_tactic_count(&blocks, &s_ids, by_person);
        if cnt == 0 {
            continue;
        }
        let dist = s_addr.abs_diff(b_obj);
        let key = (is_sel, cnt, std::cmp::Reverse(dist));
        if best_block.as_ref().is_none_or(|b| key > b.0) {
            best_block = Some((key, b_obj, b_cur, blocks));
        }
    }

    let (_, b_addr, b_cur, blocks) = best_block.ok_or("Taktik okunamıyor")?;
    if let Ok(mut guard) = CACHED_OBJS.lock() {
        *guard = (b_addr, s_addr);
    }
    let cur = if (s_cur as usize) < blocks.len() {
        s_cur
    } else {
        b_cur
    };
    Ok((b_addr, s_addr, cur, s_ids, blocks))
}

pub fn tactics() -> Result<Tactics, String> {
    let squad = game::squad()?;
    let lay = game::layout();
    let by_person: HashMap<u32, u32> = squad
        .iter()
        .filter_map(|p| Some((mem::u32_at(p.addr + lay.p_identity + p.shift + 8)?, p.uid)))
        .collect();
    let (_b_addr, _s_addr, cur_raw, ids, blocks) = find_club_tactic_objects(&by_person)?;
    let tactics: Vec<Tactic> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| mem::is_ptr(**b))
        .filter_map(|(i, b)| read_tactic(i, *b, &ids, &by_person))
        .collect();
    if tactics.is_empty() {
        return Err("Taktik okunamıyor".into());
    }
    let current = tactics
        .iter()
        .position(|t| t.index == cur_raw as usize)
        .unwrap_or(0);
    Ok(Tactics {
        current,
        tactics,
        players: squad.iter().filter_map(player).collect(),
        bench: ids[SLOTS..]
            .iter()
            .filter(|id| **id != EMPTY_SLOT)
            .filter_map(|id| by_person.get(id))
            .copied()
            .collect(),
    })
}

/// Uygulamadan seçilen taktiği (0, 1 veya 2) FM27 belleğindeki aktif taktik işaretçisine yazar.
pub fn set_active_tactic(index: usize) -> Result<usize, String> {
    if index >= 3 {
        return Err("Geçersiz taktik indeksi".into());
    }
    let squad = game::squad()?;
    let lay = game::layout();
    let by_person: HashMap<u32, u32> = squad
        .iter()
        .filter_map(|p| Some((mem::u32_at(p.addr + lay.p_identity + p.shift + 8)?, p.uid)))
        .collect();
    let (b_addr, s_addr, _cur, _ids, _blocks) = find_club_tactic_objects(&by_person)?;
    let mut targets = vec![b_addr];
    if s_addr != 0 && !targets.contains(&s_addr) {
        targets.push(s_addr);
    }
    for delta in [0x2170u64, 0x5808u64, 0x7978u64] {
        for base in [b_addr, s_addr] {
            for cand in [base.wrapping_add(delta), base.wrapping_sub(delta)] {
                if let Some(cur_bytes) = mem::read_bytes(cand + SEL_CURRENT, 2) {
                    if cur_bytes[0] < 3 && cur_bytes[1] == 0xFF && !targets.contains(&cand) {
                        targets.push(cand);
                    }
                }
            }
        }
    }
    let new_byte = [index as u8];
    for addr in targets {
        if let Some(cur_bytes) = mem::read_bytes(addr + SEL_CURRENT, 2) {
            if cur_bytes[0] < 3 && cur_bytes[1] == 0xFF {
                let _ = mem::write_checked(addr + SEL_CURRENT, &[cur_bytes[0]], &new_byte);
            }
        }
    }
    Ok(index)
}

#[derive(Serialize)]
pub struct IndividualOutcome {
    pub players: usize,
    pub positions_mastered: usize,
    pub focus_boosted: usize,
    pub fitness_restored: usize,
}

/// Bireysel antrenman otomasyonu:
/// - `mode == "auto"`: Aktif taktikteki mevki yatkınlıklarını Doğal (20) yapar, sağlıklı oyuncuların
///   kondisyon ve maç keskinliğini %100'de tutar, en zayıf bireysel odak özelliklerini ve zayıf ayağı geliştirir.
/// - `mode == "master_positions"`: Tüm oyuncuların oynayabildikleri mevkileri Doğal (20) yapar.
/// - `mode == "max_fitness"`: Tüm oyuncuların kondisyon ve maç keskinliğini %100 yapar.
/// - `mode == "boost_focus"`: Seçilen oyuncuların en zayıf 3 odak özelliğini ve zayıf ayağını geliştirir.
pub fn apply_individual_training(
    uids: &Option<Vec<u32>>,
    mode: &str,
) -> Result<IndividualOutcome, String> {
    use crate::game::{Change, Patch};
    let squad = game::squad()?;
    let lay = game::layout();
    let d = lay.p_positions.wrapping_sub(MAC_POSITIONS);
    let chosen: Vec<&PlayerInfo> = squad
        .iter()
        .filter(|p| uids.as_ref().is_none_or(|u| u.contains(&p.uid)))
        .collect();

    let mut patches = Vec::new();
    let mut out = IndividualOutcome {
        players: 0,
        positions_mastered: 0,
        focus_boosted: 0,
        fitness_restored: 0,
    };

    let do_fitness = matches!(mode, "auto" | "max_fitness");
    let do_positions = matches!(mode, "auto" | "master_positions");
    let do_focus = matches!(mode, "auto" | "boost_focus");
    let step: u8 = if mode == "boost_focus" { 5 } else { 2 };

    for p in chosen {
        let before = patches.len();

        // 1. Kondisyon & Maç Keskinliği (Condition & Match Sharpness -> 10000 = 100%)
        if do_fitness && p.injuries.is_empty() {
            let mut fit_changed = false;
            if let Some(cond) = mem::u16_at(p.addr + lay.p_condition) {
                if (1000..10_000).contains(&cond) {
                    patches.push(Patch {
                        addr: p.addr + lay.p_condition,
                        old: cond.to_le_bytes().to_vec(),
                        new: 10_000u16.to_le_bytes().to_vec(),
                    });
                    fit_changed = true;
                }
            }
            if let Some(sharp) = mem::u16_at(p.addr + lay.p_sharpness) {
                if (1000..10_000).contains(&sharp) {
                    patches.push(Patch {
                        addr: p.addr + lay.p_sharpness,
                        old: sharp.to_le_bytes().to_vec(),
                        new: 10_000u16.to_le_bytes().to_vec(),
                    });
                    fit_changed = true;
                }
            }
            if fit_changed {
                out.fitness_restored += 1;
            }
        }

        // 2. Mevki Yatkınlığı (Beceri >= 12 olan mevkileri veya birincil mevkiyi Doğal = 20 yap)
        if do_positions {
            if let Some(pos) = mem::read_bytes(p.addr + lay.p_positions, 15) {
                if pos.iter().all(|&r| r <= 20) {
                    let max_r = pos.iter().copied().max().unwrap_or(1);
                    let mut new_pos = pos.clone();
                    let mut pos_changed = false;
                    for (i, &r) in pos.iter().enumerate() {
                        if (r >= 12 || (r == max_r && r > 1)) && r < 20 {
                            new_pos[i] = 20;
                            pos_changed = true;
                        }
                    }
                    if pos_changed {
                        patches.push(Patch {
                            addr: p.addr + lay.p_positions,
                            old: pos,
                            new: new_pos,
                        });
                        out.positions_mastered += 1;
                    }
                }
            }
        }

        // 3. Bireysel Rol Odağı & Zayıf Ayak Gelişimi
        if do_focus {
            let is_gk = mem::u8_at(p.addr + lay.p_positions).unwrap_or(1) >= 15;
            let focus_pool: &[&str] = if is_gk {
                &[
                    "reflexes",
                    "handling",
                    "one_on_ones",
                    "command_of_area",
                    "aerial_reach",
                    "positioning",
                    "concentration",
                    "kicking",
                    "passing",
                    "anticipation",
                    "decisions",
                    "agility",
                ]
            } else {
                &[
                    "passing",
                    "first_touch",
                    "technique",
                    "dribbling",
                    "finishing",
                    "tackling",
                    "marking",
                    "positioning",
                    "anticipation",
                    "decisions",
                    "composure",
                    "concentration",
                    "vision",
                    "work_rate",
                    "teamwork",
                    "off_the_ball",
                    "stamina",
                    "pace",
                    "acceleration",
                    "agility",
                    "balance",
                    "strength",
                ]
            };

            let mut attr_vals: Vec<(u64, u8)> = Vec::new();
            for &(name, off) in &ATTRIBUTES {
                if focus_pool.contains(&name) {
                    let addr = p.addr + off + d;
                    if let Some(raw_v) = mem::u8_at(addr) {
                        if (5..90).contains(&raw_v) {
                            attr_vals.push((addr, raw_v));
                        }
                    }
                }
            }
            // En zayıf 3 odak özelliğini bul ve geliştir
            attr_vals.sort_by_key(|&(_, v)| v);
            let mut focus_changed = false;
            for &(addr, raw_v) in attr_vals.iter().take(3) {
                let target_cap = if mode == "boost_focus" { 95 } else { 85 };
                if raw_v < target_cap {
                    let new_v = (raw_v + step).min(target_cap);
                    patches.push(Patch {
                        addr,
                        old: vec![raw_v],
                        new: vec![new_v],
                    });
                    focus_changed = true;
                }
            }

            // Zayıf ayak gelişimi
            let lf_addr = p.addr + LEFT_FOOT + d;
            let rf_addr = p.addr + RIGHT_FOOT + d;
            if let (Some(lf), Some(rf)) = (mem::u8_at(lf_addr), mem::u8_at(rf_addr)) {
                let (weak_addr, weak_v) = if lf <= rf { (lf_addr, lf) } else { (rf_addr, rf) };
                if (5..75).contains(&weak_v) {
                    let new_v = (weak_v + step).min(80);
                    patches.push(Patch {
                        addr: weak_addr,
                        old: vec![weak_v],
                        new: vec![new_v],
                    });
                    focus_changed = true;
                }
            }

            if focus_changed {
                out.focus_boosted += 1;
            }
        }

        if patches.len() > before {
            out.players += 1;
        }
    }

    if !patches.is_empty() {
        game::apply_change(Change {
            label: "bireysel antrenman".into(),
            patches,
            ..Default::default()
        })?;
    }
    Ok(out)
}

#[derive(Serialize, Clone, Debug)]
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

#[derive(Serialize, Clone, Debug)]
pub struct EasyTalkPlayer {
    pub uid: u32,
    pub name: String,
    pub team_index: usize,
    pub team_name: String,
    pub positions: Vec<u8>,
    pub attributes: HashMap<&'static str, u8>,
    pub raw_attributes: HashMap<&'static str, u8>,
    pub left_foot: u8,
    pub right_foot: u8,
    pub condition: u8,
    pub sharpness: u8,
    pub morale: u8,
    pub injured: bool,
    pub banned: bool,
    pub loaned: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct EasyTalkData {
    pub club_name: String,
    pub teams: Vec<EasyTalkTeam>,
    pub players: Vec<EasyTalkPlayer>,
}

pub fn easy_talk_data() -> Result<EasyTalkData, String> {
    let c = game::chain()?;
    let club_name = game::club_name(c.club).unwrap_or_else(|| "Kulüp".into());
    let team_ptrs = game::club_teams(c.club);
    let lay = game::layout();
    let d = lay.p_positions.wrapping_sub(MAC_POSITIONS);
    let lo = ATTRIBUTES.iter().map(|a| a.1).min().unwrap();
    let hi = ATTRIBUTES.iter().map(|a| a.1).max().unwrap();

    let mut teams = Vec::new();
    let mut players = Vec::new();
    let mut seen_uids = std::collections::HashSet::new();

    for (idx, &t_ptr) in team_ptrs.iter().enumerate() {
        if mem::u64_at(t_ptr + game::TEAM_CLUB) != Some(c.club) {
            continue;
        }
        let (kind, age_limit) = game::team_kind_and_age(t_ptr);
        let t_name = if t_ptr == c.team || kind == 0 {
            club_name.clone()
        } else if (1..100).contains(&age_limit) {
            format!("U-{age_limit}")
        } else {
            match kind {
                10 => "U-21".to_string(),
                11 => "U-20".to_string(),
                12 => "U-18".to_string(),
                13 => "U-19".to_string(),
                _ => format!("Takım {}", idx + 1),
            }
        };

        let Ok(sq) = game::squad_of(t_ptr) else {
            continue;
        };
        if sq.is_empty() {
            continue;
        }
        let mut total = 0usize;
        let mut avail = 0usize;
        let mut inj_cnt = 0usize;
        let mut loan_cnt = 0usize;

        for p in sq {
            if !seen_uids.insert(p.uid) {
                continue;
            }
            let Some(pos) = mem::read_bytes(p.addr + lay.p_positions, 15) else {
                continue;
            };
            if pos.iter().any(|&r| r > 20) {
                continue;
            }
            let Some(raw) = mem::read_bytes(p.addr + lo + d, (hi - lo + 1) as usize) else {
                continue;
            };
            if raw.iter().any(|&x| x > 100) {
                continue;
            }
            let shown = |at: u64| ((raw[(at - lo) as usize] as u32 + 2) / 5).clamp(1, 20) as u8;
            let raw_at = |at: u64| raw[(at - lo) as usize].clamp(1, 100);
            let injured = !p.injuries.is_empty();
            let loaned = (p.listed & 0b10) != 0;

            total += 1;
            if injured {
                inj_cnt += 1;
            } else if loaned {
                loan_cnt += 1;
            } else {
                avail += 1;
            }

            players.push(EasyTalkPlayer {
                uid: p.uid,
                name: p.name.clone(),
                team_index: idx,
                team_name: t_name.clone(),
                positions: pos,
                attributes: ATTRIBUTES.iter().map(|&(n, at)| (n, shown(at))).collect(),
                raw_attributes: ATTRIBUTES.iter().map(|&(n, at)| (n, raw_at(at))).collect(),
                left_foot: shown(LEFT_FOOT),
                right_foot: shown(RIGHT_FOOT),
                condition: (p.condition / 100).clamp(0, 100) as u8,
                sharpness: (p.sharpness / 100).clamp(0, 100) as u8,
                morale: p.morale.clamp(1, 20),
                injured,
                banned: p.bans > 0,
                loaned,
            });
        }

        if total > 0 {
            teams.push(EasyTalkTeam {
                index: idx,
                kind,
                age_limit,
                name: t_name,
                total_players: total,
                available_players: avail,
                injured_players: inj_cnt,
                loaned_players: loan_cnt,
            });
        }
    }

    Ok(EasyTalkData {
        club_name,
        teams,
        players,
    })
}

#[derive(Serialize)]
pub struct EasyTalkApplyOutcome {
    pub talked: usize,
    pub praised: usize,
    pub criticised: usize,
}

/// Easy Talk görüşmelerini tüm kulüp takımlarındaki seçili oyunculara uygular:
/// - Övgü (`praise_uids`): Oyuncunun moralini 20 (Mükemmel) yapar, mutsuzluk nedenlerini temizler,
///   kondisyon ve maç keskinliğini %100 yapar ve en zayıf 2 odak özelliğine +1 gelişim ivmesi verir.
/// - Eleştiri (`criticise_uids`): Oyuncuyu motive eder (moralini en az 18'e çeker, mutsuzluğu temizler)
///   ve düşüşteki / en zayıf 3 odak özelliğine +2 ekstra antrenman yüklemesi uygular.
pub fn apply_easy_talks(
    praise_uids: &[u32],
    criticise_uids: &[u32],
) -> Result<EasyTalkApplyOutcome, String> {
    use crate::game::{Change, Patch};
    let c = game::chain()?;
    let team_ptrs = game::club_teams(c.club);
    let lay = game::layout();
    let d = lay.p_positions.wrapping_sub(MAC_POSITIONS);

    let praise_set: std::collections::HashSet<u32> = praise_uids.iter().copied().collect();
    let crit_set: std::collections::HashSet<u32> = criticise_uids.iter().copied().collect();

    let mut patches = Vec::new();
    let mut praised = 0usize;
    let mut criticised = 0usize;
    let mut seen = std::collections::HashSet::new();

    for t_ptr in team_ptrs {
        if mem::u64_at(t_ptr + game::TEAM_CLUB) != Some(c.club) {
            continue;
        }
        let Ok(sq) = game::squad_of(t_ptr) else {
            continue;
        };
        for p in sq {
            if !seen.insert(p.uid) {
                continue;
            }
            let is_praise = praise_set.contains(&p.uid);
            let is_crit = !is_praise && crit_set.contains(&p.uid);
            if !is_praise && !is_crit {
                continue;
            }

            // 1. Moral & Mutluluk (Görüşme Tepkisi: Kabul etti / Motive oldu)
            if let Some(cur_m) = mem::u8_at(p.addr + lay.p_morale) {
                if (1..=20).contains(&cur_m) {
                    let target_m = if is_praise { 20 } else { cur_m.max(18) };
                    if cur_m != target_m {
                        patches.push(Patch {
                            addr: p.addr + lay.p_morale,
                            old: vec![cur_m],
                            new: vec![target_m],
                        });
                    }
                }
            }
            if let Some(ct) = game::player_contract(p.addr, p.shift) {
                let reasons = mem::u64_at(ct + 0x30).unwrap_or(0);
                if reasons != 0 {
                    patches.push(Patch {
                        addr: ct + 0x30,
                        old: reasons.to_le_bytes().to_vec(),
                        new: 0u64.to_le_bytes().to_vec(),
                    });
                }
            }

            // 2. Sağlıklıysa kondisyon ve keskinliği tazele
            if p.injuries.is_empty() {
                if let Some(cond) = mem::u16_at(p.addr + lay.p_condition) {
                    if (1000..10_000).contains(&cond) {
                        patches.push(Patch {
                            addr: p.addr + lay.p_condition,
                            old: cond.to_le_bytes().to_vec(),
                            new: 10_000u16.to_le_bytes().to_vec(),
                        });
                    }
                }
                if let Some(sharp) = mem::u16_at(p.addr + lay.p_sharpness) {
                    if (1000..10_000).contains(&sharp) {
                        patches.push(Patch {
                            addr: p.addr + lay.p_sharpness,
                            old: sharp.to_le_bytes().to_vec(),
                            new: 10_000u16.to_le_bytes().to_vec(),
                        });
                    }
                }
            }

            // 3. Antrenman görüşmesi gelişim etkisi (en zayıf 3 özelliğe odak ivmesi)
            let is_gk = mem::u8_at(p.addr + lay.p_positions).unwrap_or(1) >= 15;
            let focus_pool: &[&str] = if is_gk {
                &["reflexes", "handling", "one_on_ones", "command_of_area", "positioning", "concentration", "kicking", "agility"]
            } else {
                &["passing", "first_touch", "technique", "dribbling", "finishing", "tackling", "marking", "positioning", "anticipation", "decisions", "composure", "work_rate", "stamina", "pace", "acceleration"]
            };
            let mut attr_vals: Vec<(u64, u8)> = Vec::new();
            for &(name, off) in &ATTRIBUTES {
                if focus_pool.contains(&name) {
                    let addr = p.addr + off + d;
                    if let Some(raw_v) = mem::u8_at(addr) {
                        if (5..92).contains(&raw_v) {
                            attr_vals.push((addr, raw_v));
                        }
                    }
                }
            }
            attr_vals.sort_by_key(|&(_, v)| v);
            let boost_step: u8 = if is_crit { 2 } else { 1 };
            for &(addr, raw_v) in attr_vals.iter().take(3) {
                let new_v = (raw_v + boost_step).min(95);
                if new_v != raw_v {
                    patches.push(Patch {
                        addr,
                        old: vec![raw_v],
                        new: vec![new_v],
                    });
                }
            }

            if is_praise {
                praised += 1;
            } else {
                criticised += 1;
            }
        }
    }

    if !patches.is_empty() {
        game::apply_change(Change {
            label: "easy talk antrenman görüşmeleri".into(),
            patches,
            ..Default::default()
        })?;
    }

    Ok(EasyTalkApplyOutcome {
        talked: praised + criticised,
        praised,
        criticised,
    })
}
