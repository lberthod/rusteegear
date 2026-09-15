//! Salons multijoueurs (cf. SPRINT_MMORPG.md) : associe chaque joueur
//! réseau (`PlayerId`) à un objet de la scène qu'il pilote, sur le même principe
//! que `combat.rs` — isoler la surface de gameplay qu'un transport
//! réseau doit piloter, sans mélanger cette logique au reste de `AppState`.
//!
//! `pub` (contrairement à `combat`, privé) : `src/bin/server.rs` — un binaire
//! séparé qui ne voit que l'API publique de la bibliothèque — doit pouvoir
//! appeler ces méthodes pour faire entrer/sortir des joueurs réseau.

use super::AppState;
use crate::net::protocol::{EntityDelta, PlayerId, Snapshot};

/// État de contrôle d'un joueur réseau pour le tick courant (cf.
/// `net::protocol::ClientMsg::Input`, dont les champs sont recopiés tels quels
/// ici plutôt que de dépendre directement du type réseau — garde `AppState`
/// utilisable même si le protocole évolue, cf. la même séparation pour
/// `PlayerInput`, qui sert au joueur local).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetworkInput {
    pub move_x: f32,
    pub move_y: f32,
    /// Orientation (yaw, radians) prédite/affichée par le client — appliquée à
    /// son objet côté serveur (cf. `sim_step`) et direction de ses tirs (cf.
    /// `ClientMsg::Input::aim_yaw` pour le pourquoi).
    pub aim_yaw: f32,
    pub attack: bool,
    pub jump: bool,
    /// Tir de l'arme à distance (cf. `app::fireball`) : recharge validée côté
    /// serveur par objet tireur (`AppState::fireball_cooldowns`), comme `attack`.
    pub fire: bool,
    /// Arme à distance sélectionnée (indice dans `fireball::RANGED_WEAPONS`,
    /// borné par `sanitize_network_input`).
    pub weapon: u8,
    /// Soin d'un allié proche (cf. `app::health`) : résolu et validé côté serveur.
    pub heal: bool,
    /// Bouclier (14 septembre 2026, capacité 2) : cf. `ClientMsg::Input::block`.
    pub block: bool,
    /// Ruée (14 septembre 2026, capacité 4) : cf. `ClientMsg::Input::dash`.
    pub dash: bool,
}

/// Niveaux-paliers nommés (GDD §8.2 : « un palier = un déblocage nommé,
/// affiché dans le HUD au moment où il tombe ») — niv. 3 « La foudre
/// répond », niv. 6 « Le poids des braises », niv. 10 « Double foyer ».
pub const PALIER_LEVELS: [u32; 3] = [3, 6, 10];

/// Palier nommé franchi en gagnant `gained` XP par-dessus `prev_xp`, ou `None`.
/// Niveau plat `1 + xp / XP_PER_LEVEL` (GDD §8.2, même compte que le serveur).
/// Si plusieurs paliers tombent d'un coup (gros rattrapage), renvoie le plus
/// haut — une seule bannière, la plus prestigieuse.
pub fn palier_atteint(prev_xp: u32, gained: u32) -> Option<u32> {
    if gained == 0 {
        return None;
    }
    let level = |xp: u32| 1 + xp / crate::net::protocol::XP_PER_LEVEL;
    let before = level(prev_xp);
    let after = level(prev_xp.saturating_add(gained));
    PALIER_LEVELS
        .iter()
        .copied()
        .filter(|&p| before < p && p <= after)
        .max()
}

/// Classe d'un joueur réseau (`GAMEDESIGN_MMORPG.md` §3.2) : choisie au
/// `Join` (`ClientMsg::Join::class`), les modificateurs qu'elle implique ne
/// sont **jamais** appliqués côté client — `spawn_network_player` les
/// applique une fois pour toutes au clone du gabarit, au moment du spawn
/// (règle d'or anti-triche, GDD §5.7 : le serveur est seul juge).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayerClass {
    /// Valeurs actuelles inchangées (3 armes à distance, mêlée normale) :
    /// zéro régression pour qui ne choisit pas de classe.
    #[default]
    Assault,
    /// Vitesse +25 %, saut +30 %, PV max −30 % : attire, détourne, active —
    /// le kiting rendu viable par `MAX_ACTIVE_CHASERS_PER_TARGET` côté IA.
    Scout,
    /// Vitesse −15 %, dégâts infligés −30 %, soin ×2,5 (portée et débit) —
    /// et seule classe autorisée à réanimer (`update_network_revive`).
    Support,
    /// Cendre (14 septembre 2026 au soir, GDD §8.1) : le brasier mis en
    /// veille, couvé sous sa propre cendre pour tenir toute la nuit sans
    /// jamais s'éteindre. Vitesse −25 %, saut −15 %, PV max +60 %, dégâts à
    /// distance −20 %, aucun bonus de mêlée — et bouclier renforcé
    /// (`block_damage_mult` 0,10 au lieu de 0,25 pour les autres classes) :
    /// il encaisse en tête de groupe, il ne finit pas les combats vite.
    Tank,
    /// Brasier (14 septembre 2026 au soir, GDD §8.1 ; rééquilibré le 15
    /// septembre 2026 après revue adversariale, cf. `melee_damage_mult`/
    /// `attack_cooldown_mult`) : le feu qui s'emballe, consume tout ce qu'il
    /// touche mais se consume lui-même. Vitesse +10 %, PV max −25 %, dégâts à
    /// distance −40 %, dégâts de contact PvP ×1,4, cadence d'attaque +18 %
    /// (`attack_cooldown_mult` 0,85) — mais bouclier inutilisable
    /// (`block_damage_mult` 1.0, tenir la capacité 2 ne réduit aucun dégât) :
    /// le duelliste au contact, fragile mais dévastateur.
    Berserker,
    /// Givre (15 septembre 2026, GDD §8.1) : le troisième état du feu, celui
    /// qui attend — à l'opposé exact de Brasier. Première et seule classe à
    /// dépasser ×1,0 en `ranged_damage_mult` (×1,50), payé par les PV max les
    /// plus bas du roster (−35 %, en dessous même de l'Éclaireur) et une
    /// mêlée quasi inutilisable (`melee_damage_mult` 0,50) — ni la vitesse
    /// pour fuir ni la force pour se défendre au contact. Règle spéciale :
    /// portée de tir étendue (`ranged_lifetime_mult` ×1,25), pour engager en
    /// premier et kiter plutôt qu'un second multiplicateur de dégâts — cf.
    /// le calcul DPS/TTK détaillé sur `ranged_damage_mult` ci-dessous.
    Sniper,
}

/// Teinte déterministe par joueur (kit multi-couleur, roadmap 14 septembre
/// 2026 — cf. `assets/models/riviere/creature_ronde.glb`, matériaux
/// désaturés vers des gris clairs justement pour porter n'importe quelle
/// teinte uniforme sans dénaturer les yeux/l'ombrage). Dérivée du seul
/// `PlayerId` attribué par le serveur : tout le monde calcule la même
/// couleur pour un même id, sans coordination ni champ réseau
/// supplémentaire. Purement visuel, comme `PlayerClass::silhouette_tint`
/// (jamais calculé côté serveur, cf. `AppState::apply_class_silhouette`).
pub(crate) fn player_hue_tint(id: PlayerId) -> [f32; 3] {
    // Conjugué du nombre d'or, en tours : deux ids consécutifs (1, 2, 3...)
    // tombent sur des teintes très éloignées l'une de l'autre au lieu d'un
    // dégradé progressif qui rendrait deux joueurs voisins à peine
    // distinguables (même technique que le remplissage de teintes de
    // Godot/Blender).
    const GOLDEN_RATIO_CONJUGATE: f32 = 0.618_034;
    let hue = ((id as f32) * GOLDEN_RATIO_CONJUGATE).fract() * 360.0;
    // Saturation modérée, pleine valeur : des couleurs franches mais pas
    // criardes sur une silhouette presque blanche (contraste doux entre
    // joueurs sans virer au fluo).
    hsv_to_rgb(hue, 0.55, 1.0)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r1 + m, g1 + m, b1 + m]
}

impl PlayerClass {
    /// Traduit l'octet reçu du réseau — une valeur hors table retombe sur
    /// Assaut (même principe que `fireball::clamp_weapon` : un client
    /// modifié qui envoie `class: 250` ne panique jamais le serveur, il
    /// obtient juste les valeurs par défaut).
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => PlayerClass::Scout,
            2 => PlayerClass::Support,
            3 => PlayerClass::Tank,
            4 => PlayerClass::Berserker,
            5 => PlayerClass::Sniper,
            _ => PlayerClass::Assault,
        }
    }

    /// Teinte de silhouette (GDD §10.3, v7) : multiplie `SceneObject::color`
    /// du héros — Assaut neutre (zéro régression visuelle pour la classe par
    /// défaut), Éclaireur verdâtre (vif, végétal), Soutien doré (chaleureux,
    /// le « foyer »). Purement visuel, appliqué côté client
    /// (`apply_class_silhouette`) — jamais par le serveur.
    pub fn silhouette_tint(self) -> [f32; 3] {
        match self {
            PlayerClass::Assault => [1.0, 1.0, 1.0],
            PlayerClass::Scout => [0.72, 1.0, 0.82],
            PlayerClass::Support => [1.0, 0.88, 0.62],
            // Cendre : gris-bleu ardoise, la braise couverte.
            PlayerClass::Tank => [0.70, 0.76, 0.90],
            // Brasier : rouge-orangé profond, le feu qui s'emballe.
            PlayerClass::Berserker => [1.0, 0.42, 0.22],
            // Givre : bleu-blanc givré, le feu qui attend.
            PlayerClass::Sniper => [0.75, 0.90, 1.0],
        }
    }

    /// Facteur d'échelle de silhouette (GDD §10.3) : l'Éclaireur est élancé,
    /// le Soutien plus trapu — assez marqué pour se lire à distance, assez
    /// subtil pour ne pas mentir sur la hitbox (qui, elle, ne change pas :
    /// le serveur simule le gabarit commun).
    pub fn silhouette_scale(self) -> f32 {
        match self {
            PlayerClass::Assault => 1.0,
            PlayerClass::Scout => 0.90,
            PlayerClass::Support => 1.08,
            // Cendre : trapu, le plus large du roster (encaisse en tête).
            PlayerClass::Tank => 1.15,
            // Brasier : plus fin, tout en vitesse.
            PlayerClass::Berserker => 0.95,
            // Givre : élancé, précis.
            PlayerClass::Sniper => 0.92,
        }
    }

    /// Multiplicateur de `Controller::move_speed` appliqué au spawn.
    fn move_speed_mult(self) -> f32 {
        match self {
            PlayerClass::Assault => 1.0,
            PlayerClass::Scout => 1.25,
            PlayerClass::Support => 0.85,
            PlayerClass::Tank => 0.75,
            PlayerClass::Berserker => 1.10,
            PlayerClass::Sniper => 1.0,
        }
    }

    /// Multiplicateur de `Controller::jump_height` appliqué au spawn (GDD
    /// §3.2 : « saut +30 % » pour l'Éclaireur — distinct de la vitesse de
    /// déplacement, +25 %).
    fn jump_height_mult(self) -> f32 {
        match self {
            PlayerClass::Assault
            | PlayerClass::Support
            | PlayerClass::Berserker
            | PlayerClass::Sniper => 1.0,
            PlayerClass::Scout => 1.30,
            PlayerClass::Tank => 0.85,
        }
    }

    /// Multiplicateur de PV max (base `health::MAX_HEALTH`) appliqué au spawn.
    fn max_health_mult(self) -> f32 {
        match self {
            PlayerClass::Assault => 1.0,
            PlayerClass::Scout => 0.70,
            PlayerClass::Support => 1.0,
            PlayerClass::Tank => 1.60,
            PlayerClass::Berserker => 0.75,
            // Givre : les PV les plus bas du roster (glass cannon), aucune
            // compensation de vitesse contrairement à l'Éclaireur (0,70).
            PlayerClass::Sniper => 0.65,
        }
    }

    /// Multiplicateur des dégâts infligés (armes à distance, cf.
    /// `fireball::resolve_fireball_hit` **et** `fireball::resolve_pvp_ranged_hit`
    /// — appliqué aux deux depuis le 15 septembre 2026, cf. la doc de ce
    /// dernier pour l'asymétrie PvE/PvP que cet ajout a corrigée) — le
    /// Soutien tape moins fort, en échange de son soin renforcé et de sa
    /// réanimation exclusive ; Cendre et Brasier sont tous deux pénalisés à
    /// distance (14 septembre 2026 au soir, GDD §8.1) pour forcer
    /// l'engagement au contact, où leur identité respective (encaisser /
    /// cogner) se joue vraiment.
    ///
    /// Givre (15 septembre 2026, GDD §8.1) est la première et seule classe à
    /// dépasser ×1,0 : ×1,50, un axe unique (dégât seul, cadence inchangée)
    /// plus facile à raisonner que le double axe dégât+cadence de Brasier
    /// (×1,4/0,85 ≈ ×1,647 de DPS composé) — Givre reste strictement en
    /// dessous de ce ratio déjà validé. Calcul de référence (même méthode que
    /// `melee_damage_mult` ci-dessous, Éclair : dégât 1, recharge 0,45 s,
    /// `PVP_RANGED_DAMAGE_PER_HP` = 0,06, `n` = coups pour vider la cible,
    /// TTK = (n−1) × cadence) :
    /// - Givre vs Assaut (1,0 PV) : Givre tue en n=12 (TTK 4,95 s), Assaut tue
    ///   Givre (0,65 PV) en n=11 (TTK 4,50 s) — Assaut gagne légèrement
    ///   l'échange debout : le bonus de Givre est compensé par sa fragilité,
    ///   pas un matchup écrasant. Givre doit gagner via sa portée +25 %
    ///   (`ranged_lifetime_mult`), pas via le DPS brut seul.
    /// - Givre vs Cendre (1,60 PV, ×0,80) : Cendre tue en n=14 (TTK 5,85 s),
    ///   Givre tue Cendre en n=18 (TTK 7,65 s) — Cendre gagne l'échange debout
    ///   (~24 % plus vite), cohérent avec son rôle de mur.
    /// - Givre vs Brasier à distance (0,75 PV, ×0,60) : Givre tue en n=9
    ///   (TTK 3,6 s) contre n=19 (TTK 8,1 s) pour Brasier — Givre écrase
    ///   Brasier à distance, mais l'inverse est vrai au contact (cf.
    ///   `melee_damage_mult`) : contre-jeu structurel voulu, pas un
    ///   déséquilibre.
    pub(super) fn ranged_damage_mult(self) -> f32 {
        match self {
            PlayerClass::Support => 0.70,
            PlayerClass::Assault | PlayerClass::Scout => 1.0,
            PlayerClass::Tank => 0.80,
            PlayerClass::Berserker => 0.60,
            PlayerClass::Sniper => 1.50,
        }
    }

    /// Multiplicateur de la durée de vie d'un projectile (`RangedWeapon::
    /// lifetime`, cf. `fireball::spawn_fireball`) — donc de sa portée
    /// effective (`portée ≈ speed × lifetime × ce multiplicateur`). Règle
    /// spéciale de Givre (15 septembre 2026, GDD §8.1, « portée de
    /// précision ») : +25 %, orthogonale au tableau de dégâts/cadence
    /// ci-dessus — elle ne complique donc pas le calcul de DPS/TTK (aucun
    /// duelliste ne voit sa cadence ou ses dégâts changer), mais donne à un
    /// glass cannon sans compensation de vitesse un vrai moyen de dicter la
    /// distance d'engagement plutôt qu'un second bonus de dégâts. Toutes les
    /// autres classes gardent la portée universelle des armes.
    pub(super) fn ranged_lifetime_mult(self) -> f32 {
        match self {
            PlayerClass::Sniper => 1.25,
            _ => 1.0,
        }
    }

    /// Multiplicateur des dégâts de contact PvP (`PVP_MELEE_DAMAGE`, cf.
    /// `update_network_attacks`) — seul Brasier s'en écarte : ×1,4, la
    /// contrepartie de son bouclier inutilisable. Toutes les autres classes
    /// gardent la valeur universelle.
    ///
    /// Rééquilibré le 15 septembre 2026 après revue adversariale : les
    /// valeurs d'origine (×1,8 dégât, cadence ×0,65) donnaient un DPS effectif
    /// ×2,77 la référence (×1,8 dégât × 1/0,65 fréquence), un massacre garanti
    /// même contre Cendre — dont le bonus de PV (+60 %, linéaire) ne pouvait
    /// jamais compenser un multiplicateur de DPS composé aussi élevé. Calcul
    /// de référence (temps pour vider une cible, `PVP_MELEE_DAMAGE` = 0,15,
    /// `NETWORK_ATTACK_COOLDOWN` = 0,4 s, `n` = nombre de coups pour atteindre
    /// les PV de la cible, temps = (n−1) × cadence — le premier coup part
    /// sans attente) :
    /// - Référence (Assaut/Soutien, 1,0 PV, aucun bonus) : n=7, temps ≈ 2,4 s.
    /// - Brasier (×1,4 / 0,85) vs Assaut/Soutien (1,0 PV) : dégât 0,21/coup,
    ///   cadence 0,34 s, n=5, temps ≈ 1,36 s — 57 % du temps de référence,
    ///   franchement favorable à Brasier sans être expéditif (> 50 % du
    ///   temps de référence, pas de kill en moins de la moitié du temps).
    /// - Brasier vs Cendre (1,6 PV, sans bloquer) : n=8, temps ≈ 2,38 s.
    ///   Cendre (aucun bonus de mêlée) vs Brasier (0,75 PV) : n=5, temps ≈
    ///   1,6 s — Cendre tue ~33 % plus vite : son bonus de PV redevient un
    ///   vrai contre structurel au corps-à-corps, sans qu'un échange direct
    ///   soit pour autant un massacre garanti (Brasier place plusieurs coups
    ///   avant de tomber).
    ///
    /// Givre (15 septembre 2026, GDD §8.1) : ×0,50, la pénalité de mêlée la
    /// plus sévère du roster (dans ce sens) — la contrepartie de son bonus
    /// `ranged_damage_mult` unique. Calcul (`PVP_MELEE_DAMAGE` = 0,15,
    /// `NETWORK_ATTACK_COOLDOWN` = 0,4 s) : Brasier (×1,4/0,85, cadence
    /// 0,34 s) tue Givre (0,65 PV) en n=4 (TTK 1,02 s) ; Givre (×0,50, cadence
    /// 0,4 s) tue Brasier (0,75 PV) en n=10 (TTK 3,6 s) — Brasier écrase Givre
    /// au contact 3,5× plus vite, le contre-jeu structurel voulu (Givre gagne
    /// à distance, Brasier gagne au contact, cf. `ranged_damage_mult`) : la
    /// sanction d'un glass cannon qui se laisse toucher, pas un simple malus
    /// de PV isolé.
    pub(super) fn melee_damage_mult(self) -> f32 {
        match self {
            PlayerClass::Berserker => 1.4,
            PlayerClass::Sniper => 0.5,
            _ => 1.0,
        }
    }

    /// Multiplicateur du temps de recharge d'attaque (`NETWORK_ATTACK_COOLDOWN`,
    /// cf. `update_network_attacks`) — Brasier frappe 18 % plus vite, aussi
    /// bien en PvP qu'au corps-à-corps contre les monstres (le contact tue
    /// déjà un monstre en un seul coup : le bonus de Brasier contre eux est
    /// en fréquence de mise à mort, pas en dégât par coup — kill PvE en un
    /// coup inchangé). Toutes les autres classes gardent la cadence
    /// universelle. Voir le calcul de DPS ci-dessus (`melee_damage_mult`) :
    /// rééquilibré le 15 septembre 2026 avec ce multiplicateur (0,65 → 0,85).
    pub(super) fn attack_cooldown_mult(self) -> f32 {
        match self {
            PlayerClass::Berserker => 0.85,
            _ => 1.0,
        }
    }

    /// Multiplicateur de dégâts entrants appliqué en tenant le bouclier
    /// (capacité 2, cf. `health::apply_network_damage`) — `health::
    /// BLOCK_DAMAGE_MULT` (0,25) reste la valeur universelle ; Cendre la
    /// pousse à 0,10 (bouclier renforcé, 14 septembre 2026 au soir, GDD §8.1)
    /// et Brasier à 1.0 (bouclier inutilisable, la contrepartie de son ×1,8
    /// en dégâts de contact) : tenir la capacité 2 ne réduit alors plus rien.
    pub(super) fn block_damage_mult(self) -> f32 {
        match self {
            PlayerClass::Tank => 0.10,
            PlayerClass::Berserker => 1.0,
            _ => crate::app::health::BLOCK_DAMAGE_MULT,
        }
    }

    /// `true` seul le Soutien peut réanimer (GDD §8.1 : « seul à réanimer »).
    /// Cendre et Brasier n'en gagnent pas — décision de design, pas un oubli :
    /// la réanimation reste l'exclusivité du Soutien, tout comme avant.
    pub(super) fn can_revive(self) -> bool {
        matches!(self, PlayerClass::Support)
    }

    /// Traduit vers l'octet réseau (`ClientMsg::Join::class`) — symétrique de
    /// `from_u8`. Ajouté pour le sélecteur de classe (Sprint 3,
    /// `sprint10audit.md`) : jusqu'ici seul `from_u8` existait, rien
    /// n'émettait encore autre chose que `0` (Assaut) au `Join`.
    pub fn to_u8(self) -> u8 {
        match self {
            PlayerClass::Assault => 0,
            PlayerClass::Scout => 1,
            PlayerClass::Support => 2,
            PlayerClass::Tank => 3,
            PlayerClass::Berserker => 4,
            PlayerClass::Sniper => 5,
        }
    }

    /// Libellé court affiché dans le sélecteur de classe (fenêtre
    /// Multijoueur, Sprint 3).
    pub fn label(self) -> &'static str {
        match self {
            PlayerClass::Assault => "Assaut",
            PlayerClass::Scout => "Éclaireur",
            PlayerClass::Support => "Soutien",
            PlayerClass::Tank => "Cendre",
            PlayerClass::Berserker => "Brasier",
            PlayerClass::Sniper => "Givre",
        }
    }

    /// Les six classes existantes, dans l'ordre affiché au sélecteur.
    pub const ALL: [PlayerClass; 6] = [
        PlayerClass::Assault,
        PlayerClass::Scout,
        PlayerClass::Support,
        PlayerClass::Tank,
        PlayerClass::Berserker,
        PlayerClass::Sniper,
    ];
}

/// Objectif de manche (Phase C, `sprint10audit.md` : Sprint 5 pose l'enum et
/// migre Vagues, Sprint 6 ajoute Survie, Sprint 7 ajoute Escorte, Sprint 8
/// câble Boss sur `Vagues` — cf. doc de chaque variante). Choisi au `Join`
/// (`ClientMsg::Join::objective`,
/// `PROTOCOL_VERSION` 3) et fixé pour la durée de vie d'un salon (cf.
/// `bin/server.rs::Lobby::objective`), sur le même principe que `PlayerClass`
/// mais au niveau du salon plutôt que du joueur — tous les joueurs d'un même
/// salon jouent le même mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoundObjective {
    /// Vagues successives jusqu'à la dernière : comportement historique, seul
    /// mode qui existait avant ce sprint — défaut, zéro régression pour qui
    /// ne choisit pas de mode (`AppState::update_waves`).
    #[default]
    Vagues,
    /// Survivre `SURVIE_DURATION_SECS` (cf. `app::combat`) face à des vagues
    /// qui recommencent en boucle une fois la dernière vidée, plutôt que de
    /// s'arrêter à la victoire (GDD §4).
    Survie,
    /// Escorter un convoi (`SceneObject::convoy`) jusqu'à destination (GDD §4,
    /// Sprint 7 de `sprint10audit.md`) : victoire à l'arrivée
    /// (`AppState::update_escorte`), défaite si le convoi est détruit en route
    /// (`AppState::is_room_lost`) — indépendant de tout système de manches.
    Escorte,
    /// Vaincre un boss unique à PV élevés (GDD §4, Sprint 8) : le GDD le décrit
    /// comme « dernière vague : une créature unique », donc une scène Boss n'a
    /// qu'une seule manche contenant le boss — traité comme `Vagues` par
    /// `AppState::update_round` (victoire à la dernière manche vidée = boss
    /// vaincu), sans logique dédiée à dupliquer (cf. `Scene::boss_demo`).
    Boss,
}

impl RoundObjective {
    /// Toutes les valeurs de l'enum, dans l'ordre de `to_u8` — même usage que
    /// `PlayerClass::ALL` (itérer en test, ex. round-trip `to_u8`/`from_u8`).
    pub const ALL: [RoundObjective; 4] = [
        RoundObjective::Vagues,
        RoundObjective::Survie,
        RoundObjective::Escorte,
        RoundObjective::Boss,
    ];

    /// Traduit l'octet reçu du réseau — une valeur hors table retombe sur
    /// `Vagues` (même principe que `PlayerClass::from_u8`).
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => RoundObjective::Survie,
            2 => RoundObjective::Escorte,
            3 => RoundObjective::Boss,
            _ => RoundObjective::Vagues,
        }
    }

    /// Traduit vers l'octet réseau — symétrique de `from_u8`.
    pub fn to_u8(self) -> u8 {
        match self {
            RoundObjective::Vagues => 0,
            RoundObjective::Survie => 1,
            RoundObjective::Escorte => 2,
            RoundObjective::Boss => 3,
        }
    }

    /// Libellé affiché dans le sélecteur de mode réseau (fenêtre
    /// Multijoueur, Sprint 21, `sprintreflecion.md`) — même principe que
    /// `PlayerClass::label`.
    pub fn label(self) -> &'static str {
        match self {
            RoundObjective::Vagues => "Vagues",
            RoundObjective::Survie => "Survie",
            RoundObjective::Escorte => "Escorte",
            RoundObjective::Boss => "Boss",
        }
    }
}

/// Monde joué dans un salon (14 septembre 2026, multijoueur de la démo
/// « Rivière & cascade ») : indépendant de `RoundObjective` (qui décide de la
/// condition de victoire *dans* un monde) — `Hameau` est le monde MMORPG
/// historique (`Scene::embedded_player`, seul monde qui existait avant ce
/// jour), `Riviere` la vallée boisée de `Scene::riviere_demo` (pas de combat,
/// `update_round` n'y a donc jamais d'effet quel que soit l'objectif choisi —
/// cf. `Combat::wave` : aucun objet de cette scène n'en porte). Décidé côté
/// serveur par le **code du salon** (`net::protocol::RIVIERE_LOBBY`), jamais
/// transmis sur le fil : contrairement à `RoundObjective`/`PlayerClass`, pas
/// de `to_u8`/`from_u8` — rien dans `ClientMsg`/`ServerMsg` n'en a besoin.
/// Client comme serveur le déduisent de la scène chargée localement
/// (`AppState::world`, posé par `use_embedded_scene`/`use_embedded_scene_cached`
/// et `load_riviere_demo`), pas d'un choix explicite dans une fenêtre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorldKind {
    #[default]
    Hameau,
    Riviere,
}

/// Contrat du jour (GDD §3.4, Phase D — Sprint 9 de `sprint10audit.md`) : défi
/// quotidien dérivé du seed du jour (`Contract::of_day`), récompensé une fois
/// par compte et par jour (`PlayerProgress::last_contract_day`,
/// `net::firebase`). Sous-ensemble du catalogue du GDD (`GDD_MMORPG.md` §3.4,
/// 6 entrées) : *Main de braise* (mêlée seule) et *Sobriété* (sans ramassage
/// d'arme) en sont volontairement absents — le premier n'a aucune notion de
/// mêlée distincte du missile homing du joueur (`app::combat::AttackProjectile`,
/// toujours à distance), le second n'a pas de ramassage d'arme câblé aux
/// joueurs réseau (`WeaponPickup` n'existe que côté donjon solo) — le catalogue
/// « grandit avec » le contenu livré (GDD §3.4, règle du catalogue), il n'a
/// jamais eu à être complet dès ce sprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Contract {
    /// *Nuit blanche* : gagner la manche sans qu'aucun `GameEvent::PlayerDown`
    /// ne survienne (`AppState::player_down_count`).
    NuitBlanche,
    /// *À l'aube juste* : gagner une manche Horde (`RoundObjective::Vagues`) en
    /// moins de 8 minutes.
    AubeJuste,
    /// *La lande garde ses morts* : gagner sans qu'aucune réanimation ne se
    /// termine (`AppState::revives_completed`).
    LandeGardeSesMorts,
    /// *Le troupeau compte sur vous* : gagner une manche Escorte
    /// (`RoundObjective::Escorte`) avec le convoi à plus de 50 % de ses PV.
    TroupeauCompteSurVous,
}

impl Contract {
    /// Toutes les valeurs de l'enum, dans l'ordre de `to_u8` — même usage que
    /// `RoundObjective::ALL`/`PlayerClass::ALL`.
    pub const ALL: [Contract; 4] = [
        Contract::NuitBlanche,
        Contract::AubeJuste,
        Contract::LandeGardeSesMorts,
        Contract::TroupeauCompteSurVous,
    ];

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Contract::AubeJuste,
            2 => Contract::LandeGardeSesMorts,
            3 => Contract::TroupeauCompteSurVous,
            _ => Contract::NuitBlanche,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Contract::NuitBlanche => 0,
            Contract::AubeJuste => 1,
            Contract::LandeGardeSesMorts => 2,
            Contract::TroupeauCompteSurVous => 3,
        }
    }

    /// Contrat du jour pour un numéro de jour donné (jour UTC = secondes Unix
    /// / 86 400, cf. `bin/server.rs::day_number`) : « seed = jour UTC, calculé
    /// identiquement par serveur et clients » (GDD §3.4) — un simple modulo sur
    /// `ALL` suffit, déterministe et sans dépendance externe (pas de `chrono`).
    pub fn of_day(day: u64) -> Contract {
        Contract::ALL[(day as usize) % Contract::ALL.len()]
    }

    /// Libellé affiché (fiction « l'Almanach du Hameau », GDD §3.4).
    pub fn label(self) -> &'static str {
        match self {
            Contract::NuitBlanche => "Nuit blanche",
            Contract::AubeJuste => "À l'aube juste",
            Contract::LandeGardeSesMorts => "La lande garde ses morts",
            Contract::TroupeauCompteSurVous => "Le troupeau compte sur vous",
        }
    }
}

impl AppState {
    /// Le contrat du jour est-il rempli par **cette** manche, telle qu'elle
    /// vient de se terminer par une victoire (`elapsed` : durée écoulée depuis
    /// le début de la manche, `Room::started.elapsed()` côté serveur — vit
    /// dans `bin/server.rs`, pas dans `AppState`, d'où le paramètre plutôt
    /// qu'un champ) ? Appelé uniquement sur une manche **gagnée** : une
    /// défaite ne remplit jamais de contrat, quelle que soit sa condition.
    pub fn contract_completed(&self, contract: Contract, elapsed: std::time::Duration) -> bool {
        match contract {
            Contract::NuitBlanche => self.player_down_count == 0,
            Contract::AubeJuste => {
                self.objective == RoundObjective::Vagues
                    && elapsed < std::time::Duration::from_secs(8 * 60)
            }
            Contract::LandeGardeSesMorts => self.revives_completed == 0,
            Contract::TroupeauCompteSurVous => {
                self.objective == RoundObjective::Escorte
                    && self
                        .scene
                        .objects
                        .iter()
                        .find(|o| o.convoy.is_some())
                        .is_some_and(|o| {
                            o.combat.as_ref().is_some_and(|c| {
                                // `max_hp == 0` : jamais touché (cf. `Combat::max_hp`,
                                // capturé au premier coup reçu) — un convoi jamais
                                // attaqué est trivialement à plus de 50 % de ses PV.
                                c.max_hp == 0 || (c.hp as f32) > 0.5 * (c.max_hp as f32)
                            })
                        })
            }
        }
    }
}

/// Portée (m) de l'attaque réseau : un coup immédiat au contact, pas le
/// missile homing avec préparation du joueur local (`app::combat`, qui
/// dépend d'un unique `attack_charge`/`attack_projectile` par `AppState` —
/// en avoir un par joueur réseau est un vrai chantier de refonte, hors scope
/// ici). Volontairement courte : un coup à distance serait un simplement un
/// substitut dégradé du système existant, pas une intention de design.
const NETWORK_ATTACK_RANGE: f32 = 1.2;

/// Temps de recharge (s) entre deux attaques d'un même joueur réseau — sans
/// lui, un client qui maintient (ou spam) `attack: true` défairait tout ce
/// qui entre en portée sans le moindre risque, cf. le même raisonnement que
/// `Controller::attack_cooldown` pour le joueur local (`app::combat`).
const NETWORK_ATTACK_COOLDOWN: f32 = 0.4;

/// Dégâts (fraction de `health::MAX_HEALTH`) d'un coup au contact porté à un
/// **autre joueur** (14 septembre 2026, capacité 1 du kit 1-2-3-4, PvP). Même
/// ordre de grandeur qu'une morsure de créature (`BiteAttack::damage`,
/// 0,08-0,18 dans `scene/demos/mmorpg/creatures.rs`) pour rester cohérent
/// avec le rythme de combat déjà en place, plutôt qu'inventer une échelle à
/// part pour le PvP.
const PVP_MELEE_DAMAGE: f32 = 0.15;

/// Portée (m, centre à centre) du coup PvP au contact — distincte de
/// `NETWORK_ATTACK_RANGE` (1,2 m, testée contre l'AABB d'un monstre) : deux
/// personnages joueurs (`creature_ronde.glb`, corps kinématiques) ne peuvent
/// pas s'approcher à moins de ≈ 2,1 m l'un de l'autre (sonde en production,
/// 14 septembre 2026 au soir) — avec 1,2 m, un duel était physiquement
/// impossible. 2,8 m laisse la place d'un coup porté au contact ou d'un pas.
const PVP_MELEE_RANGE: f32 = 2.8;

/// Ruée (14 septembre 2026, capacité 4) : distance franchie (m) et temps de
/// recharge (s). Bornée par un raycast (`Physics::raycast`) pour ne jamais
/// traverser un mur/obstacle — but ludique = repositionnement rapide, pas un
/// outil pour sortir du niveau.
pub(super) const DASH_DISTANCE: f32 = 5.0;
pub(super) const DASH_COOLDOWN: f32 = 1.5;

/// Minuteurs d'animation des capacités d'un joueur réseau (cf.
/// `AppState::update_network_ability_animations`) : même mécanique que
/// `PlayerAttackState::swing_anim_remaining` pour le joueur local — un
/// **appui** (front montant) arme un minuteur pour que le clip joue en entier,
/// même sur une pression brève.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct NetAbilityAnim {
    pub swing_remaining: f32,
    pub cast_remaining: f32,
    pub attack_was_down: bool,
    pub cast_was_down: bool,
    /// Clip de capacité choisi au tick précédent (`None` = aucun) : détecte
    /// le front montant pour remettre le clip au début.
    pub current: Option<&'static str>,
}

/// Fenêtre (s) après laquelle un dégât porté à un monstre ne compte plus pour
/// l'assist (GDD §8.3) si un autre joueur l'achève — borne volontairement
/// courte : un dégât porté puis oublié pendant de longues secondes n'a plus
/// de lien réel avec la mise à mort qui suit (cf. `credit_assists_on_kill`).
const ASSIST_WINDOW: f32 = 5.0;

/// Ramène un yaw reçu du réseau dans `(-π, π]` (0 si `NaN`/infini) : un f32
/// **fini** mais énorme (ex. `1e30`, qu'un client modifié peut envoyer — il
/// passait l'ancien filtre `is_finite`) n'a plus aucune précision utile à
/// cette échelle — propagé tel quel au quaternion de l'objet puis au snapshot,
/// il dégradait l'orientation vue par **tous** les clients. Un client honnête
/// envoie déjà un yaw issu de `to_euler` (donc dans `(-π, π]`) : la
/// normalisation est neutre pour lui. Côté affichage, l'interpolation des
/// fantômes passe par `lerp_angle` (chemin angulaire court, wrap ±π géré, cf.
/// `net::interpolation`) : un passage 3,14 → −3,14 ne fait pas tourner le
/// fantôme à l'envers.
fn normalize_network_yaw(yaw: f32) -> f32 {
    if !yaw.is_finite() {
        return 0.0;
    }
    let wrapped = yaw.rem_euclid(std::f32::consts::TAU); // [0, TAU)
    if wrapped > std::f32::consts::PI {
        wrapped - std::f32::consts::TAU
    } else {
        wrapped
    }
}

/// Nettoie un `NetworkInput` reçu du réseau avant de le mémoriser (cf.
/// `AppState::set_network_input`) : rejette `NaN`/infini (remplacés par 0, le
/// neutre pour un axe de déplacement) et borne les axes à `[-1, 1]` — la même
/// borne que le joueur local (`inp.joy.0.clamp(-1.0, 1.0)`), appliquée ici à
/// la source plutôt que de faire confiance à `sim_step` pour la répéter.
///
/// **Décision d'audit — pas de durcissement supplémentaire d'`aim_yaw`** : ce
/// champ reste par ailleurs appliqué tel quel (il décide la direction du tir,
/// cf. `ClientMsg::Input::aim_yaw`) — un clamp de vitesse angulaire a été
/// évalué et écarté : il exigerait un état par joueur (dernier yaw + date),
/// pénaliserait les demi-tours souris légitimes (instantanés à 144 Hz), et
/// l'enjeu — un client modifié qui vise parfaitement — se limite à un aimbot
/// contre des **monstres** dans un coop PvE 2-16 joueurs, sans victime
/// humaine. Coût > enjeu ; à réévaluer seulement si du PvP apparaît.
fn sanitize_network_input(input: NetworkInput) -> NetworkInput {
    let clean = |v: f32| {
        if v.is_finite() {
            v.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    };
    NetworkInput {
        move_x: clean(input.move_x),
        move_y: clean(input.move_y),
        // Yaw : pas de clamp [-1, 1] (un angle vit au-delà) mais normalisé
        // dans (-π, π] — cf. `normalize_network_yaw` pour le pourquoi.
        aim_yaw: normalize_network_yaw(input.aim_yaw),
        attack: input.attack,
        jump: input.jump,
        fire: input.fire,
        // Borné à la table réelle dès la réception : le reste du code peut
        // indexer `RANGED_WEAPONS` sans re-vérifier.
        weapon: super::fireball::clamp_weapon(input.weapon) as u8,
        heal: input.heal,
        block: input.block,
        dash: input.dash,
    }
}

impl AppState {
    /// Masque le gabarit joueur local avant même qu'un joueur ne rejoigne — à
    /// appeler par un serveur headless juste après avoir chargé la scène, et
    /// **avant** de démarrer le Play (`self.playing = true`).
    ///
    /// Sans cet appel, le gabarit reste « le joueur » du point de vue de
    /// `player_index` (c'est le premier objet pilotable trouvé) pendant toute
    /// l'attente du premier joueur réseau : personne ne le pilote, les
    /// monstres le poursuivent quand même et sa santé s'épuise sans qu'il ne
    /// bouge jamais — la manche se terminait en défaite après quelques
    /// secondes, **avant même qu'un joueur n'ait eu le temps de se
    /// connecter** (cf. AUDIT_MMORPG.md). `spawn_network_player` masque
    /// aussi ce gabarit dès le premier joueur, donc cet appel n'est utile que
    /// pour la fenêtre *avant* ce premier join.
    /// Applique l'identité visuelle d'une classe (teinte + gabarit, GDD
    /// §10.3, protocole v7) à l'objet `index` — fantôme réseau ou joueur
    /// local. Purement cosmétique : la hitbox simulée par le serveur reste le
    /// gabarit commun. Idempotente : repart toujours de la base (échelle,
    /// couleur) mémorisée à la première application (`silhouette_base`), au
    /// lieu de composer les facteurs à chaque appel.
    pub(crate) fn apply_class_silhouette(&mut self, index: usize, class: PlayerClass) {
        let hue = self.player_hue_for_index(index);
        let Some(obj) = self.scene.objects.get_mut(index) else {
            return;
        };
        let (base_scale, base_color) = *self
            .silhouette_base
            .entry(index)
            .or_insert((obj.transform.scale, obj.color));
        obj.transform.scale = base_scale * class.silhouette_scale();
        let tint = class.silhouette_tint();
        obj.color = [
            base_color[0] * tint[0] * hue[0],
            base_color[1] * tint[1] * hue[1],
            base_color[2] * tint[2] * hue[2],
        ];
    }

    /// Teinte perso du joueur possédant l'objet `index` (kit multi-couleur,
    /// 14 septembre 2026) : `[1.0; 3]` (neutre) si `index` n'appartient à
    /// aucun `PlayerId` connu (solo, ou avant que le serveur n'ait attribué
    /// le sien — cf. `ServerMsg::Welcome`, qui réapplique la silhouette une
    /// fois l'id connu). Composée avec `PlayerClass::silhouette_tint` dans
    /// `apply_class_silhouette`, pas une teinte séparée : deux joueurs de la
    /// même classe restent identifiables l'un de l'autre.
    fn player_hue_for_index(&self, index: usize) -> [f32; 3] {
        let id = if Some(index) == self.player_index() {
            self.net_conn.net_player_id
        } else {
            self.network
                .network_players
                .iter()
                .find(|&(_, &i)| i == index)
                .map(|(&id, _)| id)
        };
        id.map(player_hue_tint).unwrap_or([1.0, 1.0, 1.0])
    }

    /// Dégage la zone d'apparition (roadmap post-audit UX v2 2026-09-04,
    /// 0.1 bis) : tout monstre `AiChaser` à moins de `SPAWN_SAFE_RADIUS` du
    /// gabarit du joueur est repoussé sur un anneau à `SPAWN_PUSH_RADIUS`,
    /// dans sa propre direction (ou une direction dérivée de son indice s'il
    /// est pile sur le point). Dans le hameau livré, des chasseurs campent
    /// le point d'apparition : un arrivant idle mourait en ~6 s sans avoir vu
    /// venir personne, ce qui décidait « Manche perdue » pour tout le salon.
    /// Appelé par le serveur à chaque début de manche (`Room::restart`) —
    /// pas en solo : les scènes scriptées (arène, rogue-like) placent leurs
    /// monstres à dessein près du joueur. Renvoie le nombre de monstres déplacés
    /// ; la physique doit être reconstruite ensuite par l'appelant si elle
    /// existe déjà (cf. `hide_local_player_template`).
    pub fn clear_spawn_area(&mut self) -> usize {
        const SPAWN_SAFE_RADIUS: f32 = 10.0;
        const SPAWN_PUSH_RADIUS: f32 = 12.0;
        let Some(spawn) = self
            .scene
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .map(|o| o.transform.position)
        else {
            return 0;
        };
        let mut moved = 0;
        for (i, o) in self.scene.objects.iter_mut().enumerate() {
            if o.ai_chaser.is_none() {
                continue;
            }
            let mut delta = o.transform.position - spawn;
            delta.y = 0.0;
            if delta.length() >= SPAWN_SAFE_RADIUS {
                continue;
            }
            let dir = if delta.length() > 1e-3 {
                delta.normalize()
            } else {
                let a = i as f32 * 2.399_963; // angle d'or : répartition régulière
                glam::Vec3::new(a.cos(), 0.0, a.sin())
            };
            o.transform.position = glam::Vec3::new(
                spawn.x + dir.x * SPAWN_PUSH_RADIUS,
                o.transform.position.y,
                spawn.z + dir.z * SPAWN_PUSH_RADIUS,
            );
            moved += 1;
        }
        moved
    }

    pub fn hide_local_player_template(&mut self) {
        if let Some(o) = self
            .scene
            .objects
            .iter_mut()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        {
            o.visible = false;
        }
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
    }

    /// Fait entrer un nouveau joueur réseau dans la partie : clone le premier
    /// objet « gabarit » pilotable trouvé dans la scène (le même gabarit que le
    /// joueur local utiliserait, cf. `player_index`) et l'ajoute comme un objet
    /// indépendant, avec sa propre entrée d'input. Reconstruit la physique
    /// (nouvel objet ⇒ nouveau corps rigide). `None` si la scène ne contient
    /// aucun gabarit pilotable (aucune manche/démo compatible chargée).
    ///
    /// **Idempotent** : si `id` a déjà un
    /// objet (un second `ClientMsg::Join` du même client — rejeu réseau, bug
    /// client, ou trame forgée par un client modifié : rien dans le protocole
    /// n'empêchait un client d'envoyer `Join` une seconde fois après le
    /// premier, cf. `net::server_loop::handle_connection`, qui ne borne que la
    /// *première* trame à être un `Join`), renvoie l'objet existant plutôt que
    /// d'en spawner un second — sinon l'ancien objet devient un fantôme : plus
    /// jamais référencé par `network_players` (donc invisible du `Snapshot`
    /// réseau), mais toujours simulé par la physique indéfiniment, et chaque
    /// spawn en trop reconstruit toute la physique de la scène (coût qui
    /// grandit avec le nombre d'objets).
    ///
    /// **Recyclage des emplacements orphelins** : `despawn_network_player` ne retire jamais l'objet du
    /// `Vec` (juste `visible = false`, cf. sa doc — retirer décalerait les
    /// indices des joueurs encore connectés). Sans recyclage, un salon qui
    /// voit beaucoup de va-et-vient (reconnexions, tests, joueurs qui
    /// abandonnent) accumule un clone du gabarit par `Join` **pour toujours**
    /// — chacun avec son propre corps physique dynamique, simulé à chaque pas
    /// même invisible (`controller.input` rend l'objet « controllable »
    /// indépendamment de sa visibilité, contrairement à `ai_chaser`). Sur un
    /// salon de longue durée (le salon partagé par défaut, potentiellement
    /// des heures), ça grossit sans borne et alourdit chaque `Physics::build`
    /// (reconstruit à chaque join/leave) — un vrai risque de à-coups perçus
    /// comme des blocages de mouvement en jeu. On réutilise donc en priorité
    /// un clone déjà présent (`controller.input`, hors gabarit d'origine) qui
    /// n'appartient plus à aucun joueur connu, avant d'en pousser un nouveau —
    /// borne la taille de la scène au pic de joueurs *simultanés* jamais
    /// atteint, pas au nombre cumulé de connexions depuis le démarrage.
    pub fn spawn_network_player(&mut self, id: PlayerId, class: PlayerClass) -> Option<usize> {
        if let Some(&existing) = self.network.network_players.get(&id) {
            return Some(existing);
        }
        let template_index = self
            .scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input))?;
        let reusable_index = self.scene.objects.iter().enumerate().position(|(i, o)| {
            i != template_index
                && o.controller.as_ref().is_some_and(|c| c.input)
                && !self.network.network_players.values().any(|&v| v == i)
        });
        let mut template = self.scene.objects[template_index].clone();
        // Écarte chaque joueur du gabarit d'origine (et des précédents) : sans ça,
        // deux corps rigides spawnés au même point s'interpénètrent et la physique
        // les sépare par une violente impulsion à la première étape de simulation.
        // Cercle serré autour du gabarit (pas une ligne qui s'éloigne de +5 m par
        // joueur) : tous les joueurs démarrent proches
        // les uns des autres, dans le même coin de la carte, pour pouvoir se voir
        // et marcher les uns vers les autres dès la connexion plutôt que de devoir
        // traverser toute l'arène.
        const SPAWN_RADIUS: f32 = 3.0;
        let n = self.network.network_players.len() as f32;
        let angle = n * std::f32::consts::TAU / 8.0;
        template.transform.position.x += angle.cos() * SPAWN_RADIUS;
        template.transform.position.z += angle.sin() * SPAWN_RADIUS;
        // Modificateurs de classe (GAMEDESIGN_MMORPG.md §3.2) : appliqués une
        // fois pour toutes au clone, jamais recalculés côté client — c'est le
        // serveur qui décide de la vitesse/portée de saut réellement simulées.
        if let Some(controller) = template.controller.as_mut() {
            controller.move_speed *= class.move_speed_mult();
            controller.jump_height *= class.jump_height_mult();
        }
        // Toujours visible, même si le gabarit d'origine est déjà masqué (cf. plus
        // bas, et `hide_local_player_template` appelé avant le premier join) : sans
        // ce reset, chaque joueur réseau hérite du `visible=false` du gabarit et
        // `network_snapshot` diffuse cette invisibilité telle quelle — les clients
        // ne voient alors jamais aucun fantôme, quelle que soit la justesse du reste
        // du pipeline réseau (cf. docs/audits/app-network.md pour le bug réel
        // que ça a causé).
        template.visible = true;
        let index = match reusable_index {
            Some(i) => {
                self.scene.objects[i] = template;
                i
            }
            None => {
                let i = self.scene.objects.len();
                self.scene.objects.push(template);
                i
            }
        };
        self.network.network_players.insert(id, index);
        self.network
            .network_inputs
            .insert(id, NetworkInput::default());
        self.network.network_classes.insert(id, class);
        // PV max modulés par la classe (GDD §3.2 : Éclaireur −30 %) : calculé
        // une fois au spawn, jamais recalculé côté client — cf. `health::
        // max_health_for`, seule fonction qui doit lire ce champ.
        let max_health = crate::app::health::MAX_HEALTH * class.max_health_mult();
        self.network.network_max_health.insert(id, max_health);
        // Vie individualisée (GAMEDESIGN_EN_LIGNE.md §3.1) : chaque joueur
        // réseau démarre à pleine vie (celle de sa classe), indépendamment
        // des autres — cf. `app::health`.
        self.network.network_health.insert(id, max_health);
        // Grâce d'apparition (roadmap v2 0.1 bis), cf. `health::update_network_health`.
        self.network
            .network_spawn_grace
            .insert(id, crate::app::health::SPAWN_GRACE_S);
        // Frags individualisés (brique de progression pour un futur MMORPG) :
        // chaque nouvelle connexion démarre à 0, comme la vie.
        self.network.network_kills.insert(id, 0);
        // Assists individualisés (cf. `credit_assists_on_kill`) : même
        // principe que les frags ci-dessus, démarre à 0 par connexion.
        self.network.network_assists.insert(id, 0);
        // Masque le gabarit d'origine : personne ne le pilote (ni un joueur
        // réseau — chacun a son propre clone — ni un joueur local, le serveur
        // étant headless). Sans ça, `player_index` continuerait de le désigner
        // comme « le joueur » (c'est le premier objet pilotable trouvé) : les
        // monstres poursuivraient un mannequin inerte au lieu des vrais joueurs
        // réseau, et sa santé qui s'épuise sans qu'il ne bouge jamais
        // terminerait la manche en défaite en quelques secondes, avant même
        // qu'un joueur ait eu le temps de rejoindre (cf. AUDIT_MMORPG.md).
        self.scene.objects[template_index].visible = false;
        // Reconstruction physique COMPLÈTE à chaque join (idem despawn) —
        // décision d'audit : documenté et accepté. `Physics` n'expose pas
        // d'API incrémentale (`add_body`/`remove_body`), un join/leave est
        // rare (au pire ~16 par manche), et le seul coût visible est la perte
        // des vitesses des corps (les positions survivent via les
        // transforms) : un à-coup d'une frame, jamais observé en jeu réel.
        // N'implémenter l'API incrémentale que si un test de ressenti à
        // plusieurs clients le justifie un jour.
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
        // Hauteur du sol à la nouvelle position (14 septembre 2026) : le décalage
        // `SPAWN_RADIUS` ci-dessus ne touche que x/z, en gardant le y du gabarit —
        // correct sur un sol plat (hameau) mais pas sur un terrain accidenté (la
        // vallée de la démo Rivière & cascade), où la hauteur du sol varie avec
        // x/z. Sans ce recalage, un joueur réseau spawne soit incrusté dans le
        // relief (repoussé violemment par la physique au premier pas, vu côté
        // client comme une téléportation à des dizaines de m/s), soit en
        // suspension au-dessus du sol (constaté en jeu : le fantôme flotte sans
        // jamais retomber, `KinematicCharacterController` n'ayant pas de
        // contrôleur/gravité côté fantômes clients, cf. `ensure_remote_player`,
        // et le serveur ne le corrige jamais puisqu'aucun input ne le fait
        // avancer). Deux rayons vers le bas plutôt qu'une hauteur recalculée en
        // dur : un à la position d'origine du gabarit, pour mesurer son décalage
        // vertical habituel au-dessus du sol (pieds du modèle, demi-hauteur de
        // la capsule…) sans le supposer, un à la nouvelle position x/z pour
        // trouver le sol réel là-bas — la différence préserve ce décalage quel
        // que soit le monde chargé, sans dépendre d'une constante par scène.
        const SPAWN_GROUND_PROBE: f32 = 6.0;
        const SPAWN_GROUND_MASK: u32 = 1;
        if let Some(phys) = self.physics.as_ref() {
            let origin_pos = self.scene.objects[template_index].transform.position;
            let spawned_pos = self.scene.objects[index].transform.position;
            let ground_below = |x: f32, z: f32| -> Option<f32> {
                phys.raycast(
                    glam::Vec3::new(x, origin_pos.y + SPAWN_GROUND_PROBE, z),
                    glam::Vec3::NEG_Y,
                    SPAWN_GROUND_PROBE * 2.0,
                    SPAWN_GROUND_MASK,
                )
                .map(|hit| hit.point.y)
            };
            if let (Some(origin_ground), Some(spawn_ground)) = (
                ground_below(origin_pos.x, origin_pos.z),
                ground_below(spawned_pos.x, spawned_pos.z),
            ) {
                let above_ground = origin_pos.y - origin_ground;
                self.scene.objects[index].transform.position.y = spawn_ground + above_ground;
            }
        }
        // Resynchronise le corps physique du fantôme réseau avec la position
        // corrigée ci-dessus — `Physics::build` vient de créer le corps à
        // l'ancien y, `set_position` aligne l'un sur l'autre (même précaution
        // que `update_network_dash` un peu plus haut dans ce fichier).
        if let Some(phys) = self.physics.as_mut() {
            let p = self.scene.objects[index].transform.position;
            phys.set_position(index, p);
        }
        Some(index)
    }

    /// Retire un joueur réseau (déconnexion volontaire ou timeout) :
    /// masque son objet (comme un ennemi vaincu, cf. `Combat::attackable`) plutôt
    /// que de le retirer du `Vec` — un retrait décalerait les indices de tous les
    /// joueurs suivants dans `scene.objects`, cassant leur mapping `network_players`.
    pub fn despawn_network_player(&mut self, id: PlayerId) {
        if let Some(index) = self.network.network_players.remove(&id)
            && let Some(o) = self.scene.objects.get_mut(index)
        {
            o.visible = false;
        }
        self.network.network_inputs.remove(&id);
        self.network.network_attack_cooldowns.remove(&id);
        self.network.network_health.remove(&id);
        self.network.network_kills.remove(&id);
        self.network.network_assists.remove(&id);
        self.network.network_classes.remove(&id);
        self.network.network_max_health.remove(&id);
        self.network.network_revive.remove(&id);
        self.network.network_respawn_timers.remove(&id);
        self.network.network_ability_anims.remove(&id);
        // Reconstruction complète documentée et acceptée — cf. le commentaire
        // du site jumeau dans `spawn_network_player`.
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
    }

    /// Oublie tous les joueurs réseau (cf. AUDIT_MMORPG.md
    /// §4.2) : à appeler chaque fois que `scene.objects` est remis à un état
    /// antérieur en bloc (`restart_game`, transition Play→Edit dans
    /// `advance_play`), qui ne connaît pas les objets ajoutés en cours de partie
    /// par `spawn_network_player`. Sans cet appel, `network_players` continuerait
    /// de pointer vers des indices obsolètes (un autre objet, ou plus rien) après
    /// la restauration — un joueur réseau piloterait alors le mauvais objet.
    /// Ne notifie pas les clients (pas de `PlayerLeft` envoyé) : c'est un simple
    /// nettoyage de la table de correspondance côté `AppState`, pas une
    /// déconnexion — le serveur réseau (`src/bin/server.rs`) reste responsable de
    /// décider s'il faut aussi fermer les connexions ou re-spawner les joueurs
    /// dans la scène restaurée.
    pub fn clear_network_players(&mut self) {
        self.network.network_players.clear();
        self.network.network_inputs.clear();
        self.network.network_attack_cooldowns.clear();
        self.network.network_health.clear();
        self.network.network_kills.clear();
        self.network.network_assists.clear();
        self.network.damage_contributions.clear();
        self.network.network_classes.clear();
        self.network.network_max_health.clear();
        self.network.network_spawn_grace.clear();
        self.network.network_revive.clear();
    }

    /// Enregistre l'input reçu d'un joueur réseau pour le tick courant : remplace
    /// le précédent (le client renvoie son état complet à chaque message, pas un
    /// delta, cf. `ClientMsg::Input`). Sans effet si `id` n'est pas (ou plus)
    /// connecté (message reçu après une déconnexion, par exemple).
    ///
    /// **Durcissement** : les valeurs brutes reçues du réseau ne
    /// sont **jamais** dignes de confiance (cf. `sanitize_network_input`) — un
    /// client modifié pourrait envoyer `NaN`/`Infinity` (bytes bincode arbitraires,
    /// pas nécessairement produits par un client légitime passant par des sliders
    /// egui bornés) ; sans nettoyage ici, un `NaN` se propagerait dans la
    /// position physique de l'objet (`f32::clamp` ne filtre pas `NaN`, cf. la
    /// sémantique de `f32::clamp` : les comparaisons avec `NaN` sont toujours
    /// fausses) et corromprait la simulation pour tout le monde, pas seulement
    /// ce joueur.
    pub fn set_network_input(&mut self, id: PlayerId, input: NetworkInput) {
        if let Some(slot) = self.network.network_inputs.get_mut(&id) {
            *slot = sanitize_network_input(input);
        }
    }

    /// Indice de l'objet piloté par ce joueur réseau, s'il est connecté.
    pub fn network_player_object(&self, id: PlayerId) -> Option<usize> {
        self.network.network_players.get(&id).copied()
    }

    /// Recherche inverse de `network_player_object` : quel joueur réseau pilote
    /// l'objet `index` ? `None` si `index` n'est celui d'aucun joueur réseau
    /// (joueur local, monstre, décor...). Sert à créditer le bon joueur d'un
    /// frag quand seul l'indice de l'objet tireur est connu (cf.
    /// `fireball::resolve_fireball_hit`).
    pub(super) fn network_player_id_at(&self, index: usize) -> Option<PlayerId> {
        self.network
            .network_players
            .iter()
            .find(|&(_, &i)| i == index)
            .map(|(&id, _)| id)
    }

    /// Crédite `id` d'un frag (brique de progression pour un futur MMORPG,
    /// GAMEDESIGN_EN_LIGNE.md) — sans effet si `id` n'est pas (ou plus)
    /// connecté (cible vaincue par un tir dont le tireur s'est déconnecté
    /// entre-temps, ex. un projectile encore en vol).
    pub(super) fn credit_kill(&mut self, id: PlayerId) {
        if let Some(count) = self.network.network_kills.get_mut(&id) {
            *count += 1;
        }
    }

    /// Enregistre que le joueur réseau `id` vient de porter un dégât au
    /// monstre d'indice `creature` — brique de détection d'assist (GDD §8.3) :
    /// si un autre joueur achève ce monstre dans `ASSIST_WINDOW`, `id` reçoit
    /// un assist (cf. `credit_assists_on_kill`). Écrase l'horodatage précédent
    /// du même couple (monstre, joueur) plutôt que d'empiler : seul le
    /// dernier coup de chacun compte pour la fenêtre.
    pub(super) fn record_damage_contribution(&mut self, creature: usize, id: PlayerId) {
        let now = self.time;
        self.network
            .damage_contributions
            .entry(creature)
            .or_default()
            .insert(id, now);
    }

    /// À la mort du monstre `creature`, crédite un assist à chaque joueur
    /// réseau ayant porté un dégât dans `ASSIST_WINDOW` (GDD §8.3), hors le
    /// tireur `killer` — celui-ci reçoit déjà le frag (`credit_kill`), jamais
    /// les deux pour la même mise à mort. Purge systématiquement l'historique
    /// du monstre, qu'un assist ait été crédité ou non : sa prochaine vie
    /// (après respawn) ne doit rien hériter de la précédente.
    pub(super) fn credit_assists_on_kill(&mut self, creature: usize, killer: PlayerId) {
        let now = self.time;
        if let Some(contributors) = self.network.damage_contributions.remove(&creature) {
            for (id, at) in contributors {
                if id != killer && now - at <= ASSIST_WINDOW {
                    self.credit_assist(id);
                }
            }
        }
    }

    /// Crédite `id` d'un assist (cf. `credit_assists_on_kill`) — compteur
    /// séparé de `credit_kill` pour ne jamais confondre un assist avec un
    /// frag côté XP (`round_xp`, `src/bin/server.rs`).
    fn credit_assist(&mut self, id: PlayerId) {
        if let Some(count) = self.network.network_assists.get_mut(&id) {
            *count += 1;
        }
    }

    /// Nombre de joueurs réseau actuellement en jeu (hors joueur local).
    pub fn network_player_count(&self) -> usize {
        self.network.network_players.len()
    }

    /// Frags du joueur réseau `id` (brique de progression pour un futur
    /// MMORPG), `None` s'il n'est pas connecté.
    pub fn network_player_kills(&self, id: PlayerId) -> Option<u32> {
        self.network.network_kills.get(&id).copied()
    }

    /// Assists du joueur réseau `id` (GDD §8.3, cf. `credit_assists_on_kill`),
    /// `None` s'il n'est pas connecté.
    pub fn network_player_assists(&self, id: PlayerId) -> Option<u32> {
        self.network.network_assists.get(&id).copied()
    }

    /// Classe du joueur réseau `id` (GDD §3.2), `None` s'il n'est pas connecté.
    pub fn network_player_class(&self, id: PlayerId) -> Option<PlayerClass> {
        self.network.network_classes.get(&id).copied()
    }

    /// Résout les attaques des joueurs réseau pour ce tick : décompte
    /// les temps de recharge, puis pour chaque joueur dont l'`Input` demande une
    /// attaque et dont le temps de recharge est écoulé, frappe immédiatement à
    /// portée (`NETWORK_ATTACK_RANGE`) depuis sa position — validation **serveur**
    /// du temps de recharge (`NETWORK_ATTACK_COOLDOWN`), pas seulement affichée
    /// côté client : un client modifié qui renvoie `attack: true` à chaque tick
    /// ne peut pas frapper plus vite que le temps de recharge imposé ici.
    ///
    /// **Vaincu = pas d'attaque** (GAMEDESIGN_EN_LIGNE.md §3.1) : un joueur à
    /// 0 PV est spectateur, son `Input` continue d'arriver (le client ne sait
    /// pas qu'il doit arrêter d'envoyer) mais le serveur l'ignore.
    pub fn update_network_attacks(&mut self, dt: f32) {
        for cd in self.network.network_attack_cooldowns.values_mut() {
            *cd -= dt;
        }
        let ids: Vec<PlayerId> = self.network.network_players.keys().copied().collect();
        for id in ids {
            let ready = self
                .network
                .network_attack_cooldowns
                .get(&id)
                .is_none_or(|cd| *cd <= 0.0);
            let wants_attack = self
                .network
                .network_inputs
                .get(&id)
                .is_some_and(|i| i.attack);
            let alive = self.network.network_health.get(&id).copied().unwrap_or(1.0) > 0.0;
            if !ready || !wants_attack || !alive {
                continue;
            }
            let Some(index) = self.network.network_players.get(&id).copied() else {
                continue;
            };
            let Some(pos) = self.scene.objects.get(index).map(|o| o.transform.position) else {
                continue;
            };
            let defeated = self.scene.attack_at(pos, NETWORK_ATTACK_RANGE);
            // `credit_kill`, l'unique chemin de comptage des frags (le même que
            // les projectiles, cf. `app::fireball`) — avant, ce site incrémentait
            // `network_kills` directement, deux logiques à garder synchronisées.
            // Sa garde « id inconnu = ignoré » est sans effet ici : `id` vient de
            // `network_players.keys()`, son entrée existe depuis
            // `spawn_network_player`. `credit_assists_on_kill` d'abord : le
            // contact tue toujours en un coup (pas de dégât partiel ici), donc
            // un assist n'a de sens que si une autre arme (tir à distance)
            // avait déjà entamé cette cible plus tôt — même mémoire partagée
            // que `resolve_fireball_hit` (cf. `damage_contributions`).
            for &i in &defeated {
                self.credit_assists_on_kill(i, id);
                self.credit_kill(id);
            }
            // PvP au contact (14 septembre 2026, capacité 1 du kit 1-2-3-4) :
            // seulement dans les scènes qui l'activent explicitement
            // (`Scene::ability_bar`) — laisse les mondes coopératifs existants
            // (Hameau MMORPG) inchangés, cf. la doc historique de
            // `fireball_impact` sur ce même choix. Pas d'équipe/allié : tout
            // autre joueur réseau à portée est une cible valide (mêlée à
            // tous, cohérent avec l'absence de notion de camp dans le projet).
            if self.scene.ability_bar {
                let targets: Vec<PlayerId> = self
                    .network
                    .network_players
                    .keys()
                    .copied()
                    .filter(|&other| other != id)
                    .collect();
                for other in targets {
                    let other_alive = self
                        .network
                        .network_health
                        .get(&other)
                        .copied()
                        .unwrap_or(1.0)
                        > 0.0;
                    let in_grace = self
                        .network
                        .network_spawn_grace
                        .get(&other)
                        .is_some_and(|g| *g > 0.0);
                    if !other_alive || in_grace {
                        continue;
                    }
                    let Some(other_index) = self.network.network_players.get(&other).copied()
                    else {
                        continue;
                    };
                    let Some(other_pos) = self
                        .scene
                        .objects
                        .get(other_index)
                        .map(|o| o.transform.position)
                    else {
                        continue;
                    };
                    if pos.distance(other_pos) > PVP_MELEE_RANGE {
                        continue;
                    }
                    // Brasier (14 septembre 2026 au soir, GDD §8.1 ; rééquilibré
                    // le 15 septembre 2026, cf. `melee_damage_mult`) : dégâts de
                    // contact PvP ×1,4, appliqués ici plutôt que côté client —
                    // même règle d'or anti-triche que `ranged_damage_mult`.
                    let melee_mult = self
                        .network_player_class(id)
                        .map_or(1.0, |c| c.melee_damage_mult());
                    let died = self.apply_network_damage(
                        other,
                        PVP_MELEE_DAMAGE * melee_mult,
                        crate::net::protocol::DeathCauseKind::Player,
                        index,
                    );
                    if died {
                        self.credit_kill(id);
                    }
                }
            }
            // Brasier : cadence d'attaque +18 % (`attack_cooldown_mult`), utile
            // aussi bien en PvP qu'en PvE (achève les monstres plus vite en
            // fréquence, pas en dégât par coup — le contact les tue déjà d'un
            // seul coup). Toute autre classe garde `NETWORK_ATTACK_COOLDOWN` tel quel.
            let cooldown_mult = self
                .network_player_class(id)
                .map_or(1.0, |c| c.attack_cooldown_mult());
            self.network
                .network_attack_cooldowns
                .insert(id, NETWORK_ATTACK_COOLDOWN * cooldown_mult);
        }
    }

    /// Anime les joueurs **réseau** selon la capacité qu'ils tiennent (14
    /// septembre 2026 au soir, `Scene::ability_bar`) : pendant de
    /// `simulation::apply_ability_animations` (joueur local) pour les objets
    /// pilotés par un `Input` réseau — côté serveur, le clip élu ici part tel
    /// quel dans `EntityDelta::anim_clip` et anime le fantôme chez **tous** les
    /// autres clients ; avant, seul Walk/Idle était diffusé et un coup ou un
    /// sort restait invisible pour les autres joueurs. Priorités et durées
    /// identiques au joueur local (Block > Attack > Cast > Dash,
    /// `ABILITY_ANIM_SECONDS`). Côté client, `network_inputs` est vide : no-op.
    pub(super) fn update_network_ability_animations(&mut self, dt: f32) {
        if !self.scene.ability_bar {
            return;
        }
        let anim_secs = Self::ABILITY_ANIM_SECONDS;
        let ids: Vec<PlayerId> = self.network.network_inputs.keys().copied().collect();
        for id in ids {
            let Some(inp) = self.network.network_inputs.get(&id).copied() else {
                continue;
            };
            let Some(&index) = self.network.network_players.get(&id) else {
                continue;
            };
            let dashing = self
                .network
                .network_dash_cooldowns
                .get(&id)
                .is_some_and(|cd| *cd > DASH_COOLDOWN - Self::ROLL_ANIM_SECONDS);
            let t = self.network.network_ability_anims.entry(id).or_default();
            if inp.attack && !t.attack_was_down {
                t.swing_remaining = anim_secs;
            }
            t.attack_was_down = inp.attack;
            let cast_down = inp.fire || inp.heal;
            if cast_down && !t.cast_was_down {
                t.cast_remaining = anim_secs;
            }
            t.cast_was_down = cast_down;
            t.swing_remaining = (t.swing_remaining - dt).max(0.0);
            t.cast_remaining = (t.cast_remaining - dt).max(0.0);
            let desired = if inp.block {
                Some("Block")
            } else if inp.attack || t.swing_remaining > 0.0 {
                Some("Attack")
            } else if cast_down || t.cast_remaining > 0.0 {
                Some("Cast")
            } else if dashing {
                Some("Dash")
            } else {
                None
            };
            let rising_edge = desired.is_some() && desired != t.current;
            t.current = desired;
            let Some(clip) = desired else { continue };
            if let Some(anim) = self
                .scene
                .objects
                .get_mut(index)
                .and_then(|o| o.animation.as_mut())
            {
                anim.clip = clip.to_string();
                anim.prev_clip.clear();
                anim.blend = 1.0;
                if rising_edge {
                    anim.time = 0.0;
                }
                self.ability_anim_locked.push(index);
            }
        }
    }

    /// Résout la ruée des joueurs réseau pour ce tick (14 septembre 2026,
    /// capacité 4 du kit 1-2-3-4) : même structure que `update_network_attacks`
    /// (recharge décomptée serveur, pas seulement côté client) — un bond
    /// instantané de `DASH_DISTANCE` dans la direction `aim_yaw` du joueur,
    /// borné par un rayon physique pour ne pas traverser les murs.
    pub fn update_network_dash(&mut self, dt: f32) {
        for cd in self.network.network_dash_cooldowns.values_mut() {
            *cd -= dt;
        }
        let ids: Vec<PlayerId> = self.network.network_players.keys().copied().collect();
        for id in ids {
            let ready = self
                .network
                .network_dash_cooldowns
                .get(&id)
                .is_none_or(|cd| *cd <= 0.0);
            let wants_dash = self.network.network_inputs.get(&id).is_some_and(|i| i.dash);
            let alive = self.network.network_health.get(&id).copied().unwrap_or(1.0) > 0.0;
            if !ready || !wants_dash || !alive {
                continue;
            }
            let Some(index) = self.network.network_players.get(&id).copied() else {
                continue;
            };
            let Some(pos) = self.scene.objects.get(index).map(|o| o.transform.position) else {
                continue;
            };
            let yaw = self
                .network
                .network_inputs
                .get(&id)
                .map_or(0.0, |i| i.aim_yaw);
            let forward = glam::Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
            self.network
                .network_dash_cooldowns
                .insert(id, DASH_COOLDOWN);
            let mut distance = DASH_DISTANCE;
            const DASH_RAYCAST_MASK: u32 = 1;
            // Même correctif que `combat::update_dash` (14 septembre 2026) :
            // décale l'origine du rayon hors du collider du joueur lui-même,
            // sinon `cast_ray(solid: true)` le touche à distance ≈ 0 et la
            // ruée réseau se résout en un bond invisible.
            const DASH_RAYCAST_SKIP: f32 = 0.4;
            let ray_origin = pos + forward * DASH_RAYCAST_SKIP;
            if let Some(phys) = self.physics.as_ref()
                && let Some(hit) = phys.raycast(
                    ray_origin,
                    forward,
                    (distance - DASH_RAYCAST_SKIP).max(0.0),
                    DASH_RAYCAST_MASK,
                )
            {
                distance = DASH_RAYCAST_SKIP + hit.distance;
            }
            let new_pos = pos + forward * distance;
            if let Some(o) = self.scene.objects.get_mut(index) {
                o.transform.position = new_pos;
            }
            // Resynchronise le corps physique du fantôme réseau — même
            // correctif que `combat::update_dash` (découvert dans la foulée
            // du bug de rayon ci-dessus, même symptôme).
            if let Some(phys) = self.physics.as_mut() {
                phys.set_position(index, new_pos);
            }
        }
    }

    /// Construit un `Snapshot` de tous les joueurs réseau, pour diffusion via
    /// `ServerMsg::Snapshot` (`send_to` en boucle sur les joueurs du salon,
    /// cf. `src/bin/server.rs` — pas `broadcast_all_rooms`, qui fuiterait
    /// l'état entre salons).
    ///
    /// **Vie individualisée (GAMEDESIGN_EN_LIGNE.md §3.1)** : `health` porte
    /// désormais la vie propre de chaque joueur (`app::health`), plus le champ
    /// scalaire unique d'avant — chaque client voit la vie de chacun, pas
    /// seulement la sienne (cf. `network_client::RemotePlayer::health`).
    pub fn network_snapshot(&self, tick: u32) -> Snapshot {
        let mut entities: Vec<EntityDelta> = self
            .network
            .network_players
            .iter()
            .filter_map(|(&id, &index)| self.scene.objects.get(index).map(|o| (id, index, o)))
            .map(|(id, index, o)| {
                let (yaw, _, _) = o.transform.rotation.to_euler(glam::EulerRot::YXZ);
                EntityDelta {
                    index: index as u32,
                    player_id: Some(id),
                    position: o.transform.position.to_array(),
                    yaw,
                    visible: o.visible,
                    health: self.network.network_health.get(&id).copied(),
                    kills: Some(self.network.network_kills.get(&id).copied().unwrap_or(0)),
                    // Phase L Sprint 3 (`sprint2audijeu0718.md`) : même politique que
                    // `kills` ci-dessus — jusqu'ici calculé (`network_assists`) mais
                    // jamais diffusé, le HUD ne pouvait afficher que les frags.
                    assists: Some(self.network.network_assists.get(&id).copied().unwrap_or(0)),
                    // Silhouettes de classe (v7, GDD §10.3) : chaque client
                    // teinte le fantôme selon la classe de son propriétaire.
                    class: Some(
                        self.network
                            .network_classes
                            .get(&id)
                            .copied()
                            .unwrap_or_default()
                            .to_u8(),
                    ),
                    anim_clip: o
                        .animation
                        .as_ref()
                        .map(|a| a.clip.clone())
                        .unwrap_or_default(),
                }
            })
            .collect();
        // Monstres (`Combat::attackable`, hors joueurs et ancre FX) : diffusés
        // avec `player_id: None` — le cas prévu de longue date par
        // `EntityDelta::player_id`, activé avec l'attaque à distance : sans
        // cette diffusion, chaque client simulerait/afficherait ses propres
        // monstres, et un monstre tué par la boule de feu d'un joueur resterait
        // debout sur l'écran des autres. Diffusés même masqués (mort = le
        // `visible: false` doit atteindre tous les écrans), et **par valeur de
        // scène partagée** : les indices coïncident car serveur et clients
        // chargent la même scène embarquée (cf. `src/bin/server.rs`).
        for (index, o) in self.scene.objects.iter().enumerate() {
            let is_monster = o.controller.is_none()
                && o.combat
                    .as_ref()
                    .is_some_and(|c| c.attackable && !c.is_attack_fx);
            if !is_monster {
                continue;
            }
            let (yaw, _, _) = o.transform.rotation.to_euler(glam::EulerRot::YXZ);
            // Vie du monstre, **normalisée** 0..1 (14 septembre 2026 au soir,
            // jauges au-dessus des têtes) : `max_hp` n'est capturé qu'au
            // premier coup (cf. `Combat::max_hp`), avant ça `hp` est le maximum.
            let health = o.combat.as_ref().map(|c| {
                let max = if c.max_hp > 0 { c.max_hp } else { c.hp.max(1) };
                (c.hp as f32 / max as f32).clamp(0.0, 1.0)
            });
            entities.push(EntityDelta {
                index: index as u32,
                player_id: None,
                position: o.transform.position.to_array(),
                yaw,
                visible: o.visible,
                health,
                kills: None,
                assists: None,
                class: None,
                anim_clip: o
                    .animation
                    .as_ref()
                    .map(|a| a.clip.clone())
                    .unwrap_or_default(),
            });
        }
        Snapshot {
            tick,
            entities,
            projectiles: self
                .projectiles
                .fireballs
                .iter()
                .map(|fb| crate::net::protocol::ProjectileState {
                    position: fb.pos.to_array(),
                    weapon: fb.weapon as u8,
                })
                .collect(),
            // Projectiles de créature (jet d'eau, crachat de feu...) — même non-
            // identité que les `projectiles` ci-dessus, cf. leur doc respective.
            creature_shots: self
                .projectiles
                .creature_shots
                .iter()
                .map(|s| crate::net::protocol::CreatureShotState {
                    position: s.pos.to_array(),
                    dir: s.dir.to_array(),
                    cfg: s.cfg as u8,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paliers nommés (GDD §8.2) : niveau plat `1 + xp/1000`, bannière au
    /// franchissement seul, le plus haut palier si plusieurs tombent d'un coup.
    #[test]
    fn palier_atteint_detects_named_milestones_only_on_crossing() {
        // 1900 XP (niv. 2) + 200 → 2100 (niv. 3) : palier 3.
        assert_eq!(palier_atteint(1900, 200), Some(3));
        // Déjà niveau 3 : rejouer sans franchir ne refête pas.
        assert_eq!(palier_atteint(2100, 200), None);
        // Gros rattrapage 0 → 6000 (niv. 1 → 7) : paliers 3 ET 6 franchis,
        // on annonce le plus haut.
        assert_eq!(palier_atteint(0, 6000), Some(6));
        // Niv. 10 : 8900 + 200 → 9100 (niv. 10).
        assert_eq!(palier_atteint(8900, 200), Some(10));
        // Aucun gain : jamais de bannière.
        assert_eq!(palier_atteint(1999, 0), None);
        // Au-delà du dernier palier nommé : plus rien à annoncer.
        assert_eq!(palier_atteint(9500, 5000), None);
    }

    use glam::Vec3;

    fn app_with_zombies_demo() -> AppState {
        let mut app = AppState::new();
        app.load_zombies_demo();
        app
    }

    /// Régression (cf. AUDIT_MMORPG.md) : `hide_local_player_template` masque le gabarit
    /// avant le premier join, pour qu'aucun objet ne soit désigné « le
    /// joueur ». Sans exclure explicitement les monstres (`ai_chaser`/
    /// `combat.attackable`, qui portent aussi un script) du repli « premier
    /// objet scripté visible » de `player_index`, un monstre était désigné
    /// « le joueur » dès que les monstres devenaient visibles (manche 1) — les
    /// monstres se déclenchaient alors entre eux et vidaient `hud_health`
    /// (partagé) en quelques secondes, sans le moindre joueur connecté.
    #[test]
    fn waiting_for_the_first_player_never_drains_health_via_monster_scripts() {
        let mut app = app_with_zombies_demo();
        app.hide_local_player_template();
        app.playing = true;
        for _ in 0..80 {
            app.perf.last_frame =
                std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert_eq!(
            app.hud_health, None,
            "sans joueur connecté, aucun script de monstre ne doit pouvoir affecter la vie"
        );
        assert!(
            !app.is_lost(),
            "la manche ne doit jamais se perdre toute seule en attendant"
        );
    }

    #[test]
    fn spawning_a_network_player_adds_a_new_controllable_object() {
        let mut app = app_with_zombies_demo();
        let before = app.scene.objects.len();

        let index = app
            .spawn_network_player(1, PlayerClass::Assault)
            .expect("la démo zombies a un gabarit pilotable");

        assert_eq!(app.scene.objects.len(), before + 1);
        assert_eq!(app.network_player_object(1), Some(index));
        assert_eq!(app.network_player_count(), 1);
        assert!(app.scene.objects[index].controller.is_some());
    }

    #[test]
    fn two_network_players_get_independent_objects_and_ids() {
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let b = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        assert_ne!(a, b, "chaque joueur doit avoir son propre objet");
        assert_eq!(app.network_player_count(), 2);
    }

    /// GDD §3.2 : « L'Assaut reproduit exactement les valeurs actuelles —
    /// zéro régression pour qui ne choisit pas ». Le gabarit cloné pour un
    /// Assaut doit garder exactement la vitesse du gabarit d'origine.
    #[test]
    fn assault_class_keeps_the_templates_move_speed_unmodified() {
        let mut app = app_with_zombies_demo();
        let template_speed = app
            .scene
            .objects
            .iter()
            .find_map(|o| o.controller.as_ref())
            .expect("gabarit pilotable")
            .move_speed;

        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let spawned_speed = app.scene.objects[index]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;
        assert_eq!(
            spawned_speed, template_speed,
            "Assaut ne doit modifier ni la vitesse ni rien d'autre"
        );
    }

    /// GDD §3.2 : Éclaireur = vitesse +25 %, PV max −30 %.
    #[test]
    fn scout_class_is_faster_and_has_less_max_health_than_assault() {
        let mut assault_app = app_with_zombies_demo();
        let assault_idx = assault_app
            .spawn_network_player(1, PlayerClass::Assault)
            .unwrap();
        let assault_speed = assault_app.scene.objects[assault_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        let mut scout_app = app_with_zombies_demo();
        let scout_idx = scout_app
            .spawn_network_player(1, PlayerClass::Scout)
            .unwrap();
        let scout_speed = scout_app.scene.objects[scout_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        assert!(
            scout_speed > assault_speed,
            "l'Éclaireur doit être plus rapide que l'Assaut : {scout_speed} <= {assault_speed}"
        );
        assert_eq!(
            scout_app.network_player_health(1),
            Some(crate::app::health::MAX_HEALTH * 0.70),
            "l'Éclaireur doit démarrer à 70 % des PV max de base"
        );
    }

    /// GDD §3.2 : Soutien = vitesse −15 %, mais PV max inchangés (seuls les
    /// dégâts infligés et la vitesse sont réduits, pas l'endurance).
    #[test]
    fn support_class_is_slower_but_keeps_full_max_health() {
        let mut assault_app = app_with_zombies_demo();
        let assault_idx = assault_app
            .spawn_network_player(1, PlayerClass::Assault)
            .unwrap();
        let assault_speed = assault_app.scene.objects[assault_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        let mut support_app = app_with_zombies_demo();
        let support_idx = support_app
            .spawn_network_player(1, PlayerClass::Support)
            .unwrap();
        let support_speed = support_app.scene.objects[support_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        assert!(
            support_speed < assault_speed,
            "le Soutien doit être plus lent que l'Assaut : {support_speed} >= {assault_speed}"
        );
        assert_eq!(
            support_app.network_player_health(1),
            Some(crate::app::health::MAX_HEALTH),
            "le Soutien garde ses PV max pleins"
        );
    }

    /// GDD §8.1 (14 septembre 2026 au soir) : Cendre = vitesse −25 %, PV max
    /// +60 % — le mur du groupe, il encaisse plus et se déplace moins vite.
    #[test]
    fn tank_class_is_slower_and_has_more_max_health_than_assault() {
        let mut assault_app = app_with_zombies_demo();
        let assault_idx = assault_app
            .spawn_network_player(1, PlayerClass::Assault)
            .unwrap();
        let assault_speed = assault_app.scene.objects[assault_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        let mut tank_app = app_with_zombies_demo();
        let tank_idx = tank_app.spawn_network_player(1, PlayerClass::Tank).unwrap();
        let tank_speed = tank_app.scene.objects[tank_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        assert!(
            tank_speed < assault_speed,
            "Cendre doit être plus lent que l'Assaut : {tank_speed} >= {assault_speed}"
        );
        assert_eq!(
            tank_app.network_player_health(1),
            Some(crate::app::health::MAX_HEALTH * 1.60),
            "Cendre doit démarrer à 160 % des PV max de base"
        );
    }

    /// GDD §8.1 : Brasier = vitesse +10 %, PV max −25 % — rapide et fragile,
    /// à l'opposé de Cendre.
    #[test]
    fn berserker_class_is_faster_and_has_less_max_health_than_assault() {
        let mut assault_app = app_with_zombies_demo();
        let assault_idx = assault_app
            .spawn_network_player(1, PlayerClass::Assault)
            .unwrap();
        let assault_speed = assault_app.scene.objects[assault_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        let mut berserker_app = app_with_zombies_demo();
        let berserker_idx = berserker_app
            .spawn_network_player(1, PlayerClass::Berserker)
            .unwrap();
        let berserker_speed = berserker_app.scene.objects[berserker_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        assert!(
            berserker_speed > assault_speed,
            "Brasier doit être plus rapide que l'Assaut : {berserker_speed} <= {assault_speed}"
        );
        assert_eq!(
            berserker_app.network_player_health(1),
            Some(crate::app::health::MAX_HEALTH * 0.75),
            "Brasier doit démarrer à 75 % des PV max de base"
        );
    }

    /// GDD §8.1 : le bouclier renforcé de Cendre (`block_damage_mult` 0,10)
    /// absorbe 90 % des dégâts, contre 75 % pour les autres classes — un vrai
    /// mur, pas juste une variante du bouclier universel.
    #[test]
    fn tank_class_reduces_blocked_damage_far_more_than_the_universal_shield() {
        let mut app = app_with_zombies_demo();
        let idx1 = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        app.spawn_network_player(2, PlayerClass::Tank).unwrap();
        app.network.network_inputs.get_mut(&2).unwrap().block = true;
        let before = app.network_player_health(2).unwrap();
        app.apply_network_damage(
            2,
            0.1,
            crate::net::protocol::DeathCauseKind::Player,
            idx1,
        );
        let lost = before - app.network_player_health(2).unwrap();
        assert!(
            (lost - 0.1 * 0.10).abs() < 1e-5,
            "Cendre bouclier tenu doit absorber 90 % des dégâts, {lost} perdus au lieu de {}",
            0.1 * 0.10
        );
    }

    /// GDD §8.1 : le bouclier de Brasier ne réduit plus rien
    /// (`block_damage_mult` 1.0) — la contrepartie de son ×1,4 en mêlée.
    #[test]
    fn berserker_class_shield_does_not_reduce_damage_at_all() {
        let mut app = app_with_zombies_demo();
        let idx1 = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        app.spawn_network_player(2, PlayerClass::Berserker).unwrap();
        app.network.network_inputs.get_mut(&2).unwrap().block = true;
        let before = app.network_player_health(2).unwrap();
        app.apply_network_damage(
            2,
            0.1,
            crate::net::protocol::DeathCauseKind::Player,
            idx1,
        );
        let lost = before - app.network_player_health(2).unwrap();
        assert!(
            (lost - 0.1).abs() < 1e-5,
            "Brasier bouclier tenu doit encaisser les dégâts pleins, {lost} perdus au lieu de 0.1"
        );
    }

    /// GDD §8.1 (rééquilibré le 15 septembre 2026) : dégâts de contact PvP
    /// ×1,4 pour Brasier, cadence d'attaque +18 % (recharge à 0,85×) — les
    /// deux effets de `update_network_attacks`, pas seulement le
    /// multiplicateur brut testé isolément ci-dessus.
    #[test]
    fn berserker_class_hits_harder_and_recharges_faster_in_pvp() {
        let mut scene = crate::scene::Scene {
            ability_bar: true,
            ..Default::default()
        };
        scene.objects.push(crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(glam::Vec3::ZERO)
                .with_scale(glam::Vec3::new(40.0, 1.0, 40.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(glam::Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        let mut app = AppState::new();
        app.scene = scene;
        app.playing = true;
        let attacker = app
            .spawn_network_player(1, PlayerClass::Berserker)
            .unwrap();
        let target = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        app.scene.objects[target].transform.position = app.scene.objects[attacker]
            .transform
            .position;
        // Grâce d'apparition à zéro : sans ça, la cible fraîchement spawnée
        // serait protégée du PvP par `SPAWN_GRACE_S` et ce test mesurerait
        // zéro dégât au lieu du multiplicateur de Brasier (cf. `in_grace`
        // dans `update_network_attacks`).
        app.network.network_spawn_grace.insert(2, 0.0);
        app.network.network_inputs.get_mut(&1).unwrap().attack = true;
        let before = app.network_player_health(2).unwrap();
        app.update_network_attacks(1.0 / 60.0);
        let lost = before - app.network_player_health(2).unwrap();
        assert!(
            (lost - super::PVP_MELEE_DAMAGE * 1.4).abs() < 1e-5,
            "Brasier doit infliger ×1,4 les dégâts de contact PvP, {lost} au lieu de {}",
            super::PVP_MELEE_DAMAGE * 1.4
        );
        let cooldown = app.network.network_attack_cooldowns[&1];
        assert!(
            (cooldown - super::NETWORK_ATTACK_COOLDOWN * 0.85).abs() < 1e-5,
            "Brasier doit recharger à 0,85× le temps universel, {cooldown} au lieu de {}",
            super::NETWORK_ATTACK_COOLDOWN * 0.85
        );
    }

    /// GDD §8.1 (15 septembre 2026) : Givre = PV max −35 %, vitesse
    /// inchangée — la classe la plus fragile du roster, sans la compensation
    /// de mobilité de l'Éclaireur (0,70).
    #[test]
    fn sniper_class_has_the_lowest_max_health_and_unchanged_speed() {
        let mut assault_app = app_with_zombies_demo();
        let assault_idx = assault_app
            .spawn_network_player(1, PlayerClass::Assault)
            .unwrap();
        let assault_speed = assault_app.scene.objects[assault_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        let mut sniper_app = app_with_zombies_demo();
        let sniper_idx = sniper_app
            .spawn_network_player(1, PlayerClass::Sniper)
            .unwrap();
        let sniper_speed = sniper_app.scene.objects[sniper_idx]
            .controller
            .as_ref()
            .unwrap()
            .move_speed;

        assert_eq!(
            sniper_speed, assault_speed,
            "Givre ne modifie pas la vitesse de déplacement"
        );
        assert_eq!(
            sniper_app.network_player_health(1),
            Some(crate::app::health::MAX_HEALTH * 0.65),
            "Givre doit démarrer à 65 % des PV max de base"
        );
    }

    /// GDD §8.1 : `ranged_damage_mult`/`melee_damage_mult`/
    /// `ranged_lifetime_mult` de Givre, vérifiés directement — Givre est la
    /// seule classe à dépasser ×1,0 à distance, payé par la pénalité de
    /// mêlée la plus sévère du roster (dans ce sens) et compensé par une
    /// portée de tir étendue.
    #[test]
    fn sniper_class_multipliers_match_the_glass_cannon_design() {
        assert_eq!(PlayerClass::Sniper.ranged_damage_mult(), 1.50);
        assert_eq!(PlayerClass::Sniper.melee_damage_mult(), 0.5);
        assert_eq!(PlayerClass::Sniper.ranged_lifetime_mult(), 1.25);
        assert_eq!(PlayerClass::Sniper.attack_cooldown_mult(), 1.0);
        assert_eq!(
            PlayerClass::Sniper.block_damage_mult(),
            crate::app::health::BLOCK_DAMAGE_MULT
        );
        assert!(!PlayerClass::Sniper.can_revive());
        // Aucune autre classe ne dépasse ×1,0 à distance (niche unique de
        // Givre, cf. recon).
        for class in PlayerClass::ALL {
            if class != PlayerClass::Sniper {
                assert!(
                    class.ranged_damage_mult() <= 1.0,
                    "{class:?} ne devrait pas dépasser ×1,0 à distance"
                );
            }
        }
    }

    /// GDD §8.1 : dégâts de contact PvP réduits de moitié pour Givre — même
    /// structure que `berserker_class_hits_harder_and_recharges_faster_in_pvp`
    /// (bout en bout via `update_network_attacks`), cadence inchangée.
    #[test]
    fn sniper_class_hits_softer_in_melee_pvp() {
        let mut scene = crate::scene::Scene {
            ability_bar: true,
            ..Default::default()
        };
        scene.objects.push(crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(glam::Vec3::ZERO)
                .with_scale(glam::Vec3::new(40.0, 1.0, 40.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(glam::Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        let mut app = AppState::new();
        app.scene = scene;
        app.playing = true;
        let attacker = app.spawn_network_player(1, PlayerClass::Sniper).unwrap();
        let target = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        app.scene.objects[target].transform.position =
            app.scene.objects[attacker].transform.position;
        app.network.network_spawn_grace.insert(2, 0.0);
        app.network.network_inputs.get_mut(&1).unwrap().attack = true;
        let before = app.network_player_health(2).unwrap();
        app.update_network_attacks(1.0 / 60.0);
        let lost = before - app.network_player_health(2).unwrap();
        assert!(
            (lost - super::PVP_MELEE_DAMAGE * 0.5).abs() < 1e-5,
            "Givre doit infliger ×0,5 les dégâts de contact PvP, {lost} au lieu de {}",
            super::PVP_MELEE_DAMAGE * 0.5
        );
        let cooldown = app.network.network_attack_cooldowns[&1];
        assert!(
            (cooldown - super::NETWORK_ATTACK_COOLDOWN).abs() < 1e-5,
            "Givre garde la cadence universelle, {cooldown} au lieu de {}",
            super::NETWORK_ATTACK_COOLDOWN
        );
    }

    /// `PlayerClass::from_u8` (décodage du réseau) : une valeur hors table ne
    /// doit jamais faire paniquer le serveur, elle retombe sur Assaut — même
    /// principe que `fireball::clamp_weapon` pour un indice d'arme invalide.
    #[test]
    fn player_class_from_u8_falls_back_to_assault_for_unknown_values() {
        assert_eq!(PlayerClass::from_u8(0), PlayerClass::Assault);
        assert_eq!(PlayerClass::from_u8(1), PlayerClass::Scout);
        assert_eq!(PlayerClass::from_u8(2), PlayerClass::Support);
        assert_eq!(PlayerClass::from_u8(3), PlayerClass::Tank);
        assert_eq!(PlayerClass::from_u8(4), PlayerClass::Berserker);
        assert_eq!(PlayerClass::from_u8(5), PlayerClass::Sniper);
        assert_eq!(PlayerClass::from_u8(250), PlayerClass::Assault);
    }

    /// Ajouter une classe à `PlayerClass::ALL` n'est **pas** forcé par le
    /// compilateur (contrairement aux matches exhaustifs par variante) — ce
    /// test échoue explicitement si une classe existe sans y figurer, au lieu
    /// de la laisser silencieusement absente du sélecteur UI et des boucles
    /// de test qui itèrent `ALL` (14 septembre 2026 au soir, ajout de
    /// Cendre/Brasier).
    #[test]
    fn player_class_all_lists_every_known_class() {
        assert_eq!(PlayerClass::ALL.len(), 6);
    }

    /// Sprint 3 (`sprint10audit.md`) : `to_u8` doit être l'inverse exact de
    /// `from_u8` pour toutes les classes — c'est ce qui garantit que la classe
    /// choisie dans le sélecteur (fenêtre Multijoueur) arrive intacte côté
    /// serveur via `ClientMsg::Join::class`.
    #[test]
    fn player_class_to_u8_round_trips_through_from_u8() {
        for class in PlayerClass::ALL {
            assert_eq!(PlayerClass::from_u8(class.to_u8()), class);
        }
    }

    /// Phase C (Sprint 5, `sprint10audit.md`) : même garantie que
    /// `player_class_from_u8_falls_back_to_assault_for_unknown_values`/
    /// `player_class_to_u8_round_trips_through_from_u8` ci-dessus, pour
    /// `RoundObjective` — une valeur hors table (`ClientMsg::Join::objective`
    /// forgée) ne doit jamais faire paniquer le serveur, et `to_u8` doit rester
    /// l'inverse exact de `from_u8` pour les quatre modes.
    #[test]
    fn round_objective_from_u8_falls_back_to_vagues_for_unknown_values() {
        assert_eq!(RoundObjective::from_u8(0), RoundObjective::Vagues);
        assert_eq!(RoundObjective::from_u8(1), RoundObjective::Survie);
        assert_eq!(RoundObjective::from_u8(2), RoundObjective::Escorte);
        assert_eq!(RoundObjective::from_u8(3), RoundObjective::Boss);
        assert_eq!(RoundObjective::from_u8(250), RoundObjective::Vagues);
    }

    #[test]
    fn round_objective_to_u8_round_trips_through_from_u8() {
        for objective in RoundObjective::ALL {
            assert_eq!(RoundObjective::from_u8(objective.to_u8()), objective);
        }
    }

    /// Phase D (Sprint 9, `sprint10audit.md`) : même garantie round-trip que
    /// `RoundObjective`/`PlayerClass` — une valeur hors table ne doit jamais
    /// paniquer, `to_u8`/`from_u8` doivent rester inverses l'un de l'autre.
    #[test]
    fn contract_to_u8_round_trips_through_from_u8() {
        for contract in Contract::ALL {
            assert_eq!(Contract::from_u8(contract.to_u8()), contract);
        }
        assert_eq!(Contract::from_u8(250), Contract::NuitBlanche);
    }

    /// Le contrat du jour est déterministe (même jour ⇒ même contrat, cf. GDD
    /// §3.4 « calculé identiquement par serveur et clients ») et parcourt tout
    /// le catalogue au fil des jours plutôt que de retomber toujours sur le
    /// même (`% ALL.len()` couvre les 4 valeurs sur des jours consécutifs).
    #[test]
    fn contract_of_day_is_deterministic_and_cycles_through_the_catalogue() {
        assert_eq!(Contract::of_day(100), Contract::of_day(100));
        let seen: std::collections::HashSet<Contract> = (0..Contract::ALL.len() as u64)
            .map(Contract::of_day)
            .collect();
        assert_eq!(
            seen.len(),
            Contract::ALL.len(),
            "4 jours consécutifs doivent couvrir les 4 contrats du catalogue"
        );
    }

    /// *Nuit blanche* : un seul `PlayerDown` sur la manche suffit à invalider
    /// le contrat, même si la manche est par ailleurs gagnée.
    #[test]
    fn nuit_blanche_fails_as_soon_as_one_player_goes_down() {
        let mut app = AppState::new();
        assert!(app.contract_completed(Contract::NuitBlanche, std::time::Duration::ZERO));
        app.player_down_count = 1;
        assert!(!app.contract_completed(Contract::NuitBlanche, std::time::Duration::ZERO));
    }

    /// *À l'aube juste* : seul le mode Horde (`Vagues`) compte, et seulement
    /// sous 8 minutes — un autre mode ou une manche trop longue ne remplit
    /// jamais ce contrat, quelle que soit sa durée/son mode par ailleurs.
    #[test]
    fn aube_juste_requires_vagues_under_eight_minutes() {
        let mut app = AppState::new();
        app.objective = RoundObjective::Vagues;
        assert!(
            app.contract_completed(Contract::AubeJuste, std::time::Duration::from_secs(7 * 60))
        );
        assert!(
            !app.contract_completed(Contract::AubeJuste, std::time::Duration::from_secs(9 * 60))
        );

        app.objective = RoundObjective::Survie;
        assert!(
            !app.contract_completed(Contract::AubeJuste, std::time::Duration::from_secs(60)),
            "un autre mode que Vagues (Horde) ne peut jamais remplir ce contrat"
        );
    }

    /// *La lande garde ses morts* : la moindre réanimation achevée invalide le
    /// contrat, même une seule sur toute la manche.
    #[test]
    fn lande_garde_ses_morts_fails_as_soon_as_one_revive_completes() {
        let mut app = AppState::new();
        assert!(app.contract_completed(Contract::LandeGardeSesMorts, std::time::Duration::ZERO));
        app.revives_completed = 1;
        assert!(!app.contract_completed(Contract::LandeGardeSesMorts, std::time::Duration::ZERO));
    }

    /// *Le troupeau compte sur vous* : nécessite le mode Escorte et un convoi
    /// à plus de 50 % de ses PV ; un convoi jamais touché (`max_hp == 0`,
    /// jamais capturé) compte comme trivialement au-dessus du seuil.
    #[test]
    fn troupeau_compte_sur_vous_requires_escorte_and_convoy_above_half_hp() {
        let mut app = AppState::new();
        app.objective = RoundObjective::Escorte;
        app.scene = crate::scene::Scene {
            objects: vec![crate::scene::SceneObject {
                convoy: Some(crate::scene::Convoy::default()),
                combat: Some(crate::scene::Combat {
                    attackable: true,
                    hp: 8,
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(
            app.contract_completed(Contract::TroupeauCompteSurVous, std::time::Duration::ZERO),
            "convoi jamais touché (max_hp=0) ⇒ trivialement au-dessus de 50 %"
        );

        app.scene.objects[0].combat.as_mut().unwrap().max_hp = 8;
        app.scene.objects[0].combat.as_mut().unwrap().hp = 3;
        assert!(
            !app.contract_completed(Contract::TroupeauCompteSurVous, std::time::Duration::ZERO),
            "3/8 PV est sous le seuil de 50 %"
        );

        app.objective = RoundObjective::Vagues;
        app.scene.objects[0].combat.as_mut().unwrap().hp = 8;
        assert!(
            !app.contract_completed(Contract::TroupeauCompteSurVous, std::time::Duration::ZERO),
            "un mode autre qu'Escorte ne peut jamais remplir ce contrat"
        );
    }

    /// Régression : rien dans le protocole
    /// n'empêche un client d'envoyer un second `Join` (rejeu, bug client, trame
    /// forgée — cf. `net::server_loop::handle_connection`, qui ne borne que la
    /// *première* trame à être un `Join`). Avant le correctif, ce second appel
    /// spawnait un objet fantôme supplémentaire sans jamais nettoyer le
    /// premier : plus référencé par `network_players` (invisible du `Snapshot`
    /// réseau), mais simulé indéfiniment par la physique.
    #[test]
    fn spawning_twice_for_the_same_player_reuses_the_existing_object() {
        let mut app = app_with_zombies_demo();
        let before = app.scene.objects.len();

        let first = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let second = app.spawn_network_player(1, PlayerClass::Assault).unwrap();

        assert_eq!(
            first, second,
            "un second Join du même joueur doit renvoyer le même objet, pas en créer un autre"
        );
        assert_eq!(
            app.scene.objects.len(),
            before + 1,
            "un seul objet doit avoir été ajouté à la scène, pas un fantôme par Join répété"
        );
        assert_eq!(app.network_player_count(), 1);
    }

    #[test]
    fn despawning_hides_the_object_and_forgets_the_player() {
        let mut app = app_with_zombies_demo();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();

        app.despawn_network_player(1);

        assert_eq!(app.network_player_object(1), None);
        assert_eq!(app.network_player_count(), 0);
        assert!(
            !app.scene.objects[index].visible,
            "l'objet doit rester en place (indices stables) mais devenir invisible"
        );
    }

    /// Sans recyclage, chaque
    /// `Join` pousse un nouveau clone dans `scene.objects`, pour toujours —
    /// un salon de longue durée (beaucoup de va-et-vient) grossit sans borne
    /// et alourdit chaque `Physics::build` (reconstruit à chaque join/leave),
    /// perçu en jeu comme des à-coups/blocages de mouvement. Vérifie qu'un
    /// nouveau joueur qui rejoint APRÈS le départ d'un précédent réutilise
    /// son emplacement au lieu d'en créer un troisième.
    #[test]
    fn a_new_player_reuses_a_slot_left_by_a_departed_one_instead_of_growing_the_scene() {
        let mut app = app_with_zombies_demo();
        let before = app.scene.objects.len();

        let first = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        assert_eq!(app.scene.objects.len(), before + 1);
        app.despawn_network_player(1);

        let second = app.spawn_network_player(2, PlayerClass::Assault).unwrap();

        assert_eq!(
            second, first,
            "le second joueur doit récupérer l'emplacement laissé par le premier"
        );
        assert_eq!(
            app.scene.objects.len(),
            before + 1,
            "la scène ne doit pas grossir tant qu'il ne dépasse jamais 1 joueur simultané"
        );
        assert!(
            app.scene.objects[second].visible,
            "l'emplacement recyclé doit redevenir visible pour son nouveau joueur"
        );
    }

    /// Complète le test précédent : avec deux joueurs simultanés, un 3e qui
    /// rejoint pendant que les deux premiers sont encore là doit bien obtenir
    /// un nouvel objet (rien à recycler) — le recyclage ne doit jamais faire
    /// partager un objet à deux joueurs connectés en même temps.
    #[test]
    fn simultaneous_players_never_share_a_recycled_slot() {
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let b = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        assert_ne!(a, b);

        app.despawn_network_player(1);
        // Pendant que 2 est toujours connecté, 3 rejoint : doit récupérer
        // l'emplacement de 1 (le seul orphelin), jamais celui de 2.
        let c = app.spawn_network_player(3, PlayerClass::Assault).unwrap();

        assert_eq!(
            c, a,
            "le 3e joueur doit recycler l'emplacement du 1er, parti"
        );
        assert_ne!(c, b, "jamais l'emplacement d'un joueur encore connecté");
        assert_eq!(app.network_player_count(), 2);
    }

    /// Régression AUDIT_MMORPG.md §4.2 : `restart_game` remet `scene.objects` à
    /// l'état d'avant Play (qui ne connaît pas les joueurs réseau, spawnés en
    /// cours de partie) — `network_players` doit être oublié en même temps,
    /// sinon il continuerait de pointer vers des indices obsolètes.
    #[test]
    fn restart_game_forgets_network_players() {
        let mut app = app_with_zombies_demo();
        app.play_snapshot = app.scene.objects.clone();
        app.spawn_network_player(1, PlayerClass::Assault);
        assert_eq!(app.network_player_count(), 1);

        app.restart_game();

        assert_eq!(
            app.network_player_count(),
            0,
            "un restart doit oublier les joueurs réseau, pas garder des indices obsolètes"
        );
        assert_eq!(app.network_player_object(1), None);
    }

    #[test]
    fn setting_input_for_an_unknown_player_is_a_no_op() {
        let mut app = app_with_zombies_demo();
        // Ne doit pas paniquer même si `id` n'a jamais rejoint (message tardif
        // après une déconnexion, par exemple).
        app.set_network_input(
            999,
            NetworkInput {
                move_x: 1.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
                block: false,
                dash: false,
            },
        );
        assert_eq!(app.network_player_count(), 0);
    }

    #[test]
    fn network_input_moves_the_players_own_object_independently() {
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let b = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        let start_a = app.scene.objects[a].transform.position;
        let start_b = app.scene.objects[b].transform.position;

        // Le joueur 1 avance en +X, le joueur 2 reste immobile (input par défaut).
        app.set_network_input(
            1,
            NetworkInput {
                move_x: 1.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
                block: false,
                dash: false,
            },
        );
        app.playing = true;
        for _ in 0..30 {
            app.perf.last_frame =
                std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }

        let end_a = app.scene.objects[a].transform.position;
        let end_b = app.scene.objects[b].transform.position;
        // Distance horizontale (X/Z) seulement : les deux capsules tombent et se
        // stabilisent sous la gravité (léger mouvement vertical + un peu de glissement
        // à l'atterrissage), sans rapport avec l'input — seul le déplacement
        // horizontal du joueur 1 doit refléter son `NetworkInput`.
        let horiz = |a: Vec3, b: Vec3| ((b.x - a.x).powi(2) + (b.z - a.z).powi(2)).sqrt();
        let moved_a = horiz(start_a, end_a);
        let moved_b = horiz(start_b, end_b);
        assert!(
            moved_a > 1.0,
            "le joueur 1 doit s'être nettement déplacé horizontalement : {start_a:?} -> {end_a:?}"
        );
        assert!(
            moved_b < moved_a * 0.5,
            "le joueur 2 (input par défaut) ne doit pas suivre le déplacement du joueur 1 : \
             joueur 1 = {moved_a:.2} m, joueur 2 = {moved_b:.2} m"
        );
    }

    /// Reproduction locale (diagnostic 15 septembre 2026, sonde
    /// `examples/probe_combat_riviere.rs` contre le vrai serveur) : rejoue
    /// LOCALEMENT le scénario incriminé (joueur réseau spawné sur
    /// `Scene::riviere_demo()`, donc recalé en hauteur par double raycast,
    /// cf. `spawn_network_player`, puis `NetworkInput` continu en +X — même
    /// entrée constante que `examples/probe_riviere_walk_only.rs`).
    ///
    /// Verdict de ce test : l'hypothèse initiale (le recalage de hauteur au
    /// spawn laisse le joueur pénétré dans le terrain et neutralise tout
    /// mouvement horizontal dès le premier pas) est INFIRMÉE — `grounded`
    /// est déjà `true` juste après le spawn et le joueur avance normalement
    /// dès le tick 0 (cf. les assertions ci-dessous).
    ///
    /// AVANT le correctif du 15 septembre 2026 (`place()` dans
    /// `src/scene/demos/riviere.rs` posait les arbres solides avec un
    /// collider `Auto`), le déplacement s'arrêtait net à `horiz≈2,729 m`
    /// (`(9.135, 24.687)` — position stable au mm près, reproduite à
    /// l'identique par la sonde live `examples/probe_riviere_walk_only.rs`
    /// contre `wss://ws.loicberthod.ch`) : un rayon horizontal depuis ce
    /// point touchait le collider `Auto` (`cuboid()` déduit de l'AABB du
    /// mesh importé — tronc ET houppier, cf. `Physics::build`) de l'« Arbre
    /// 638 » voisin (`riviere_demo`, RNG déterministe) à `horiz≈2,995 m` de
    /// son centre. `KinematicCharacterController` (`slide: true`) immobilise
    /// alors le joueur, sans aucune reprise malgré 170 pas simulés
    /// supplémentaires (~8,5 s) sous entrée continue — le "gel réseau"
    /// observé en production.
    ///
    /// APRÈS le correctif (`place()` pose désormais les arbres solides avec
    /// un collider `TriMesh`, silhouette exacte), le même « Arbre 638 »
    /// bloque le joueur plus loin (`horiz≈2,933 m` du spawn, soit
    /// `horiz≈2,704 m` de son centre au lieu de `2,995 m` avec `Auto`) :
    /// confirmé, mesuré directement (bascule locale et re-run du test), que
    /// `TriMesh` colle bien plus près à la géométrie visible que le
    /// `cuboid()` qu'il remplace. Un seuil bas (`> 2.8`, entre les deux
    /// valeurs mesurées) détecte toute régression qui repasserait ce même
    /// arbre en `Auto`.
    ///
    /// Ce test ne prétend PAS que le joueur traverse toute la ceinture
    /// d'arbres proches du spawn en ligne droite sans jamais s'arrêter : la
    /// forêt y est délibérément dense (cf. la doc du module) et un bot qui
    /// n'entre JAMAIS de composante latérale finit toujours par heurter un
    /// tronc réel, `TriMesh` ou pas — ce n'est pas un bug (`slide: true`
    /// annule alors bien la composante de vitesse qui entre dans l'obstacle,
    /// comportement attendu d'un `KinematicCharacterController`). La preuve
    /// robuste, indépendante de la trajectoire choisie, que plus AUCUN arbre
    /// solide n'utilise le collider `Auto` oversized est
    /// `riviere_demo_solid_forest_decor_uses_exact_silhouette_collider`
    /// (`src/scene/demos/riviere.rs`).
    #[test]
    fn network_input_moves_the_player_on_riviere_demo() {
        let mut app = AppState::new();
        app.load_riviere_demo();
        let a = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let start_a = app.scene.objects[a].transform.position;
        assert_eq!(
            app.physics.as_ref().map(|p| p.is_grounded(a)),
            Some(true),
            "au spawn (recalé en hauteur), le joueur doit déjà être détecté au sol"
        );

        app.set_network_input(
            1,
            NetworkInput {
                move_x: 1.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
                block: false,
                dash: false,
            },
        );
        app.playing = true;

        // Un seul pas suffit à infirmer « aucun mouvement dès le premier
        // tick » : si le recalage de spawn neutralisait bien tout
        // déplacement horizontal comme l'hypothèse le prévoyait, ce premier
        // pas serait ~0.
        app.perf.last_frame = std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        let after_one_tick = app.scene.objects[a].transform.position;
        assert!(
            (after_one_tick.x - start_a.x).abs() > 1e-3,
            "le joueur doit déjà avoir bougé après UN seul pas simulé : {start_a:?} -> {after_one_tick:?}"
        );

        // ~9 s au total (180 pas x 0.05 s) : largement au-delà des ~4 s qui
        // menaient au blocage contre l'Arbre 638, avec ou sans le correctif —
        // confirme qu'il n'y a pas de reprise tardive (ni avant ni après le
        // correctif) une fois le contact établi, cohérent avec un `slide`
        // normal contre un obstacle réel plutôt qu'un gel intrinsèque au
        // contrôleur.
        for _ in 0..179 {
            app.perf.last_frame =
                std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }

        let end_a = app.scene.objects[a].transform.position;
        let horiz = ((end_a.x - start_a.x).powi(2) + (end_a.z - start_a.z).powi(2)).sqrt();
        assert!(
            horiz > 2.8,
            "avec le correctif TriMesh, le joueur doit franchir la distance à laquelle le \
             collider Auto (avant correctif) l'arrêtait (horiz≈2,729 m contre l'Arbre 638) : \
             {start_a:?} -> {end_a:?} (horiz={horiz:.4})"
        );
    }

    /// Sweep (angles 0..7 du cercle de spawn réseau, cf. `spawn_network_player`)
    /// reproduisant `Room::for_world(Riviere)` du binaire serveur
    /// (`clear_spawn_area` + `hide_local_player_template` + `playing=true`
    /// avant le premier spawn, cf. `src/bin/server.rs`) : chaque joueur
    /// réseau doit progresser sur les tout premiers ticks, quel que soit
    /// l'angle de spawn — ce que l'hypothèse « recalage de hauteur = gel
    /// systématique » prédirait faux pour TOUS les angles. Ne vérifie que
    /// les 6 premiers ticks (~0,3 s) : au-delà, certains angles mènent tout
    /// droit dans la forêt dense et butent légitimement sur un arbre (cf.
    /// la doc de `network_input_moves_the_player_on_riviere_demo`) — ce
    /// test ne porte donc que sur l'amorce du mouvement, pas sur sa durée.
    #[test]
    fn network_movement_starts_immediately_at_every_riviere_spawn_angle() {
        let mut app = AppState::new();
        app.load_riviere_demo();
        app.clear_spawn_area();
        app.hide_local_player_template();
        app.playing = true;

        let mut indices = Vec::new();
        for id in 1..=8u32 {
            let idx = app.spawn_network_player(id, PlayerClass::Assault).unwrap();
            assert_eq!(
                app.physics.as_ref().map(|p| p.is_grounded(idx)),
                Some(true),
                "joueur {id} (idx {idx}) : pas au sol juste après le recalage de spawn"
            );
            indices.push((id, idx, app.scene.objects[idx].transform.position));
        }
        for (id, ..) in &indices {
            app.set_network_input(
                *id,
                NetworkInput {
                    move_x: 1.0,
                    move_y: 0.3,
                    aim_yaw: 0.0,
                    attack: false,
                    jump: false,
                    fire: false,
                    weapon: 0,
                    heal: false,
                    block: false,
                    dash: false,
                },
            );
        }
        for _ in 0..6 {
            app.perf.last_frame =
                std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        for (id, idx, start) in indices {
            let end = app.scene.objects[idx].transform.position;
            let horiz = ((end.x - start.x).powi(2) + (end.z - start.z).powi(2)).sqrt();
            assert!(
                horiz > 0.02,
                "joueur {id} (idx {idx}) : aucun mouvement horizontal amorcé en 6 pas \
                 ({start:?} -> {end:?}, horiz={horiz:.4})"
            );
        }
    }

    #[test]
    fn network_snapshot_reports_every_connected_player() {
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let b = app.spawn_network_player(2, PlayerClass::Assault).unwrap();

        let snap = app.network_snapshot(7);
        assert_eq!(snap.tick, 7);
        let indices: Vec<u32> = snap.entities.iter().map(|e| e.index).collect();
        assert!(indices.contains(&(a as u32)));
        assert!(indices.contains(&(b as u32)));
    }

    /// Phase L Sprint 3 (`sprint2audijeu0718.md`) : `network_assists` était déjà
    /// calculé (`credit_assists_on_kill`) mais jamais diffusé — le HUD des
    /// autres clients ne pouvait afficher que les frags. Preuve que
    /// `EntityDelta::assists` reflète bien le compteur serveur, comme `kills`.
    #[test]
    fn network_snapshot_reports_each_players_live_assist_count() {
        let mut app = app_with_zombies_demo();
        app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        app.network.network_assists.insert(1, 2);

        let snap = app.network_snapshot(1);
        let entity = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .expect("le joueur 1 doit apparaître dans le snapshot");
        assert_eq!(entity.assists, Some(2));

        // Joueur 2, sans assist : `Some(0)`, pas `None` — même politique que
        // `kills` (`Some` pour toute entité-joueur, cf. la doc du champ).
        let entity_b = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(2))
            .expect("le joueur 2 doit apparaître dans le snapshot");
        assert_eq!(entity_b.assists, Some(0));
    }

    /// v7 (silhouettes de classe, GDD §10.3) : la classe de chaque joueur
    /// part dans son `EntityDelta` — sans elle, un client ne peut pas
    /// teinter les fantômes des autres.
    #[test]
    fn network_snapshot_reports_the_player_class_for_silhouettes() {
        let mut app = app_with_zombies_demo();
        app.spawn_network_player(1, PlayerClass::Scout).unwrap();
        let snap = app.network_snapshot(1);
        let e = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .expect("le joueur 1 doit figurer dans le snapshot");
        assert_eq!(e.class, Some(PlayerClass::Scout.to_u8()));
    }

    /// La silhouette repart toujours de la base neutre mémorisée : appliquer
    /// deux fois (snapshots successifs, reconnexion) ne compose jamais les
    /// facteurs, et changer de classe repart du gabarit d'origine.
    #[test]
    fn apply_class_silhouette_is_idempotent_from_the_neutral_base() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Fantôme".into(),
            ..Default::default()
        });
        let base_scale = app.scene.objects[0].transform.scale;

        app.apply_class_silhouette(0, PlayerClass::Support);
        app.apply_class_silhouette(0, PlayerClass::Support);
        let s = app.scene.objects[0].transform.scale;
        assert!(
            (s.x - base_scale.x * PlayerClass::Support.silhouette_scale()).abs() < 1e-6,
            "double application = un seul facteur (obtenu {s:?})"
        );

        app.apply_class_silhouette(0, PlayerClass::Scout);
        let s2 = app.scene.objects[0].transform.scale;
        assert!(
            (s2.x - base_scale.x * PlayerClass::Scout.silhouette_scale()).abs() < 1e-6,
            "changer de classe repart de la base neutre (obtenu {s2:?})"
        );
        assert_eq!(
            app.scene.objects[0].color,
            PlayerClass::Scout.silhouette_tint(),
            "teinte appliquée depuis la couleur de base [1,1,1]"
        );
    }

    #[test]
    fn network_snapshot_reports_the_player_animation_clip() {
        // Le clip joué par un joueur réseau (s'il a un
        // `AnimationState`) doit atterrir dans son `EntityDelta`, pour que les
        // autres écrans jouent la même animation sur son fantôme.
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        app.scene.objects[a].animation = Some(crate::scene::AnimationState {
            clip: "run".into(),
            time: 0.0,
            speed: 1.0,
            prev_clip: String::new(),
            prev_time: 0.0,
            blend: 1.0,
            locomotion: None,
        });

        let snap = app.network_snapshot(1);
        let entity = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .expect("le joueur 1 doit figurer dans le snapshot");
        assert_eq!(entity.anim_clip, "run");
    }

    #[test]
    fn network_snapshot_leaves_anim_clip_empty_without_animation_state() {
        // Un joueur sans `AnimationState` (mesh non skinné) ne doit pas planter
        // ni inventer un clip — champ vide, comme documenté.
        let mut app = app_with_zombies_demo();
        app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let snap = app.network_snapshot(1);
        let entity = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .expect("le joueur 1 doit figurer dans le snapshot");
        assert!(entity.anim_clip.is_empty());
    }

    #[test]
    fn spawned_players_stay_visible_even_after_hiding_the_local_template() {
        // Reproduit l'ordre réel du serveur headless (`src/bin/server.rs`) :
        // `hide_local_player_template` masque le gabarit *avant* le premier
        // join. Sans le correctif (`template.visible = true` dans
        // `spawn_network_player`), chaque joueur réseau héritait de ce
        // `visible=false` — invisible pour toujours dans le `Snapshot`, malgré
        // des positions correctement transmises (cf. docs/audits/app-network.md
        // pour le bug réel que ça a causé).
        let mut app = app_with_zombies_demo();
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        assert!(
            app.scene.objects[index].visible,
            "un joueur réseau tout juste spawné doit être visible, \
             même si le gabarit d'origine a été masqué avant son arrivée"
        );
        let snap = app.network_snapshot(1);
        let entity = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .expect("le joueur 1 doit figurer dans le snapshot");
        assert!(
            entity.visible,
            "le snapshot diffusé doit refléter visible=true"
        );
    }

    /// 14 septembre 2026 au soir : un coup/sort d'un joueur réseau doit
    /// s'animer côté serveur (donc chez les autres joueurs via
    /// `EntityDelta::anim_clip`), et jouer en entier même sur un appui bref.
    #[test]
    fn a_network_players_brief_attack_press_plays_the_attack_clip_for_all() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::default();
        app.scene.ability_bar = true;
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Héros".into(),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            animation: Some(crate::scene::AnimationState::default()),
            ..Default::default()
        });
        let idx = app.spawn_network_player(7, PlayerClass::Assault).expect("gabarit pilotable");
        let clip = |app: &AppState| app.scene.objects[idx].animation.as_ref().unwrap().clip.clone();
        let input = NetworkInput {
            attack: true,
            ..Default::default()
        };
        app.set_network_input(7, input);
        app.playing = true;
        // Pas de simulation complets : l'élection Walk/Idle de fin de tick ne
        // doit pas écraser le clip de capacité (cf. `ability_anim_locked`).
        app.sim_step(1.0 / 60.0);
        assert_eq!(clip(&app), "Attack", "appui : le clip Attack part");
        let anim_time = app.scene.objects[idx].animation.as_ref().unwrap().time;
        // Touche relâchée dès le tick suivant : le clip continue le temps du
        // minuteur (et avance, pas figé sur sa première image), puis s'arrête.
        app.set_network_input(7, NetworkInput::default());
        app.sim_step(1.0 / 60.0);
        app.sim_step(1.0 / 60.0);
        assert_eq!(clip(&app), "Attack", "relâché : le clip joue encore");
        assert!(
            app.scene.objects[idx].animation.as_ref().unwrap().time > anim_time,
            "le clip doit avancer d'un tick à l'autre"
        );
        for _ in 0..40 {
            app.sim_step(1.0 / 60.0);
        }
        assert_eq!(clip(&app), "Idle", "minuteur écoulé : Walk/Idle reprend la main");
        // Une fois le minuteur écoulé, la fonction ne touche plus au clip
        // (Walk/Idle reprennent la main dans `sim_step`).
        let t = app.network.network_ability_anims[&7];
        assert_eq!(t.swing_remaining, 0.0);
        assert_eq!(t.current, None);
        let snap = app.network_snapshot(1);
        assert!(snap.entities.iter().any(|e| e.player_id == Some(7)));
    }

    /// Jauges au-dessus des têtes (14 septembre 2026 au soir) : la vie des
    /// monstres part normalisée dans le `Snapshot`, avant comme après un coup.
    #[test]
    fn the_snapshot_carries_normalized_monster_health() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::default();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Monstre".into(),
            combat: Some(crate::scene::Combat {
                attackable: true,
                hp: 4,
                ..Default::default()
            }),
            ..Default::default()
        });
        let h = |app: &AppState| app.network_snapshot(0).entities[0].health;
        assert_eq!(h(&app), Some(1.0), "jamais touché : plein");
        app.scene.damage_attackable_by(0, 1);
        assert_eq!(h(&app), Some(0.75));
        app.scene.damage_attackable_by(0, 3);
        assert_eq!(h(&app), Some(0.0), "vaincu : diffusé à 0 (et masqué)");
    }

    /// PvP au contact : deux joueurs qui se touchent (≈ 2,1 m centre à
    /// centre, cf. `PVP_MELEE_RANGE`) se blessent ; à 3,5 m, non.
    #[test]
    fn pvp_melee_reaches_a_player_standing_body_to_body_but_not_one_step_further() {
        for (dist, expect_hit) in [(2.2_f32, true), (3.5_f32, false)] {
            let mut app = AppState::new();
            app.scene = crate::scene::Scene::default();
            app.scene.ability_bar = true;
            app.scene.objects.push(crate::scene::SceneObject {
                name: "Héros".into(),
                controller: Some(crate::scene::Controller {
                    input: true,
                    ..Default::default()
                }),
                ..Default::default()
            });
            let i1 = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
            let i2 = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
            app.scene.objects[i1].transform.position = glam::Vec3::ZERO;
            app.scene.objects[i2].transform.position = glam::Vec3::new(dist, 0.0, 0.0);
            app.network.network_spawn_grace.insert(2, 0.0);
            app.set_network_input(
                1,
                NetworkInput {
                    attack: true,
                    ..Default::default()
                },
            );
            app.update_network_attacks(1.0 / 60.0);
            let hp = app.network_player_health(2).unwrap();
            assert_eq!(hp < 1.0, expect_hit, "distance {dist} m : vie de la cible {hp}");
        }
    }

    #[test]
    fn sanitize_replaces_non_finite_axes_with_zero() {
        let dirty = NetworkInput {
            move_x: f32::NAN,
            move_y: f32::INFINITY,
            aim_yaw: 0.0,
            attack: true,
            jump: true,
            fire: false,
            weapon: 0,
            heal: false,
            block: false,
            dash: false,
        };
        let clean = sanitize_network_input(dirty);
        assert_eq!(clean.move_x, 0.0);
        assert_eq!(clean.move_y, 0.0);
        // Les booléens ne sont pas concernés par le nettoyage numérique.
        assert!(clean.attack);
        assert!(clean.jump);
    }

    #[test]
    fn sanitize_clamps_axes_to_unit_range() {
        let dirty = NetworkInput {
            move_x: 50.0,
            move_y: -50.0,
            aim_yaw: 0.0,
            attack: false,
            jump: false,
            fire: false,
            weapon: 0,
            heal: false,
            block: false,
            dash: false,
        };
        let clean = sanitize_network_input(dirty);
        assert_eq!(clean.move_x, 1.0);
        assert_eq!(clean.move_y, -1.0);
    }

    #[test]
    fn a_nan_input_from_the_network_never_corrupts_the_players_position() {
        // Un client modifié pourrait envoyer des octets produisant un NaN (pas
        // nécessairement un client légitime passant par des sliders bornés) :
        // vérifie que la position reste finie après plusieurs pas de simulation.
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        app.set_network_input(
            1,
            NetworkInput {
                move_x: f32::NAN,
                move_y: f32::NAN,
                aim_yaw: 0.0,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
                block: false,
                dash: false,
            },
        );
        app.playing = true;
        for _ in 0..10 {
            app.perf.last_frame =
                std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let pos = app.scene.objects[a].transform.position;
        assert!(
            pos.is_finite(),
            "un NaN reçu du réseau ne doit jamais corrompre la position : {pos:?}"
        );
    }

    /// `aim_yaw` est normalisé dans `(-π, π]` dès la réception : un f32 fini
    /// mais énorme (1e30 — il passait l'ancien filtre `is_finite`) n'a plus
    /// aucune précision utile et dégradait l'orientation diffusée à tous les
    /// clients ; un yaw honnête (issu de `to_euler`) traverse inchangé.
    #[test]
    fn sanitize_normalizes_huge_yaw_values() {
        use std::f32::consts::{PI, TAU};
        let sanitized_yaw = |yaw: f32| {
            sanitize_network_input(NetworkInput {
                move_x: 0.0,
                move_y: 0.0,
                aim_yaw: yaw,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
                block: false,
                dash: false,
            })
            .aim_yaw
        };
        for huge in [1e30f32, -400.0, 1e9, f32::MAX] {
            let y = sanitized_yaw(huge);
            assert!(
                y.is_finite() && y > -PI - 1e-4 && y <= PI + 1e-4,
                "yaw {huge} doit être ramené dans (-π, π], obtenu {y}"
            );
        }
        // Valeur exacte vérifiable : 7,0 rad = un tour complet + 0,717 rad.
        let y = sanitized_yaw(7.0);
        assert!(
            (y - (7.0 - TAU)).abs() < 1e-5,
            "7,0 rad doit devenir 7,0 − 2π ≈ 0,717, obtenu {y}"
        );
        // Un yaw honnête (déjà dans (-π, π]) traverse inchangé.
        for honest in [0.0f32, 1.57, -3.0, PI] {
            let y = sanitized_yaw(honest);
            assert!(
                (y - honest).abs() < 1e-5,
                "yaw honnête {honest} ne doit pas être altéré, obtenu {y}"
            );
        }
        // Et l'invariant historique : NaN/infini → 0 (le neutre).
        assert_eq!(sanitized_yaw(f32::NAN), 0.0);
        assert_eq!(sanitized_yaw(f32::INFINITY), 0.0);
    }

    /// Construit une scène minimale : un joueur pilotable au centre, et deux
    /// cibles `attackable` à portée immédiate (cf. `NETWORK_ATTACK_RANGE`) —
    /// de quoi vérifier que le temps de recharge serveur limite bien le nombre
    /// de cibles vaincues par unité de temps, indépendamment de ce qu'un client
    /// prétend envoyer.
    fn scene_with_player_and_two_targets_in_range() -> crate::scene::Scene {
        use crate::scene::{Combat, Controller, MeshKind, Scene, SceneObject, Transform};

        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Joueur".into(),
            transform: Transform::from_pos(Vec3::ZERO),
            controller: Some(Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        for i in 0..2 {
            scene.objects.push(SceneObject {
                name: format!("Cible {i}"),
                mesh: MeshKind::Cube,
                transform: Transform::from_pos(Vec3::new(0.3, 0.0, 0.0)),
                combat: Some(Combat {
                    attackable: true,
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        scene
    }

    #[test]
    fn server_rejects_attack_before_cooldown_elapsed() {
        let mut app = AppState::new();
        app.scene = scene_with_player_and_two_targets_in_range();
        let player_index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        // Les deux cibles sont à portée du point d'apparition du joueur réseau
        // (décalé de +5 en X par `spawn_network_player`) : replace-les au même
        // décalage pour rester dans `NETWORK_ATTACK_RANGE`.
        let player_pos = app.scene.objects[player_index].transform.position;
        for o in app.scene.objects.iter_mut() {
            if o.combat.is_some() {
                o.transform.position = player_pos;
            }
        }
        app.playing = true;
        app.set_network_input(
            1,
            NetworkInput {
                move_x: 0.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: true,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
                block: false,
                dash: false,
            },
        );

        // Plusieurs ticks rapprochés (bien en-deçà de NETWORK_ATTACK_COOLDOWN) :
        // un client qui spamme `attack: true` ne doit vaincre qu'UNE cible.
        for _ in 0..5 {
            app.perf.last_frame =
                std::time::Instant::now() - std::time::Duration::from_secs_f32(0.02);
            app.advance_play();
        }
        let defeated_early: usize = app
            .scene
            .objects
            .iter()
            .filter(|o| o.combat.is_some() && !o.visible)
            .count();
        assert_eq!(
            defeated_early, 1,
            "le temps de recharge doit limiter à une seule cible vaincue malgré le spam"
        );

        // Après le temps de recharge, une nouvelle attaque doit passer.
        for _ in 0..15 {
            app.perf.last_frame =
                std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let defeated_later: usize = app
            .scene
            .objects
            .iter()
            .filter(|o| o.combat.is_some() && !o.visible)
            .count();
        assert_eq!(
            defeated_later, 2,
            "une fois le temps de recharge écoulé, la seconde cible doit tomber aussi"
        );
    }

    /// Non-régression (15 septembre 2026) : `update_network_attacks` résolvait
    /// le corps-à-corps réseau via `Scene::attack_at`, qui masquait la cible
    /// d'un coup sans jamais lire/décrémenter `Combat::hp` — un monstre à
    /// plusieurs PV (le Golem des berges, 6 PV ; le boss « L'Aîné de la
    /// Cascade », 60 PV) tombait donc en un seul coup de mêlée réseau, alors
    /// que le même monstre encaisse bien ses PV un par un en solo (cf.
    /// `AppState::update_attack`/`damage_attackable`). Preuve ici avec une
    /// cible à 3 PV : un coup de mêlée réseau ne doit l'entamer que d'un
    /// point, pas la vaincre — il en faut trois pour l'achever.
    #[test]
    fn network_melee_needs_several_hits_on_a_multi_hp_target_not_just_one() {
        let mut app = AppState::new();
        app.scene = scene_with_player_and_two_targets_in_range();
        for o in app.scene.objects.iter_mut() {
            if let Some(combat) = o.combat.as_mut() {
                combat.hp = 3;
            }
        }
        let player_index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let player_pos = app.scene.objects[player_index].transform.position;
        let target_index = app
            .scene
            .objects
            .iter()
            .position(|o| o.combat.is_some())
            .expect("cible attackable absente");
        app.scene.objects[target_index].transform.position = player_pos;
        app.set_network_input(
            1,
            NetworkInput {
                attack: true,
                ..Default::default()
            },
        );

        // Premier coup : entame la cible sans l'achever.
        app.update_network_attacks(1.0 / 60.0);
        assert!(
            app.scene.objects[target_index].visible,
            "une cible à 3 PV ne doit pas tomber en un seul coup de mêlée réseau"
        );
        assert_eq!(app.scene.objects[target_index].combat.as_ref().unwrap().hp, 2);

        // Recharge écoulée : deuxième coup, entame encore.
        app.network.network_attack_cooldowns.insert(1, 0.0);
        app.update_network_attacks(1.0 / 60.0);
        assert!(app.scene.objects[target_index].visible);
        assert_eq!(app.scene.objects[target_index].combat.as_ref().unwrap().hp, 1);

        // Troisième coup : achève enfin la cible.
        app.network.network_attack_cooldowns.insert(1, 0.0);
        app.update_network_attacks(1.0 / 60.0);
        assert!(
            !app.scene.objects[target_index].visible,
            "le troisième coup doit achever une cible à 3 PV"
        );
    }

    /// Brique de progression pour un futur MMORPG (GAMEDESIGN_EN_LIGNE.md) : un
    /// monstre vaincu au contact crédite le tireur d'un frag individualisé,
    /// diffusé à tous via `EntityDelta::kills` (pas seulement au joueur
    /// concerné — voir le score de chacun est un vrai signal compétitif en
    /// coopératif).
    #[test]
    fn a_contact_kill_credits_the_attacking_players_kill_count() {
        let mut app = AppState::new();
        app.scene = scene_with_player_and_two_targets_in_range();
        let player_index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let player_pos = app.scene.objects[player_index].transform.position;
        for o in app.scene.objects.iter_mut() {
            if o.combat.is_some() {
                o.transform.position = player_pos;
            }
        }
        app.playing = true;
        assert_eq!(app.network_player_kills(1), Some(0));
        app.set_network_input(
            1,
            NetworkInput {
                move_x: 0.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: true,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
                block: false,
                dash: false,
            },
        );

        app.perf.last_frame = std::time::Instant::now() - std::time::Duration::from_secs_f32(0.02);
        app.advance_play();

        assert_eq!(
            app.network_player_kills(1),
            Some(1),
            "vaincre un monstre au contact doit créditer le tireur d'un frag"
        );
        let snap = app.network_snapshot(1);
        let entity = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .expect("le joueur 1 doit figurer dans le snapshot");
        assert_eq!(
            entity.kills,
            Some(1),
            "le frag doit être diffusé dans le Snapshot, pas seulement connu localement"
        );
    }

    /// Sprint 4 (PHASE B, `sprint10audit.md`) : deux joueurs endommagent la même
    /// créature, celui qui ne l'achève pas reçoit quand même de l'XP d'assist —
    /// jamais compté comme un frag (compteurs séparés, `network_kills` /
    /// `network_assists`).
    #[test]
    fn damaging_a_creature_that_another_player_finishes_off_credits_an_assist_not_a_kill() {
        let mut app = AppState::new();
        app.scene = scene_with_player_and_two_targets_in_range();
        app.spawn_network_player(1, PlayerClass::Assault);
        app.spawn_network_player(2, PlayerClass::Assault);
        assert_eq!(app.network_player_assists(1), Some(0));
        assert_eq!(app.network_player_assists(2), Some(0));

        const CREATURE: usize = 42;
        app.record_damage_contribution(CREATURE, 1);
        app.credit_assists_on_kill(CREATURE, 2);
        app.credit_kill(2);

        assert_eq!(
            app.network_player_assists(1),
            Some(1),
            "le joueur qui a blessé la cible sans l'achever doit recevoir un assist"
        );
        assert_eq!(
            app.network_player_kills(1),
            Some(0),
            "pas de frag pour l'assist"
        );
        assert_eq!(
            app.network_player_assists(2),
            Some(0),
            "le tireur qui achève la cible ne se crédite pas lui-même d'un assist"
        );
        assert_eq!(
            app.network_player_kills(2),
            Some(1),
            "le tireur qui achève la cible reçoit le frag"
        );
    }

    /// Une contribution de dégâts trop ancienne (au-delà de `ASSIST_WINDOW`) ne
    /// doit plus ouvrir droit à un assist — sans cette borne, un dégât porté en
    /// tout début de manche créditerait un assist sans lien réel avec un kill
    /// survenu bien plus tard.
    #[test]
    fn a_damage_contribution_older_than_the_assist_window_does_not_count() {
        let mut app = AppState::new();
        app.scene = scene_with_player_and_two_targets_in_range();
        app.spawn_network_player(1, PlayerClass::Assault);
        app.spawn_network_player(2, PlayerClass::Assault);

        const CREATURE: usize = 7;
        app.record_damage_contribution(CREATURE, 1);
        app.time += ASSIST_WINDOW + 1.0;
        app.credit_assists_on_kill(CREATURE, 2);

        assert_eq!(
            app.network_player_assists(1),
            Some(0),
            "une contribution trop ancienne ne doit plus créditer d'assist"
        );
    }

    /// L'historique de contributions d'une créature doit être purgé après
    /// résolution de sa mort, qu'un assist ait été crédité ou non — sinon sa
    /// prochaine vie (après respawn, même indice d'objet) hériterait à tort
    /// des dégâts de la vie précédente.
    #[test]
    fn credit_assists_on_kill_clears_the_creatures_contribution_history() {
        let mut app = AppState::new();
        app.scene = scene_with_player_and_two_targets_in_range();
        app.spawn_network_player(1, PlayerClass::Assault);
        app.spawn_network_player(2, PlayerClass::Assault);

        const CREATURE: usize = 3;
        app.record_damage_contribution(CREATURE, 1);
        app.credit_assists_on_kill(CREATURE, 2);
        assert_eq!(app.network_player_assists(1), Some(1));

        // Nouvelle vie du même emplacement : un assist sans nouvelle
        // contribution enregistrée ne doit rien créditer de plus.
        app.credit_assists_on_kill(CREATURE, 2);
        assert_eq!(
            app.network_player_assists(1),
            Some(1),
            "l'historique de la créature doit avoir été purgé au premier kill résolu"
        );
    }
}

#[cfg(test)]
mod spawn_area_tests {
    use super::super::AppState;
    use crate::scene::{Scene, SceneObject};
    use glam::Vec3;

    /// Un chasseur posé sur le point d'apparition est repoussé hors du rayon
    /// de sécurité ; un chasseur déjà loin ne bouge pas (roadmap post-audit
    /// UX v2 2026-09-04, 0.1 bis).
    #[test]
    fn chasers_camping_the_spawn_are_pushed_away() {
        let mut app = AppState::new();
        let mut scene = Scene::demo();
        let at = |p: Vec3| crate::scene::Transform {
            position: p,
            ..Default::default()
        };
        let player = SceneObject {
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            transform: at(Vec3::new(2.0, 0.0, 3.0)),
            ..Default::default()
        };
        let near = SceneObject {
            ai_chaser: Some(Default::default()),
            transform: at(Vec3::new(2.5, 0.0, 3.0)),
            ..Default::default()
        };
        let far = SceneObject {
            ai_chaser: Some(Default::default()),
            transform: at(Vec3::new(40.0, 0.0, 3.0)),
            ..Default::default()
        };
        scene.objects.push(player);
        scene.objects.push(near);
        scene.objects.push(far);
        app.scene = scene;
        let moved = app.clear_spawn_area();
        assert_eq!(moved, 1);
        let n = app.scene.objects.len();
        let near_pos = app.scene.objects[n - 2].transform.position;
        let far_pos = app.scene.objects[n - 1].transform.position;
        assert!((near_pos - Vec3::new(2.0, 0.0, 3.0)).length() >= 10.0);
        assert_eq!(far_pos, Vec3::new(40.0, 0.0, 3.0));
    }
}
