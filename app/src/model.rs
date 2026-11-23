//! Datamodel: de vier onderdelen van FM27 Tactical Effectiveness, de 17 key factors en de
//! trainingssessies. Namen zijn letterlijk zoals FM27 ze toont (uit de game-binary gehaald),
//! zodat ze in het spel één-op-één terug te vinden zijn.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Component {
    TacticalUnderstanding,
    PlayerAbility,
    PlayerBuyIn,
    NextMatchInfo,
}

impl Component {
    pub const ALL: [Component; 4] = [
        Component::TacticalUnderstanding,
        Component::PlayerAbility,
        Component::PlayerBuyIn,
        Component::NextMatchInfo,
    ];

    /// Titel zoals in FM27 (Türkçe / English).
    pub fn label(self) -> &'static str {
        if crate::i18n::is_tr() {
            match self {
                Component::TacticalUnderstanding => "Taktik Anlayışı",
                Component::PlayerAbility => "Oyuncu Yeteneği",
                Component::PlayerBuyIn => "Oyuncu Benimsemesi",
                Component::NextMatchInfo => "Sonraki Maç Bilgisi",
            }
        } else {
            match self {
                Component::TacticalUnderstanding => "Tactical Understanding",
                Component::PlayerAbility => "Player Ability",
                Component::PlayerBuyIn => "Player Buy-In",
                Component::NextMatchInfo => "Next Match Info",
            }
        }
    }

}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Factor {
    // Tactical Understanding → Tactical Areas (in balbezit)
    InPossessionShape,
    PassingStyle,
    BuildUp,
    ChanceCreation,
    // Tactical Understanding → Tactical Areas (zonder bal)
    OutOfPossessionShape,
    LineOfEngagement,
    PressingIntensity,
    PressingTrap,
    // Tactical Understanding → Tactical Comprehension
    PlayerTacticalAbility,
    TacticalComplexity,
    // Player Ability
    PlayerCapability,
    PlayerRoleSuitability,
    // Player Buy-In
    ManagerialSupport,
    TeammatesAndStaffRelationships,
    ManagerTendencies,
    // Next Match Info
    QualityOfNextOppositionReport,
    OppositionPlayerKnowledge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    InPossession,
    OutOfPossession,
}

impl Factor {
    pub const ALL: [Factor; 17] = [
        Factor::InPossessionShape,
        Factor::PassingStyle,
        Factor::BuildUp,
        Factor::ChanceCreation,
        Factor::OutOfPossessionShape,
        Factor::LineOfEngagement,
        Factor::PressingIntensity,
        Factor::PressingTrap,
        Factor::PlayerTacticalAbility,
        Factor::TacticalComplexity,
        Factor::PlayerCapability,
        Factor::PlayerRoleSuitability,
        Factor::ManagerialSupport,
        Factor::TeammatesAndStaffRelationships,
        Factor::ManagerTendencies,
        Factor::QualityOfNextOppositionReport,
        Factor::OppositionPlayerKnowledge,
    ];

    pub const TACTICAL_AREAS: [Factor; 8] = [
        Factor::InPossessionShape,
        Factor::PassingStyle,
        Factor::BuildUp,
        Factor::ChanceCreation,
        Factor::OutOfPossessionShape,
        Factor::LineOfEngagement,
        Factor::PressingIntensity,
        Factor::PressingTrap,
    ];

    /// Interne FM27-sessienaam (voor planner / session_code).
    pub fn session_name(self) -> &'static str {
        match self {
            Factor::InPossessionShape => "In Possession Shape",
            Factor::PassingStyle => "Passing Style",
            Factor::BuildUp => "Build Up",
            Factor::ChanceCreation => "Chance Creation",
            Factor::OutOfPossessionShape => "Out Of Possession Shape",
            Factor::LineOfEngagement => "Line of Engagement",
            Factor::PressingIntensity => "Pressing Intensity",
            Factor::PressingTrap => "Pressing Trap",
            Factor::PlayerTacticalAbility => "Player Tactical Ability",
            Factor::TacticalComplexity => "Tactical Complexity",
            Factor::PlayerCapability => "Player Capability",
            Factor::PlayerRoleSuitability => "Player Role Suitability",
            Factor::ManagerialSupport => "Managerial Support",
            Factor::TeammatesAndStaffRelationships => "Teammates and Staff Relationships",
            Factor::ManagerTendencies => "Manager Tendencies",
            Factor::QualityOfNextOppositionReport => "Quality of Next Opposition Report",
            Factor::OppositionPlayerKnowledge => "Opposition Player Knowledge",
        }
    }

    /// Türkçe / İngilizce etiket.
    pub fn label(self) -> &'static str {
        if !crate::i18n::is_tr() {
            return self.session_name();
        }
        match self {
            Factor::InPossessionShape => "Top Takımdayken Diziliş",
            Factor::PassingStyle => "Pas Stili",
            Factor::BuildUp => "Oyun Kurulum",
            Factor::ChanceCreation => "Pozisyon Üretimi",
            Factor::OutOfPossessionShape => "Top Rakipteyken Diziliş",
            Factor::LineOfEngagement => "Karşılama Çizgisi",
            Factor::PressingIntensity => "Pres Yoğunluğu",
            Factor::PressingTrap => "Pres Tuzağı",
            Factor::PlayerTacticalAbility => "Oyuncu Taktik Yeteneği",
            Factor::TacticalComplexity => "Taktiksel Karmaşıklık",
            Factor::PlayerCapability => "Oyuncu Yeterliliği",
            Factor::PlayerRoleSuitability => "Oyuncu Rolüne Uygunluk",
            Factor::ManagerialSupport => "Menajer Desteği",
            Factor::TeammatesAndStaffRelationships => "Takım Arkadaşları ve Personel İlişkileri",
            Factor::ManagerTendencies => "Menajer Eğilimleri",
            Factor::QualityOfNextOppositionReport => "Sonraki Rakip Raporunun Kalitesi",
            Factor::OppositionPlayerKnowledge => "Rakip Oyuncu Bilgisi",
        }
    }

    pub fn component(self) -> Component {
        use Factor::*;
        match self {
            InPossessionShape | PassingStyle | BuildUp | ChanceCreation | OutOfPossessionShape
            | LineOfEngagement | PressingIntensity | PressingTrap | PlayerTacticalAbility
            | TacticalComplexity => Component::TacticalUnderstanding,
            PlayerCapability | PlayerRoleSuitability => Component::PlayerAbility,
            ManagerialSupport | TeammatesAndStaffRelationships | ManagerTendencies => {
                Component::PlayerBuyIn
            }
            QualityOfNextOppositionReport | OppositionPlayerKnowledge => Component::NextMatchInfo,
        }
    }

    pub fn is_tactical_area(self) -> bool {
        Factor::TACTICAL_AREAS.contains(&self)
    }

    pub fn phase(self) -> Option<Phase> {
        use Factor::*;
        match self {
            InPossessionShape | PassingStyle | BuildUp | ChanceCreation => Some(Phase::InPossession),
            OutOfPossessionShape | LineOfEngagement | PressingIntensity | PressingTrap => {
                Some(Phase::OutOfPossession)
            }
            _ => None,
        }
    }

    /// Wat je eraan kunt doen (kort).
    pub fn advice(self) -> &'static str {
        use Factor::*;
        if crate::i18n::is_tr() {
            match self {
                InPossessionShape | PassingStyle | BuildUp | ChanceCreation | OutOfPossessionShape
                | LineOfEngagement | PressingIntensity | PressingTrap => {
                    "Aynı adlı antrenman seansını çalıştırın (Taktik Anlayışı grubu). Birincil Antrenman Taktiğinizi oynadığınız taktiğe ayarlayın."
                }
                PlayerTacticalAbility => {
                    "Oyuncuların kendilerine bağlıdır (karar alma, sezgi, takım oyunu). Daha yüksek Taktik Yeteneğine sahip oyuncuları oynatın veya talimatları daha basit tutun."
                }
                TacticalComplexity => {
                    "Çok fazla takım, oyuncu ve rakip talimatı var. Az katkı sağlayan talimatları kaldırın ve daha sonra tekrar ekleyin."
                }
                PlayerCapability => {
                    "Oyuncular rollerini yeterince iyi yerine getiremiyor. Özelliklerine uygun roller seçin veya kadroyu güçlendirin."
                }
                PlayerRoleSuitability => {
                    "Oyuncuları doğal mevki ve rollerinde oynatın. Puanı kimin düşürdüğünü görmek için Taktik sekmesini kontrol edin."
                }
                ManagerialSupport => {
                    "İyi sonuçlar, etkili takım konuşmaları ve oyuncularla olumlu diyaloglar menajer desteğini artırır."
                }
                TeammatesAndStaffRelationships => {
                    "Takım Kaynaşması (Ekstra Etkinlik) planlayın. Kadronun çekirdeğini bir arada tutun ve huzursuzluktan kaçının."
                }
                ManagerTendencies => {
                    "Oyuncular menajer profilinizdeki oyun tarzını bekler. Taktiğiniz bundan ne kadar uzaklaşırsa bu değer o kadar düşer."
                }
                QualityOfNextOppositionReport => {
                    "Analistlerinize/gözlemcilerinize bir sonraki rakip hakkında rapor hazırlatın ve iyi analistler işe alın."
                }
                OppositionPlayerKnowledge => {
                    "Rakibi ve oyuncularını gözlemleyerek artar. Tanınmayan (Avrupa) rakipleri önceden gözlemletin."
                }
            }
        } else {
            match self {
                InPossessionShape | PassingStyle | BuildUp | ChanceCreation | OutOfPossessionShape
                | LineOfEngagement | PressingIntensity | PressingTrap => {
                    "Run the training session with the same name (Tactical Understanding group). Set your Primary Trained Tactic to the tactic you play."
                }
                PlayerTacticalAbility => {
                    "Depends on the players themselves (Decisions, Anticipation, Teamwork). Play players with higher tactical ability or keep instructions simpler."
                }
                TacticalComplexity => {
                    "Too many team, player and opposition instructions. Remove instructions that add little and add them back later."
                }
                PlayerCapability => {
                    "Players cannot carry out their roles well enough. Pick roles that match their attributes or strengthen the squad."
                }
                PlayerRoleSuitability => {
                    "Play players in their natural position and role. Check the Tactics tab to see who pulls the score down."
                }
                ManagerialSupport => {
                    "Good results, effective team talks and positive conversations with players raise managerial support."
                }
                TeammatesAndStaffRelationships => {
                    "Schedule Team Bonding (Extra-Curricular). Keep the core of the squad together and avoid unrest."
                }
                ManagerTendencies => {
                    "Players expect the playing style from your manager profile. The further your tactic drifts from it, the lower this drops."
                }
                QualityOfNextOppositionReport => {
                    "Have your analysts/scouts compile a report on the next opponent and hire good analysts."
                }
                OppositionPlayerKnowledge => {
                    "Grows by scouting the opponent and their players. Scout unfamiliar (European) opponents ahead of time."
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    TacticalUnderstanding,
    MatchPreparation,
    FootballingDevelopment,
    PhysicalRecovery,
    ExtraCurricular,
    Fixed,
}

impl Category {
    pub fn label(self) -> &'static str {
        if crate::i18n::is_tr() {
            match self {
                Category::TacticalUnderstanding => "Taktik Anlayışı",
                Category::MatchPreparation => "Maç Hazırlığı",
                Category::FootballingDevelopment => "Futbol Gelişimi",
                Category::PhysicalRecovery => "Fiziksel ve Yenilenme",
                Category::ExtraCurricular => "Takım İçi Etkinlik",
                Category::Fixed => "Maç / Seyahat",
            }
        } else {
            match self {
                Category::TacticalUnderstanding => "Tactical Understanding",
                Category::MatchPreparation => "Match Preparation",
                Category::FootballingDevelopment => "Footballing Development",
                Category::PhysicalRecovery => "Physical & Recovery",
                Category::ExtraCurricular => "Extra-Curricular",
                Category::Fixed => "Match / Travel",
            }
        }
    }

    /// Kleur voor de GUI en de HTML-export (RGB).
    pub fn color(self) -> [u8; 3] {
        match self {
            Category::TacticalUnderstanding => [139, 124, 255],
            Category::MatchPreparation => [76, 178, 255],
            Category::FootballingDevelopment => [61, 214, 140],
            Category::PhysicalRecovery => [245, 181, 68],
            Category::ExtraCurricular => [244, 114, 182],
            Category::Fixed => [255, 92, 108],
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Session {
    pub name: &'static str,
    pub category: Category,
    /// Maximaal aantal keer per week (uit de game-data).
    pub max_per_week: i32,
    /// Effect op [conditie, vermoeidheid, geluk, blessurerisico, scherpte], −3…+3, zoals
    /// FM27 het in de sessie-info toont (positief = gunstig; uit de game-data).
    pub effects: [i8; 5],
}

const fn s(name: &'static str, category: Category, max_per_week: i32, effects: [i8; 5]) -> Session {
    Session { name, category, max_per_week, effects }
}

/// Alle 59 sessies van FM27, letterlijk uit `training/training_sessions` in de game-data
/// (game_plugin.bundle/…/game_simulation/training.fmf), in dezelfde volgorde.
pub const SESSIONS: &[Session] = &[
    s("Passing Style", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
    s("Build Up", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
    s("Chance Creation", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
    s("In Possession Shape", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
    s("Line of Engagement", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
    s("Pressing Intensity", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
    s("Pressing Trap", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
    s("Out Of Possession Shape", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
    s("Press Avoidance", Category::MatchPreparation, 1, [-1, 0, 0, -1, 2]),
    s("Ball Progression", Category::MatchPreparation, 1, [-1, 0, 0, -1, 2]),
    s("Final Third", Category::MatchPreparation, 1, [-1, 0, 0, -1, 2]),
    s("Defensive Block", Category::MatchPreparation, 1, [-1, 0, 0, -1, 2]),
    s("Defensive Positioning", Category::MatchPreparation, 1, [-1, 0, 0, -1, 2]),
    s("Defensive Engagement", Category::MatchPreparation, 1, [-1, 0, 0, -1, 2]),
    s("Corners", Category::MatchPreparation, 1, [-1, 0, 0, -1, 2]),
    s("Free Kicks", Category::MatchPreparation, 1, [-1, 0, 0, -1, 2]),
    s("Penalties", Category::MatchPreparation, 1, [-1, 0, 0, -1, 2]),
    s("Individual Development", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Attacking Ball Work", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Defensive Ball Work", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 1]),
    s("Possession Ball Work", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Attacking - Balanced", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Attacking - Patient", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Attacking - Direct", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Attacking - Wings", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Defending - Balanced", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Defending - Engaged", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Defending - Disengaged", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Defending - Wide", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Theory - Attacking Shadow Play", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Theory - Defensive Shadow Play", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Technical - Chance Creation", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Technical - Chance Conversion", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Technical - Ball Retention", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Technical - Ground Defence", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Technical - Aerial Defence", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Transition - Restrict", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Transition - Press", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Transition - Progress", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("GK - Balanced", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("GK - Shot Stopping", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("GK - Handling", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("GK - One On Ones", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("GK - Distribution", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Set Pieces - Delivery", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Set Pieces - Penalties", Category::FootballingDevelopment, 7, [-1, -1, 1, -1, 0]),
    s("Rest", Category::PhysicalRecovery, 21, [3, 1, 2, 0, -3]),
    s("Recovery", Category::PhysicalRecovery, 7, [1, 2, 1, 1, -2]),
    s("Light Conditioning", Category::PhysicalRecovery, 7, [-1, -1, -1, -2, -1]),
    s("Conditioning", Category::PhysicalRecovery, 7, [-3, -2, -2, -2, -1]),
    s("Heavy Conditioning", Category::PhysicalRecovery, 7, [-3, -3, -3, -2, -1]),
    s("Rest (IM)", Category::PhysicalRecovery, 21, [0, 0, 0, 0, 0]),
    s("Recovery (IM)", Category::PhysicalRecovery, 7, [0, 0, 0, 0, 0]),
    s("Light Conditioning (IM)", Category::PhysicalRecovery, 7, [0, 0, 0, 0, 0]),
    s("Team Bonding", Category::ExtraCurricular, 2, [1, 0, 3, 0, -3]),
    s("Community Outreach", Category::ExtraCurricular, 2, [1, 0, 3, 0, -3]),
    s("Travel", Category::ExtraCurricular, 21, [1, 0, 0, 0, -3]),
    s("Match", Category::Fixed, 7, [0, 0, 0, 0, 0]),
    s("Not Available", Category::PhysicalRecovery, 21, [0, 0, 0, 0, 0]),
    // Algemene keuzes die de SI-presets gebruiken: FM vult zelf de specifieke sessie in
    s("Match Preparation", Category::MatchPreparation, 9, [-1, 0, 0, -1, 2]),
    s("Tactical Understanding IP", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
    s("Tactical Understanding OOP", Category::TacticalUnderstanding, 8, [-1, 0, 0, -1, 1]),
];

impl Session {
    /// Belasting 0 (rust) … 6 (zwaar): hoe negatief het effect op conditie en vermoeidheid is.
    pub fn load(&self) -> i32 {
        (-(self.effects[0] as i32) - self.effects[1] as i32).max(0)
    }
}

pub fn session(name: &str) -> Option<&'static Session> {
    SESSIONS.iter().find(|s| s.name == name)
}

/// Interne sessiecode van FM27 (zoals in de save en het geheugen): Tactical 0–7, Match
/// Preparation 100–108, Footballing Development 200–228, Physical 300–308, Extra 400–402,
/// Match 500. Afgeleid van de volgorde in de game-data en gecontroleerd tegen de kalender in
/// een echte save (bv. dag [200, 3, 7] = Individual Development / In Possession Shape / Out Of
/// Possession Shape). De algemene keuzes ("Match Preparation", "Tactical Understanding IP")
/// hebben geen eigen code.
pub fn session_code(name: &str) -> Option<u16> {
    let idx = SESSIONS.iter().position(|s| s.name == name)?;
    match idx {
        0..=7 => Some(idx as u16),
        8..=16 => Some(100 + (idx - 8) as u16),
        17..=45 => Some(200 + (idx - 17) as u16),
        46..=53 => Some(300 + (idx - 46) as u16),
        54..=56 => Some(400 + (idx - 54) as u16),
        57 => Some(500),
        _ => None,
    }
}

pub fn session_by_code(code: u16) -> Option<&'static str> {
    SESSIONS.iter().map(|s| s.name).find(|n| session_code(n) == Some(code))
}

pub fn category_of(name: &str) -> Category {
    session(name).map(|s| s.category).unwrap_or(Category::Fixed)
}

/// Türkçe / İngilizce antrenman seansı adı (tam görünüm).
pub fn session_tr(name: &str) -> &'static str {
    if !crate::i18n::is_tr() {
        return match name {
            "Passing Style" => "Passing Style",
            "Build Up" => "Build Up",
            "Chance Creation" => "Chance Creation",
            "In Possession Shape" => "In Possession Shape",
            "Line of Engagement" => "Line of Engagement",
            "Pressing Intensity" => "Pressing Intensity",
            "Pressing Trap" => "Pressing Trap",
            "Out Of Possession Shape" => "Out Of Possession Shape",
            "Press Avoidance" => "Press Avoidance",
            "Ball Progression" => "Ball Progression",
            "Final Third" => "Final Third",
            "Defensive Block" => "Defensive Block",
            "Defensive Positioning" => "Defensive Positioning",
            "Defensive Engagement" => "Defensive Engagement",
            "Corners" => "Corners",
            "Free Kicks" => "Free Kicks",
            "Penalties" => "Penalties",
            "Individual Development" => "Individual Development",
            "Attacking Ball Work" => "Attacking Ball Work",
            "Defensive Ball Work" => "Defensive Ball Work",
            "Possession Ball Work" => "Possession Ball Work",
            "Attacking - Balanced" => "Attacking - Balanced",
            "Attacking - Patient" => "Attacking - Patient",
            "Attacking - Direct" => "Attacking - Direct",
            "Attacking - Wings" => "Attacking - Wings",
            "Defending - Balanced" => "Defending - Balanced",
            "Defending - Engaged" => "Defending - Engaged",
            "Defending - Disengaged" => "Defending - Disengaged",
            "Defending - Wide" => "Defending - Wide",
            "Theory - Attacking Shadow Play" => "Theory - Attacking Shadow Play",
            "Theory - Defensive Shadow Play" => "Theory - Defensive Shadow Play",
            "Technical - Chance Creation" => "Technical - Chance Creation",
            "Technical - Chance Conversion" => "Technical - Chance Conversion",
            "Technical - Ball Retention" => "Technical - Ball Retention",
            "Technical - Ground Defence" => "Technical - Ground Defence",
            "Technical - Aerial Defence" => "Technical - Aerial Defence",
            "Transition - Restrict" => "Transition - Restrict",
            "Transition - Press" => "Transition - Press",
            "Transition - Progress" => "Transition - Progress",
            "GK - Balanced" => "GK - Balanced",
            "GK - Shot Stopping" => "GK - Shot Stopping",
            "GK - Handling" => "GK - Handling",
            "GK - One On Ones" => "GK - One On Ones",
            "GK - Distribution" => "GK - Distribution",
            "Set Pieces - Delivery" => "Set Pieces - Delivery",
            "Set Pieces - Penalties" => "Set Pieces - Penalties",
            "Rest" | "Rest (IM)" => "Rest",
            "Recovery" | "Recovery (IM)" => "Recovery",
            "Light Conditioning" | "Light Conditioning (IM)" => "Light Conditioning",
            "Conditioning" => "Conditioning",
            "Heavy Conditioning" => "Heavy Conditioning",
            "Team Bonding" => "Team Bonding",
            "Community Outreach" => "Community Outreach",
            "Travel" => "Travel",
            "Match" => "Match",
            "Not Available" => "Not Available",
            "Match Preparation" => "Match Preparation",
            "Tactical Understanding IP" => "Tactical Understanding (IP)",
            "Tactical Understanding OOP" => "Tactical Understanding (OOP)",
            _ => "Training",
        };
    }
    match name {
        "Passing Style" => "Pas Stili",
        "Build Up" => "Oyun Kurulum",
        "Chance Creation" => "Pozisyon Üretimi",
        "In Possession Shape" => "Top Takımdayken Diziliş",
        "Line of Engagement" => "Karşılama Çizgisi",
        "Pressing Intensity" => "Pres Yoğunluğu",
        "Pressing Trap" => "Pres Tuzağı",
        "Out Of Possession Shape" => "Top Rakipteyken Diziliş",
        "Press Avoidance" => "Presten Çıkış",
        "Ball Progression" => "Topu İleri Taşıma",
        "Final Third" => "Üçüncü Bölge",
        "Defensive Block" => "Savunma Bloğu",
        "Defensive Positioning" => "Savunma Yerleşimi",
        "Defensive Engagement" => "Savunma Karşılaması",
        "Corners" => "Kornerler",
        "Free Kicks" => "Serbest Vuruşlar",
        "Penalties" => "Penaltılar",
        "Individual Development" => "Bireysel Gelişim",
        "Attacking Ball Work" => "Hücum Top Çalışması",
        "Defensive Ball Work" => "Savunma Top Çalışması",
        "Possession Ball Work" => "Pas & Topa Sahip Olma",
        "Attacking - Balanced" => "Hücum - Dengeli",
        "Attacking - Patient" => "Hücum - Sabırlı",
        "Attacking - Direct" => "Hücum - Dikine",
        "Attacking - Wings" => "Hücum - Kanatlar",
        "Defending - Balanced" => "Savunma - Dengeli",
        "Defending - Engaged" => "Savunma - Önde Baskı",
        "Defending - Disengaged" => "Savunma - Geride Kabul",
        "Defending - Wide" => "Savunma - Kanat Savunması",
        "Theory - Attacking Shadow Play" => "Teori - Hücum Gölge Oyunu",
        "Theory - Defensive Shadow Play" => "Teori - Savunma Gölge Oyunu",
        "Technical - Chance Creation" => "Teknik - Pozisyon Üretimi",
        "Technical - Chance Conversion" => "Teknik - Bitiricilik",
        "Technical - Ball Retention" => "Teknik - Top Saklama",
        "Technical - Ground Defence" => "Teknik - Yerden Savunma",
        "Technical - Aerial Defence" => "Teknik - Hava Savunması",
        "Transition - Restrict" => "Geçiş - Rakibi Kısıtlama",
        "Transition - Press" => "Geçiş - Şok Pres",
        "Transition - Progress" => "Geçiş - Hızlı Çıkış",
        "GK - Balanced" => "Kaleci - Dengeli",
        "GK - Shot Stopping" => "Kaleci - Şut Kurtarma",
        "GK - Handling" => "Kaleci - Top Kontrolü",
        "GK - One On Ones" => "Kaleci - Bire Birler",
        "GK - Distribution" => "Kaleci - Top Dağıtımı",
        "Set Pieces - Delivery" => "Duran Top - Orta & Servis",
        "Set Pieces - Penalties" => "Duran Top - Penaltılar",
        "Rest" | "Rest (IM)" => "Dinlenme",
        "Recovery" | "Recovery (IM)" => "Yenilenme",
        "Light Conditioning" | "Light Conditioning (IM)" => "Hafif Kondisyon",
        "Conditioning" => "Kondisyon",
        "Heavy Conditioning" => "Ağır Kondisyon",
        "Team Bonding" => "Takım Kaynaşması",
        "Community Outreach" => "Topluluk Etkinliği",
        "Travel" => "Seyahat",
        "Match" => "Maç",
        "Not Available" => "Müsait Değil",
        "Match Preparation" => "Maç Hazırlığı",
        "Tactical Understanding IP" => "Taktik Anlayışı (Topla)",
        "Tactical Understanding OOP" => "Taktik Anlayışı (Topsuz)",
        _ => "Antrenman",
    }
}

/// Türkçe / İngilizce odak adı.
pub fn focus_tr(focus: &str) -> &'static str {
    if crate::i18n::is_tr() {
        match focus {
            "Full Week of Rest" => "Tam Hafta Dinlenme",
            "In Possession" => "Top Takımdayken (Hücum)",
            "Out Of Possession" => "Top Rakipteyken (Savunma)",
            "Balanced" => "Dengeli",
            "Gegenpress" => "Gegenpress (Yüksek Pres)",
            "Tiki-Taka" => "Tiki-Taka (Topa Sahip Olma)",
            "Kanat & Hızlı Hücum" => "Kanat & Hızlı Hücum",
            "Savunma & Kontra" => "Savunma & Kontra",
            "Aktif Taktik Uyumlu" => "Aktif Taktik Uyumlu",
            _ => "Dengeli",
        }
    } else {
        match focus {
            "Full Week of Rest" => "Full Week of Rest",
            "In Possession" => "In Possession",
            "Out Of Possession" => "Out Of Possession",
            "Balanced" => "Balanced",
            "Gegenpress" => "Gegenpress (High Press)",
            "Tiki-Taka" => "Tiki-Taka (Possession)",
            "Kanat & Hızlı Hücum" => "Wing & Fast Attack",
            "Savunma & Kontra" => "Low Block & Counter",
            "Aktif Taktik Uyumlu" => "Active Tactic Compatible",
            _ => "Balanced",
        }
    }
}

/// Uitgelezen of handmatig ingevulde stand van alle factors (0–100) en onderdelen.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Effectiveness {
    pub factors: Vec<(Factor, f32)>,
    pub components: Vec<(Component, f32)>,
    pub source: String,
    /// Spel-datum waarop de Breakdown is uitgelezen (als FM met de module draaide).
    #[serde(default)]
    pub game_date: Option<chrono::NaiveDate>,
}

impl Default for Effectiveness {
    fn default() -> Self {
        Effectiveness {
            factors: Factor::ALL.iter().map(|f| (*f, 50.0)).collect(),
            components: Component::ALL.iter().map(|c| (*c, 50.0)).collect(),
            source: String::new(),
            game_date: None,
        }
    }
}

impl Effectiveness {
    pub fn get(&self, f: Factor) -> f32 {
        self.factors.iter().find(|(x, _)| *x == f).map(|(_, v)| *v).unwrap_or(50.0)
    }

    pub fn set(&mut self, f: Factor, v: f32) {
        if let Some(e) = self.factors.iter_mut().find(|(x, _)| *x == f) {
            e.1 = v.clamp(0.0, 100.0);
        } else {
            self.factors.push((f, v.clamp(0.0, 100.0)));
        }
    }

    pub fn component(&self, c: Component) -> f32 {
        if let Some((_, v)) = self.components.iter().find(|(x, _)| *x == c) {
            return *v;
        }
        let vals: Vec<f32> =
            Factor::ALL.iter().filter(|f| f.component() == c).map(|f| self.get(*f)).collect();
        vals.iter().sum::<f32>() / vals.len().max(1) as f32
    }

    /// Factors gesorteerd van zwak naar sterk.
    pub fn weakest(&self) -> Vec<(Factor, f32)> {
        let mut v: Vec<(Factor, f32)> = Factor::ALL.iter().map(|f| (*f, self.get(*f))).collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v
    }

    pub fn phase_score(&self, p: Phase) -> f32 {
        let v: Vec<f32> = Factor::TACTICAL_AREAS
            .iter()
            .filter(|f| f.phase() == Some(p))
            .map(|f| self.get(*f))
            .collect();
        v.iter().sum::<f32>() / v.len().max(1) as f32
    }

    pub fn buy_in(&self) -> f32 {
        self.get(Factor::TeammatesAndStaffRelationships)
            .min(self.get(Factor::ManagerialSupport))
    }
}

/// Kleurklasse van een meter: rood → groen, zoals FM27 ze toont.
pub fn level_name(score: f32) -> &'static str {
    if crate::i18n::is_tr() {
        match score {
            s if s < 25.0 => "çok düşük",
            s if s < 45.0 => "düşük",
            s if s < 60.0 => "orta",
            s if s < 80.0 => "iyi",
            _ => "mükemmel",
        }
    } else {
        match score {
            s if s < 25.0 => "very poor",
            s if s < 45.0 => "poor",
            s if s < 60.0 => "average",
            s if s < 80.0 => "good",
            _ => "excellent",
        }
    }
}

pub fn level_color(score: f32) -> [u8; 3] {
    // rood (0) → oranje → geel → limoen → groen (100), zoals de FM27-meters
    let stops: [(f32, [f32; 3]); 5] = [
        (0.0, [230.0, 60.0, 50.0]),
        (35.0, [240.0, 130.0, 30.0]),
        (55.0, [235.0, 215.0, 40.0]),
        (75.0, [170.0, 220.0, 50.0]),
        (100.0, [60.0, 200.0, 90.0]),
    ];
    let s = score.clamp(0.0, 100.0);
    for w in stops.windows(2) {
        let (a, ca) = w[0];
        let (b, cb) = w[1];
        if s <= b {
            let t = (s - a) / (b - a);
            return [
                (ca[0] + (cb[0] - ca[0]) * t) as u8,
                (ca[1] + (cb[1] - ca[1]) * t) as u8,
                (ca[2] + (cb[2] - ca[2]) * t) as u8,
            ];
        }
    }
    [60, 200, 90]
}
