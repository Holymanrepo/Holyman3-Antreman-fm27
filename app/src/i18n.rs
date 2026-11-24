//! Dil desteği: Türkçe (varsayılan), İngilizce ve Portekizce (Ayarlar'dan ve ilk kurulumda değiştirilebilir).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{OnceLock, atomic::{AtomicU8, Ordering}};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    #[serde(alias = "Nl", alias = "Other")]
    Tr,
    En,
    Pt,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(l: Lang) {
    let v = match l {
        Lang::Tr => 0,
        Lang::En => 1,
        Lang::Pt => 2,
    };
    CURRENT.store(v, Ordering::Relaxed);
}

pub fn get() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        2 => Lang::Pt,
        _ => Lang::Tr,
    }
}

pub fn is_tr() -> bool {
    get() == Lang::Tr
}

pub fn is_pt() -> bool {
    get() == Lang::Pt
}

pub fn detect() -> Lang {
    Lang::Tr
}

fn pt_map() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        for (en, pt) in [
            ("Menu", "Menu"),
            ("Overview", "Visão Geral"),
            ("Training", "Treino"),
            ("Easy Talk", "Easy Talk"),
            ("Planning", "Planeamento"),
            ("Squad", "Plantel"),
            ("Tactics", "Táticas"),
            ("Settings", "Definições"),
            ("Contact", "Contacto"),
            ("English", "Português"),
            ("Live: unsupported", "Ao vivo: não suportado"),
            ("Connected to FM27", "Conectado ao FM27"),
            ("Not connected", "Não conectado"),
            ("Undo", "Desfazer"),
            ("undo", "desfazer"),
            ("Reload", "Recarregar"),
            ("Reading…", "A ler…"),
            ("Dismiss", "Ocultar"),
            ("View on GitHub", "Ver no GitHub"),
            ("Update Now & Restart", "Atualizar e Reiniciar"),
            ("Download New Version", "Transferir Nova Versão"),
            ("Downloading & Installing…", "A transferir e instalar…"),
            ("Updating…", "A atualizar…"),
            ("General", "Geral"),
            ("App Language:", "Idioma da Aplicação:"),
            ("Bring FM window to front before reading", "Trazer janela do FM para a frente antes de ler"),
            ("FM27 Graphics Folder (Logos, Kits, Faces):", "Pasta Graphics do FM27 (Logos, Kits, Faces):"),
            ("Automatically loads the connected club's logo and graphics packs from this folder.", "Carrega automaticamente o emblema do clube conectado e pacotes gráficos desta pasta."),
            ("Player Faces Folder:", "Pasta de Faces dos Jogadores:"),
            ("Reload Faces", "Recarregar Faces"),
            ("Displays player portraits (face_<uid>.png) next to names in Easy Talk, Squad and Tactics.", "Exibe retratos dos jogadores (face_<uid>.png) ao lado dos nomes no Easy Talk, Plantel e Táticas."),
            ("Planner & Tactical Training", "Planeador e Treino Tático"),
            ("Tactical Style:", "Estilo Tático:"),
            ("Special Camp Mode:", "Modo de Estágio Especial:"),
            ("Smart rest load adjustment by squad condition", "Ajuste inteligente de descanso pela condição do plantel"),
            ("Add Team Bonding when Player Buy-In is below:", "Adicionar Coesão de Equipa quando a Adesão for inferior a:"),
            ("Set pieces every N weeks (0 = off)", "Bolas paradas a cada N semanas (0 = desligado)"),
            ("Update targets every N weeks", "Atualizar alvos a cada N semanas"),
            ("Rest Load", "Carga de Descanso"),
            ("More Rest", "Mais Descanso"),
            ("Balanced", "Equilibrado"),
            ("Less Rest", "Menos Descanso"),
            ("Every training day and the day before a match ends with Rest.", "Cada dia de treino e o dia anterior ao jogo terminam com Descanso."),
            ("Standard balanced schedule.", "Plano equilibrado padrão."),
            ("The day after a match adds a tactical session after Recovery.", "O dia seguinte ao jogo adiciona uma sessão tática após a Recuperação."),
            ("No tactical sessions (Player Development only)", "Sem sessões táticas (Apenas Desenvolvimento de Jogadores)"),
            ("These slots are used for player development matching your tactical style; match preparation stays. Ideal if you use 'Keep Locked at 100%' on the Squad tab.", "Estes espaços são usados para o desenvolvimento de jogadores adequado ao seu estilo tático; a preparação para o jogo mantém-se. Ideal se usar 'Manter Bloqueado a 100%' no separador Plantel."),
            ("Live Connection in FM", "Conexão ao Vivo no FM"),
            ("Allow live editing on all FM27 builds", "Permitir edição ao vivo em todas as versões do FM27"),
            ("Keeps all live features (training, budget, morale, injuries, transfers) working seamlessly after FM27 updates. Every change is verified before writing, can be undone, and only becomes permanent when you save in FM.", "Mantém todas as funcionalidades ao vivo a funcionar após atualizações do FM27. Cada alteração é verificada antes de gravar, pode ser desfeita e só se torna permanente quando grava no FM."),
            ("This FM build is verified and fully compatible.", "Esta versão do FM foi verificada e é totalmente compatível."),
            ("Live editing active: all features available.", "Edição ao vivo ativa: todas as funcionalidades disponíveis."),
            ("Read-only mode: enable the toggle above to make changes.", "Modo só de leitura: ative a opção acima para fazer alterações."),
            ("Folders & Permissions", "Pastas e Permissões"),
            ("Open export folder", "Abrir pasta de exportação"),
            ("Open app folder (last screenshot & analysis)", "Abrir pasta da aplicação (última captura e leitura)"),
            ("Contact & Updates", "Contacto e Atualizações"),
            ("For questions, feedback or support, feel free to reach out via email, follow my Kick channel, or check GitHub for the latest version.", "Para dúvidas, comentários ou suporte, entre em contacto por e-mail, siga o meu canal na Kick ou verifique o GitHub para a versão mais recente."),
            ("GitHub Page", "Página do GitHub"),
            ("Check for Updates", "Procurar Atualizações"),
            ("Checking GitHub…", "A verificar o GitHub…"),
            ("About & Credits", "Sobre e Créditos"),
            ("Session names, limits and templates come from FM27's own game data. The module only reads and writes after verification, with undo support. For single-player use only. Font: Inter (SIL OFL). Icons: Phosphor (MIT).", "Nomes de sessões, limites e modelos vêm dos dados do próprio FM27. O módulo apenas lê e grava após verificação, com suporte para desfazer. Apenas para uso um jogador."),
            ("Original FM27 Trainer architecture & base project:", "Arquitetura original do FM27 Trainer e projeto base:"),
            ("Heal Everyone", "Curar Todos"),
            ("Clear Suspensions", "Limpar Suspensões"),
            ("Max Everyone's Morale", "Moral Máxima para Todos"),
            ("Maximise Tactical Understanding", "Maximizar Entendimento Tático"),
            ("Tactical Understanding", "Entendimento Tático"),
            ("How well the squad knows your current tactic per area (player average). Maximising does what FM's in-game editor does: sets everything to 100%.", "Quão bem o plantel conhece a sua tática atual por área (média dos jogadores). Maximizar faz o mesmo que o editor do jogo: define tudo para 100%."),
            ("Keep Locked at 100%", "Manter Bloqueado a 100%"),
            ("ON", "LIGADO"),
            ("OFF", "DESLIGADO"),
            ("While the app is open, restores any values lowered by FM back to 100% every 20 seconds.", "Enquanto a aplicação estiver aberta, restaura quaisquer valores reduzidos pelo FM para 100% a cada 20 segundos."),
            ("Player", "Jogador"),
            ("Morale", "Moral"),
            ("Condition", "Condição"),
            ("Tactic", "Tática"),
            ("Injury", "Lesão"),
            ("Ban", "Castigo"),
            ("Unhappy", "Descontente"),
            ("banned", "suspenso"),
            ("yes", "sim"),
            ("Heal", "Curar"),
            ("Inspire", "Motivar"),
            ("How well your players fit the roles in your tactic and tactic-compatible training", "Quão bem os seus jogadores se adaptam às funções na sua tática e treino compatível"),
            ("Apply Training for This Tactic", "Aplicar Treino para Esta Tática"),
            ("Reading your tactic…", "A ler a sua tática…"),
            ("Current XI", "11 Atual"),
            ("average fit", "adaptação média"),
            ("Best XI", "Melhor 11"),
            ("your XI is already the best", "o seu 11 já é o melhor"),
            ("Weakest Slot", "Posição Mais Fraca"),
            ("no XI selected", "nenhum 11 selecionado"),
            ("Your Starting XI in This Tactic", "O Seu 11 Inicial Nesta Tática"),
            ("Line", "Setor"),
            ("IP / OOP Role", "Função C/ Bola / S/ Bola"),
            ("Fit", "Adaptação"),
            ("Position", "Posição"),
            ("Better Option in Squad", "Melhor Alternativa no Plantel"),
            ("injured", "lesionado"),
            ("empty", "vazio"),
            ("Best XI for This Tactic", "Melhor 11 para Esta Tática"),
            ("Assignment of your available players across the 11 slots for the highest total fit. Injured and suspended players are excluded.", "Distribuição dos seus jogadores disponíveis pelas 11 posições para a maior adaptação total. Jogadores lesionados e suspensos são excluídos."),
            ("(switches slot)", "(muda de posição)"),
            ("According to this calculation, your current starting XI is already the best.", "De acordo com este cálculo, o seu 11 inicial atual já é o melhor."),
            ("Better Fitting Roles", "Funções Mais Adequadas"),
            ("Another in-possession role on the same line that fits the player's attributes noticeably better.", "Outra função com bola no mesmo setor que se adapta significativamente melhor aos atributos do jogador."),
            ("None: every player is in a well-suited role.", "Nenhuma: todos os jogadores estão numa função adequada."),
            ("EASY TALK · Squad > Training", "EASY TALK · Plantel > Treino"),
            ("Batch praise & criticism talks for all club teams (First Team, U-20, U-18) based on training rating and attribute development.", "Conversas em lote de elogio e crítica para todas as equipas do clube (Equipa Principal, Sub-20, Sub-18) com base na nota de treino e evolução de atributos."),
            ("← Back to Table", "← Voltar à Tabela"),
            ("... Settings", "... Definições"),
            ("Reset Batch", "Reiniciar Lote"),
            ("Teams", "Equipas"),
            ("TEAMS", "EQUIPAS"),
            ("Praise development", "Elogiar evolução"),
            ("Praise rating", "Elogiar nota"),
            ("Criticise development", "Criticar evolução"),
            ("Criticise training", "Criticar treino"),
            ("Already talked", "Já conversado"),
            ("No talk", "Sem conversa"),
            ("left out", "de fora"),
            ("NAME", "NOME"),
            ("TEAM", "EQUIPA"),
            ("TALK", "CONVERSA"),
            ("DEV.", "EVOL."),
            ("RATING", "NOTA"),
            ("SQUAD", "PLANTEL"),
            ("CURRENT CUTOFFS", "LIMITES ATUAIS"),
            ("ALREADY TALKED", "JÁ CONVERSADO"),
            ("NO TALK", "SEM CONVERSA"),
            ("Nobody talked to yet.", "Ainda não conversou com ninguém."),
            ("See the last summary", "Ver último resumo"),
            ("Hide summary", "Ocultar resumo"),
            ("Click a name to leave it out of this batch · hover to see development · click the talk badge to switch it", "Clique num nome para o deixar fora deste lote · passe o rato para ver a evolução · clique na conversa para alterar"),
        ] {
            m.insert(en, pt);
        }
        m
    })
}

pub fn tr_or_en_or_pt<T: AsRef<str>>(tr: T, en: T) -> String {
    match get() {
        Lang::Tr => tr.as_ref().to_string(),
        Lang::En => en.as_ref().to_string(),
        Lang::Pt => {
            let e = en.as_ref();
            if let Some(&pt) = pt_map().get(e) {
                return pt.to_string();
            }
            e.replace(" players", " jogadores")
                .replace(" injured", " lesionados")
                .replace(" suspended", " suspensos")
                .replace(" talks in this batch", " conversas neste lote")
                .replace("Confirm ", "Confirmar ")
                .replace(" talks  »", " conversas  »")
                .replace(" · active", " · em uso")
                .replace("Tactic ", "Tática ")
                .replace("Suggested Training: ", "Treino Sugerido: ")
                .replace("New Update Available: ", "Nova Atualização Disponível: ")
                .replace("Current: ", "Atual: ")
                .replace("Week of ", "Semana de ")
                .replace(" matches", " jogos")
                .replace("matches plan", "conforme o plano")
        }
    }
}

pub fn pick_static(tr: &'static str, en: &'static str) -> &'static str {
    match get() {
        Lang::Tr => tr,
        Lang::En => en,
        Lang::Pt => pt_map().get(en).copied().unwrap_or(en),
    }
}

#[macro_export]
macro_rules! t {
    ($tr:expr $(,)?) => {
        $tr
    };
    ($tr:literal, $en:literal $(,)?) => {
        $crate::i18n::pick_static($tr, $en)
    };
    ($tr:expr, $en:expr $(,)?) => {
        if $crate::i18n::is_tr() {
            $tr
        } else {
            $crate::i18n::tr_or_en_or_pt($tr, $en)
        }
    };
    ($tr:expr, $en:expr, $pt:expr $(,)?) => {
        match $crate::i18n::get() {
            $crate::i18n::Lang::Tr => $tr,
            $crate::i18n::Lang::En => $en,
            $crate::i18n::Lang::Pt => $pt,
        }
    };
}

pub fn month_short(m: u32) -> &'static str {
    const TR: [&str; 12] = ["Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara"];
    const EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const PT: [&str; 12] = ["Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez"];
    let i = (m.clamp(1, 12) - 1) as usize;
    match get() {
        Lang::Tr => TR[i],
        Lang::En => EN[i],
        Lang::Pt => PT[i],
    }
}

pub fn weekday_short(i: usize) -> &'static str {
    const TR: [&str; 7] = ["Pzt", "Sal", "Çar", "Per", "Cum", "Cmt", "Paz"];
    const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const PT: [&str; 7] = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
    match get() {
        Lang::Tr => TR[i % 7],
        Lang::En => EN[i % 7],
        Lang::Pt => PT[i % 7],
    }
}

pub fn weekday_initial(i: usize) -> &'static str {
    const TR: [&str; 7] = ["P", "S", "Ç", "P", "C", "C", "P"];
    const EN: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];
    const PT: [&str; 7] = ["S", "T", "Q", "Q", "S", "S", "D"];
    match get() {
        Lang::Tr => TR[i % 7],
        Lang::En => EN[i % 7],
        Lang::Pt => PT[i % 7],
    }
}

pub fn date(d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    format!("{} {} {}", d.day(), month_short(d.month()), d.year())
}

pub fn date_short(d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    format!("{} {}", d.day(), month_short(d.month()))
}
