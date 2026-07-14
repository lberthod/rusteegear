//! Requêtes de gameplay sur la scène : AABB monde/locale, zones mortelles,
//! ramassage (pièces, armes), combat (portée d'attaque, dégâts), lumières les
//! plus proches. Extrait de `scene/mod.rs`.

use glam::Vec3;

use super::{MeshKind, Scene, SceneObject, TapAction, WEAPONS, Weapon};

impl Scene {
    /// Estimation grossière de l'occupation mémoire (octets) : `(objets, meshes importés,
    /// nombre de textures uniques)`. Pour le profiler mémoire (ordre de grandeur).
    pub fn memory_estimate(&self) -> (usize, usize, usize) {
        let mut obj_bytes = self.objects.len() * std::mem::size_of::<SceneObject>();
        let mut textures = std::collections::BTreeSet::new();
        for o in &self.objects {
            let audio_len = o.audio.as_ref().map_or(0, |a| a.clip.len());
            obj_bytes += o.name.len() + o.script.len() + o.texture.len() + audio_len;
            if !o.texture.is_empty() {
                textures.insert(o.texture.as_str());
            }
        }
        let vsize = std::mem::size_of::<crate::gfx::mesh::Vertex>();
        let mesh_bytes: usize = self
            .imported
            .iter()
            .map(|m| m.data.vertices.len() * vsize + m.data.indices.len() * 4)
            .sum();
        (obj_bytes, mesh_bytes, textures.len())
    }

    /// AABB local d'un objet (primitive codée ou mesh importé).
    pub fn local_aabb(&self, mesh: MeshKind) -> (Vec3, Vec3) {
        match mesh {
            MeshKind::Cube | MeshKind::Sphere => (Vec3::splat(-0.5), Vec3::splat(0.5)),
            MeshKind::Plane => (Vec3::new(-0.5, -0.02, -0.5), Vec3::new(0.5, 0.02, 0.5)),
            MeshKind::Cylinder => (Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5)),
            MeshKind::Capsule => (Vec3::new(-0.25, -0.5, -0.25), Vec3::new(0.25, 0.5, 0.25)),
            MeshKind::Terrain => (Vec3::new(-0.5, -0.1, -0.5), Vec3::new(0.5, 0.1, 0.5)),
            MeshKind::Imported(i) => {
                let m = &self.imported[i as usize];
                (m.aabb_min, m.aabb_max)
            }
        }
    }

    /// AABB monde de l'objet `o` (AABB local transformé, ré-englobé axe-aligné).
    pub fn world_aabb(&self, o: &SceneObject) -> (Vec3, Vec3) {
        let (lmin, lmax) = self.local_aabb(o.mesh);
        let m = o.transform.matrix();
        let mut wmin = Vec3::splat(f32::INFINITY);
        let mut wmax = Vec3::splat(f32::NEG_INFINITY);
        for sx in [lmin.x, lmax.x] {
            for sy in [lmin.y, lmax.y] {
                for sz in [lmin.z, lmax.z] {
                    let q = (m * Vec3::new(sx, sy, sz).extend(1.0)).truncate();
                    wmin = wmin.min(q);
                    wmax = wmax.max(q);
                }
            }
        }
        (wmin, wmax)
    }

    /// Le point monde `p` est-il dans l'AABB monde de l'objet `o` ?
    pub fn world_aabb_contains(&self, o: &SceneObject, p: Vec3) -> bool {
        let (wmin, wmax) = self.world_aabb(o);
        p.cmpge(wmin).all() && p.cmple(wmax).all()
    }

    /// Les AABB monde de `a` et `b` se chevauchent-ils ? Contrairement à
    /// `world_aabb_contains` (test d'un *point*), ce test réussit dès le *contact* des
    /// volumes : indispensable quand les deux objets ont un corps physique, car les
    /// colliders empêchent alors le centre de l'un d'entrer dans l'AABB de l'autre.
    pub fn world_aabb_intersects(&self, a: &SceneObject, b: &SceneObject) -> bool {
        let (amin, amax) = self.world_aabb(a);
        let (bmin, bmax) = self.world_aabb(b);
        amin.cmple(bmax).all() && bmin.cmple(amax).all()
    }

    /// Le point `p` (position du joueur) touche-t-il une zone mortelle ?
    pub fn deadly_at(&self, p: Vec3) -> bool {
        self.objects
            .iter()
            .filter(|o| o.deadly)
            .any(|o| self.world_aabb_contains(o, p))
    }

    /// Ramassage par contact : masque (collecte) les collectibles encore visibles dont
    /// le centre est à moins de `radius` (+ leur rayon) du point `p` (position du joueur).
    /// Renvoie les **indices** des pièces ramassées cette frame (pour score + respawn).
    pub fn collect_at(&mut self, p: Vec3, radius: f32) -> Vec<usize> {
        let mut hit = Vec::new();
        for (i, o) in self.objects.iter_mut().enumerate() {
            if o.tap_action == TapAction::Hide && o.visible {
                let piece_r = o.transform.scale.max_element() * 0.5;
                if (o.transform.position - p).length() <= radius + piece_r {
                    o.visible = false;
                    hit.push(i);
                }
            }
        }
        hit
    }

    /// Ramassage d'arme au contact (cf. `WeaponPickup`) : masque le premier butin touché
    /// et renvoie son profil (`WEAPONS[weapon]`) — un seul par appel, contrairement à
    /// `collect_at` qui peut en ramasser plusieurs d'un coup (équiper 2 armes à la fois
    /// n'aurait pas de sens, contrairement à empocher 2 pièces).
    pub fn weapon_pickup_at(&mut self, p: Vec3, radius: f32) -> Option<Weapon> {
        for o in &mut self.objects {
            if let Some(wp) = o.weapon_pickup
                && o.visible
            {
                let piece_r = o.transform.scale.max_element() * 0.5;
                if (o.transform.position - p).length() <= radius + piece_r {
                    o.visible = false;
                    return Some(WEAPONS[wp.weapon]);
                }
            }
        }
        None
    }

    /// Résout une attaque du joueur en `p` (portée `radius`) : vainc (masque) les ennemis
    /// `attackable` encore visibles à portée. Renvoie les indices vaincus (pour score,
    /// son, et mise en file de réapparition côté `App`, comme les bonus).
    /// Ne vainc que **la cible la plus proche** à portée, pas toutes celles dans le
    /// rayon. Audit gameplay : un swing en zone (toutes les cibles à portée à la fois)
    /// laissait un groupe de monstres convergeant ensemble se faire vaincre d'un seul
    /// coup avant qu'aucun n'ait pu mordre — la taille des monstres (donc leur propre
    /// rayon de mise à mort) compense presque exactement leur vitesse, ce qui les fait
    /// arriver à portée de façon quasi synchronisée plutôt qu'échelonnée. Un coup =
    /// une cible force à revenir au corps-à-corps plusieurs fois pour vider un groupe,
    /// laissant une vraie fenêtre aux autres pendant la recharge.
    pub fn attack_at(&mut self, p: Vec3, radius: f32) -> Vec<usize> {
        match self.nearest_attackable(p, radius) {
            Some(i) => {
                self.objects[i].visible = false;
                vec![i]
            }
            None => Vec::new(),
        }
    }

    /// Frappe de zone (cf. `AttackMode::Zone`) : vainc (masque) TOUTES les cibles
    /// `attackable` encore visibles à portée d'un coup, contrairement à `attack_at` qui
    /// n'en vainc qu'une (cf. sa doc : un swing en zone par défaut trivialise un groupe
    /// convergent). Réservée aux armes qui l'assument explicitement via un coût élevé
    /// (préparation/recharge longues, cf. `Weapon::mode` — le Marteau) : le compromis
    /// change selon l'arme équipée, pas selon un swing par défaut universel.
    /// Ne renvoie que les cibles **vaincues** par ce coup (cf. `damage_attackable`) : une
    /// cible à plusieurs points de vie touchée mais encore vivante n'apparaît pas dans le
    /// résultat (elle reste visible, sans recul — le recul en zone n'a pas de direction
    /// unique à appliquer par cible, contrairement au mode `Single`, cf. `AppState`).
    pub fn attack_zone_at(&mut self, p: Vec3, radius: f32) -> Vec<usize> {
        let targets: Vec<usize> = self
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.combat.as_ref().is_some_and(|c| c.attackable) && o.visible)
            .filter(|(_, o)| {
                let enemy_r = o.transform.scale.max_element() * 0.5;
                (o.transform.position - p).length() - enemy_r <= radius
            })
            .map(|(i, _)| i)
            .collect();
        targets
            .into_iter()
            .filter(|&i| self.damage_attackable(i))
            .collect()
    }

    /// Inflige un coup à la cible `i` (décompte `Combat.hp`) : la masque et renvoie
    /// `true` si ce coup l'achève (hp tombé à 0), renvoie `false` si elle survit (hp
    /// encore > 0, reste visible). Distingue un coup qui achève d'un coup qui blesse
    /// seulement — nécessaire pour un duel à plusieurs points de vie (cf.
    /// `Scene::brawl_demo`), impossible à exprimer avec l'ancien `attack_at`/`attack_zone_at`
    /// (masquage immédiat, sans notion de PV restants).
    pub fn damage_attackable(&mut self, i: usize) -> bool {
        self.damage_attackable_by(i, 1)
    }

    /// Comme `damage_attackable`, mais retire `amount` points de vie d'un coup —
    /// pour les armes lourdes (cf. `app::fireball::RangedWeapon::damage`), sans
    /// boucler N fois sur un décompte unitaire.
    pub fn damage_attackable_by(&mut self, i: usize, amount: u32) -> bool {
        let Some(o) = self.objects.get_mut(i) else {
            return false;
        };
        let Some(c) = &mut o.combat else {
            return false;
        };
        c.hp = c.hp.saturating_sub(amount);
        if c.hp == 0 {
            o.visible = false;
            true
        } else {
            false
        }
    }

    /// Cible la plus proche à portée, **sans la vaincre** (contrairement à `attack_at`) :
    /// utilisé pour verrouiller la cible d'un missile au moment du tir (cf.
    /// `AppState::attack_projectile`), l'impact réel étant résolu plus tard, à l'arrivée.
    pub fn nearest_attackable(&self, p: Vec3, radius: f32) -> Option<usize> {
        self.objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.combat.as_ref().is_some_and(|c| c.attackable) && o.visible)
            .map(|(i, o)| {
                let enemy_r = o.transform.scale.max_element() * 0.5;
                (i, (o.transform.position - p).length() - enemy_r)
            })
            .filter(|&(_, dist)| dist <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// État des **pièces-objectif** (action « Masquer », **non** réapparaissantes) :
    /// `Some((ramassées, total))` si la scène en contient, sinon `None`. Les pièces bonus
    /// (`respawn_delay > 0`) ne comptent pas. `ramassées == total` ⇒ niveau gagné.
    pub fn collectibles(&self) -> Option<(usize, usize)> {
        let goal = |o: &&SceneObject| o.tap_action == TapAction::Hide && o.respawn_delay == 0.0;
        let total = self.objects.iter().filter(goal).count();
        if total == 0 {
            return None;
        }
        let collected = self
            .objects
            .iter()
            .filter(goal)
            .filter(|o| !o.visible)
            .count();
        Some((collected, total))
    }

    /// Sélectionne les indices des `max` lumières ponctuelles les **plus proches** de
    /// `cam` (culling/LOD de lumières : seules les plus pertinentes sont envoyées au
    /// shader quand la scène en compte plus que la limite). Ordre : de la plus proche
    /// à la plus éloignée. Si le nombre de lumières ≤ `max`, les renvoie toutes dans
    /// l'ordre d'origine (aucun tri).
    pub fn nearest_point_lights(&self, cam: Vec3, max: usize) -> Vec<usize> {
        let n = self.point_lights.len();
        if n <= max {
            return (0..n).collect();
        }
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| {
            let da = (Vec3::from(self.point_lights[a].position) - cam).length_squared();
            let db = (Vec3::from(self.point_lights[b].position) - cam).length_squared();
            da.total_cmp(&db)
        });
        idx.truncate(max);
        idx
    }
}
