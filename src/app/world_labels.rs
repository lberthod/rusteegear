//! Jauges de vie **au-dessus des personnages** (14 septembre 2026 au soir,
//! demande « rajoute des points de vie / jauge de vie sur les personnages et
//! permet le PvP ») : instantané par frame des entités qui portent une vie —
//! autres joueurs réseau (`RemotePlayer::health`, diffusée par le serveur) et
//! monstres (`Combat::hp`, diffusé normalisé via `EntityDelta::health` en ligne,
//! lu localement en solo) — projeté à l'écran par `editor::hud::world_health_labels`.
//! Le joueur local garde sa grande barre de vie en haut de l'écran.
//!
//! Un **flash** blanc signale chaque perte de vie (`hit_flash`) : détecté ici
//! par comparaison avec la vie vue au tick précédent, sans toucher aux chemins
//! de dégâts (solo comme en ligne, mêlée comme tir).

use glam::Vec3;

use super::AppState;

/// Nature de l'entité étiquetée (couleur/gabarit de la jauge).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelKind {
    Player,
    Monster,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldLabel {
    /// Point monde au-dessus de la tête (haut de l'AABB + marge).
    pub anchor: Vec3,
    /// Pseudo (joueurs) ou nom de type sans son suffixe d'instance (monstres,
    /// cf. `type_name` — « Renard enragé 1 » s'affiche « Renard enragé »).
    pub name: String,
    /// Vie 0..1.
    pub health: f32,
    /// Texte des points de vie : « 85 » (joueur, sur 100) ou « 2/3 » (monstre).
    pub hp_text: String,
    pub kind: LabelKind,
    /// Temps restant (s) du flash de dégât, 0 = aucun.
    pub hit_flash: f32,
}

/// Nom de **type** affiché au-dessus d'un monstre (bestiaire varié, 14
/// septembre 2026 au soir) : `o.name` retire son suffixe final « espace +
/// chiffres » quand il en a un (ex. « Renard enragé 1 » → « Renard enragé »),
/// sinon renvoyé tel quel (ex. « Champignon mordeur », une espèce à instance
/// unique dans `Scene::riviere_demo`, n'a pas de suffixe à retirer). `o.name`
/// lui-même reste unique par **instance** (essentiel : `console.rs`,
/// `demos.rs`, `app::creature_attack` font tous des lookups exacts sur
/// `o.name`) — cette fonction n'affecte que l'affichage, jamais la donnée de
/// scène. Générique à toute démo (hameau MMORPG compris), pas spécifique à
/// Rivière.
fn type_name(full: &str) -> &str {
    let Some(pos) = full.rfind(' ') else {
        return full;
    };
    let suffix = &full[pos + 1..];
    if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
        &full[..pos]
    } else {
        full
    }
}

/// Distance (m) au-delà de laquelle les jauges ne sont plus dessinées
/// (lisibilité : pas une forêt de barres au loin).
const LABEL_MAX_DISTANCE: f32 = 45.0;
/// Durée (s) du flash blanc après une perte de vie.
const HIT_FLASH_S: f32 = 0.4;
/// Marge (m) au-dessus du haut de l'AABB.
const HEAD_MARGIN: f32 = 0.25;

impl AppState {
    /// Vie 0..1 de l'entité `index` si elle en porte une : fantôme d'un autre
    /// joueur réseau, ou monstre `attackable` visible. `None` sinon.
    fn labeled_health(&self, index: usize) -> Option<(f32, String, LabelKind, String)> {
        let o = self.scene.objects.get(index)?;
        if !o.visible {
            return None;
        }
        if let Some((id, rp)) = self
            .net_conn
            .remote_players
            .iter()
            .find(|(_, rp)| rp.scene_index == index)
        {
            if Some(*id) == self.net_conn.net_player_id {
                return None;
            }
            let h = rp.health.unwrap_or(1.0).clamp(0.0, 1.0);
            return Some((
                h,
                format!("{}", (h * 100.0).round() as u32),
                LabelKind::Player,
                rp.name.clone(),
            ));
        }
        if o.controller.is_some() {
            return None;
        }
        let c = o.combat.as_ref()?;
        if !c.attackable || c.is_attack_fx {
            return None;
        }
        let max = if c.max_hp > 0 { c.max_hp } else { c.hp.max(1) };
        let h = (c.hp as f32 / max as f32).clamp(0.0, 1.0);
        Some((
            h,
            format!("{}/{}", c.hp, max),
            LabelKind::Monster,
            type_name(&o.name).to_string(),
        ))
    }

    /// Jauges à dessiner cette frame (cf. la doc du module).
    pub fn world_health_labels(&self) -> Vec<WorldLabel> {
        let center = self
            .player_position()
            .unwrap_or(self.camera.target);
        let mut labels = Vec::new();
        for index in 0..self.scene.objects.len() {
            let Some((health, hp_text, kind, name)) = self.labeled_health(index) else {
                continue;
            };
            let o = &self.scene.objects[index];
            if o.transform.position.distance_squared(center) > LABEL_MAX_DISTANCE * LABEL_MAX_DISTANCE
            {
                continue;
            }
            let (_, top) = self.scene.world_aabb(o);
            let p = o.transform.position;
            labels.push(WorldLabel {
                anchor: Vec3::new(p.x, top.y + HEAD_MARGIN, p.z),
                name,
                health,
                hp_text,
                kind,
                hit_flash: self.label_hit_flash.get(&index).copied().unwrap_or(0.0),
            });
        }
        labels
    }

    /// Décompte les flashs et en arme un pour chaque entité étiquetée dont la
    /// vie a **baissé** depuis le tick précédent. Appelé à chaque pas fixe.
    pub(super) fn update_label_hit_flashes(&mut self, dt: f32) {
        self.label_hit_flash.retain(|_, t| {
            *t -= dt;
            *t > 0.0
        });
        let mut seen: Vec<(usize, f32)> = Vec::new();
        for index in 0..self.scene.objects.len() {
            if let Some((h, _, _, _)) = self.labeled_health(index) {
                seen.push((index, h));
            }
        }
        for (index, h) in seen {
            if let Some(prev) = self.label_last_health.get(&index)
                && h < *prev - 1e-4
            {
                self.label_hit_flash.insert(index, HIT_FLASH_S);
            }
            self.label_last_health.insert(index, h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::AppState;
    use super::{LabelKind, type_name};
    use crate::scene::{Combat, MeshKind, Scene, SceneObject, Transform};

    /// Bestiaire varié (14 septembre 2026 au soir) : le HUD affiche un nom
    /// de **type** au-dessus des monstres, dérivé de `SceneObject::name` en
    /// retirant son éventuel suffixe d'instance — jamais en le renommant en
    /// interne (`o.name` doit rester unique par instance, cf. la doc de
    /// `type_name`).
    #[test]
    fn type_name_strips_only_a_trailing_numeric_suffix() {
        assert_eq!(type_name("Renard enragé 1"), "Renard enragé");
        assert_eq!(type_name("Créature 26"), "Créature");
        assert_eq!(type_name("Champignon mordeur"), "Champignon mordeur");
        assert_eq!(type_name("Blob des sous-bois (vert)"), "Blob des sous-bois (vert)");
        assert_eq!(type_name("Créature"), "Créature");
        assert_eq!(type_name(""), "");
    }

    fn scene_with_monster(hp: u32) -> Scene {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Joueur".into(),
            mesh: MeshKind::Capsule,
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Monstre".into(),
            mesh: MeshKind::Cube,
            transform: Transform::from_pos(glam::Vec3::new(0.0, 0.5, -4.0)),
            combat: Some(Combat {
                attackable: true,
                hp,
                ..Default::default()
            }),
            ..Default::default()
        });
        scene
    }

    #[test]
    fn a_monster_gets_a_label_above_its_head_and_a_flash_when_hurt() {
        let mut app = AppState::new();
        app.scene = scene_with_monster(3);
        let labels = app.world_health_labels();
        assert_eq!(labels.len(), 1, "un seul monstre étiqueté, jamais le joueur local");
        let l = &labels[0];
        assert_eq!(l.kind, LabelKind::Monster);
        assert_eq!(l.hp_text, "3/3");
        assert!((l.health - 1.0).abs() < 1e-6);
        assert!(l.anchor.y > 1.0, "ancre au-dessus du cube (haut à 1.0) : {}", l.anchor.y);
        assert_eq!(l.hit_flash, 0.0);

        app.update_label_hit_flashes(0.016);
        app.scene.damage_attackable_by(1, 1);
        app.update_label_hit_flashes(0.016);
        let l = &app.world_health_labels()[0];
        assert_eq!(l.hp_text, "2/3");
        assert!(l.hit_flash > 0.0, "une perte de vie arme le flash");
        for _ in 0..40 {
            app.update_label_hit_flashes(0.016);
        }
        assert_eq!(app.world_health_labels()[0].hit_flash, 0.0, "le flash s'éteint seul");
    }

    #[test]
    fn a_hidden_monster_and_a_far_one_have_no_label() {
        let mut app = AppState::new();
        app.scene = scene_with_monster(2);
        app.scene.objects[1].visible = false;
        assert!(app.world_health_labels().is_empty());
        app.scene.objects[1].visible = true;
        app.scene.objects[1].transform.position.z = -200.0;
        assert!(app.world_health_labels().is_empty(), "hors de portée d'affichage");
    }
}
