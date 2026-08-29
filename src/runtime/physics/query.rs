use super::*;

impl Physics {
    /// Vitesse linéaire (m/s) de l'objet `index` (corps dynamique **ou**
    /// kinématique, Sprint 103b), `None` s'il n'en a pas. Sert au rattrapage
    /// doux à l'arrêt de la réconciliation réseau (cf. `app::network_client`) :
    /// distinguer « joueur immobile » (on peut aligner sans gêner) de « en
    /// plein déplacement ». Un corps kinématique n'a pas de `linvel` géré par
    /// rapier — on renvoie la vitesse suivie nous-mêmes dans `KinematicState`
    /// (cf. `control_kinematic`), mise à jour à chaque appel de `control`.
    pub fn velocity(&self, index: usize) -> Option<Vec3> {
        if let Some(&(_, _, state)) = self.kinematic.iter().find(|&&(i, _, _)| i == index) {
            return Some(Vec3::new(state.hvel.x, state.vspeed, state.hvel.z));
        }
        let &(_, handle) = self.dynamic.iter().find(|&&(i, _)| i == index)?;
        let v = self.bodies.get(handle)?.linvel();
        Some(Vec3::new(v.x, v.y, v.z))
    }

    /// Force la position du corps rigide (dynamique **ou** kinématique,
    /// Sprint 103b) de l'objet `index`, sans effet s'il n'en a pas (objet
    /// statique/sans physique) — utilisé par la réconciliation réseau du
    /// joueur local (`app::network_client::apply_local_network_position`,
    /// `SPRINTNETWORK.md`).
    ///
    /// **Nécessaire, pas cosmétique** : `step` recopie la pose du corps
    /// rigide dans `scene.objects[index].transform` à *chaque* appel (sync à
    /// sens unique physique → transform, jamais l'inverse) — écrire
    /// directement dans `transform.position` sans passer par cette méthode
    /// n'a donc d'effet que pour la frame courante ; `step` l'écrase dès le
    /// tick suivant avec la position du corps rigide, resté inchangé (cf.
    /// docs/audits/physics.md pour le bug réel que ça a causé).
    /// `set_translation` fonctionne aussi bien sur un corps kinématique
    /// (téléportation directe, hors de `move_shape`) que dynamique.
    ///
    /// Sprint 103c (audit réseau après la migration 103b) : pour un corps
    /// kinématique, remet aussi `KinematicState.grounded` à `false` si le
    /// déplacement dépasse `TELEPORT_INVALIDATES_GROUND` — une vraie
    /// téléportation (respawn, gros désync) place le corps *hors* de
    /// `move_shape`, où l'état « au sol » mis en cache par le dernier
    /// `control_kinematic` n'a plus aucune raison d'être encore valable
    /// (ex. la correction retire le joueur d'une plateforme). **Pas** pour
    /// les petites corrections de réconciliation habituelles (`CORRECTION_
    /// PULL`/`IDLE_SETTLE_PULL` dans `app::network_client`, de l'ordre du
    /// centimètre à quelques dizaines de cm par appel) : un premier essai
    /// remettait `grounded` à `false` sur *toute* correction, quelle que
    /// soit son amplitude — en écrivant le test de montée d'escalier avec
    /// réconciliation simulée (`network_client::tests::climbing_stairs_
    /// does_not_trigger_a_spurious_correction`), ça cassait la montée
    /// normale : la réconciliation corrige quasiment à chaque tick pendant
    /// un déplacement réel, donc `grounded` ne restait jamais vrai assez
    /// longtemps pour que `control_kinematic` cesse d'appliquer un tick de
    /// gravité parasite à chaque correction, cumulant une chute jamais
    /// voulue. Le seuil distingue les deux cas : sous lui, on fait confiance
    /// à l'état mis en cache (la correction est trop petite pour avoir pu
    /// faire décoller le joueur) ; au-dessus, on force une vraie détection.
    pub fn set_position(&mut self, index: usize, pos: Vec3) {
        self.invalidate_query_cache();
        if let Some(slot) = self.kinematic.iter().position(|&(i, _, _)| i == index) {
            let handle = self.kinematic[slot].1;
            if let Some(body) = self.bodies.get_mut(handle) {
                let prev = body.translation();
                let moved = (Vector::new(pos.x, pos.y, pos.z) - prev).length();
                body.set_translation(Vector::new(pos.x, pos.y, pos.z), true);
                if moved > TELEPORT_INVALIDATES_GROUND {
                    self.kinematic[slot].2.grounded = false;
                }
            }
            return;
        }
        if let Some(&(_, handle)) = self.dynamic.iter().find(|&&(i, _)| i == index)
            && let Some(body) = self.bodies.get_mut(handle)
        {
            body.set_translation(Vector::new(pos.x, pos.y, pos.z), true);
            return;
        }
        // Corps scripté (`PhysicsKind::Kinematic`, cf. `scripted` et
        // `resolve_scripted_moves`) : sans ce cas, un appelant qui téléporte un
        // objet scripté (tests, réconciliation future) ne ferait bouger que
        // `scene.objects[index].transform` — `resolve_scripted_moves` lirait
        // ensuite une position physique périmée au tick suivant (`cur =
        // body.translation()`), et calculerait un déplacement `desired` aberrant
        // à partir de l'ancien emplacement jamais mis à jour ici.
        if let Some(&(_, handle)) = self.scripted.iter().find(|&&(i, _)| i == index)
            && let Some(body) = self.bodies.get_mut(handle)
        {
            body.set_next_kinematic_translation(Vector::new(pos.x, pos.y, pos.z));
            body.set_translation(Vector::new(pos.x, pos.y, pos.z), true);
        }
    }

    /// Impose la vitesse linéaire d'un corps dynamique : utile pour un projectile qui
    /// doit partir à une vitesse connue dès sa création, plutôt que de l'accélérer
    /// progressivement comme le ferait `control` pour un joueur piloté.
    pub fn set_velocity(&mut self, index: usize, v: Vec3) {
        self.invalidate_query_cache();
        if let Some(&(_, handle)) = self.dynamic.iter().find(|&&(i, _)| i == index)
            && let Some(body) = self.bodies.get_mut(handle)
        {
            body.set_linvel(Vector::new(v.x, v.y, v.z), true);
        }
    }

    /// Broad-phase **jetable** pour les requêtes spatiales ponctuelles
    /// (`raycast`/`overlap_sphere`) — délibérément distincte de `self.broad`
    /// (la BVH incrémentale que `step` fait vivre d'un pas à l'autre) : la
    /// peupler nous-mêmes ici évite de perturber son état interne (compteurs de
    /// changement, détection de première passe) entre deux pas de simulation (cf.
    /// docs/audits/physics.md — la réutiliser a fait dérailler la physique réelle en
    /// test). Construite O(nombre de colliders) au premier appel puis **mémoïsée
    /// dans `query_cache`** jusqu'à la prochaine mutation du monde
    /// (`invalidate_query_cache`) : toutes les sondes d'un même tick partagent
    /// une seule construction.
    fn with_query_broad_phase<R>(&self, f: impl FnOnce(&DefaultBroadPhase) -> R) -> R {
        let mut cache = self.query_cache.borrow_mut();
        let broad = cache.get_or_insert_with(|| {
            let mut broad = DefaultBroadPhase::new();
            let handles: Vec<ColliderHandle> = self.collider_owner.keys().copied().collect();
            broad.update(
                &self.integration,
                &self.colliders,
                &self.bodies,
                &handles,
                &[],
                &mut Vec::new(),
            );
            broad
        });
        f(broad)
    }

    /// À appeler en tête de **toute** méthode qui peut déplacer un corps ou un
    /// collider : la broad-phase de requête mémoïsée décrirait sinon des
    /// positions périmées. `take()` d'un cache déjà vide est gratuit.
    pub(super) fn invalidate_query_cache(&mut self) {
        self.query_cache.get_mut().take();
    }

    /// Lance un rayon dans le monde physique, via le `QueryPipeline` de rapier —
    /// brique de `raycast()` côté Lua (`src/app/mod.rs`) : capteur de sol (rayon vers
    /// le bas), ligne de vue d'un cône de vision, etc. `mask` filtre les colliders par
    /// couche (mêmes bits que `collision_layer`/`collision_mask`) : seuls les colliders
    /// dont la couche recoupe `mask` sont touchés. `dir` n'a pas besoin d'être
    /// normalisé ; direction nulle → `None` sans planter plutôt que de diviser par
    /// zéro (`Vec3::try_normalize`).
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_toi: f32, mask: u32) -> Option<RaycastHit> {
        let dir = dir.try_normalize()?;
        self.with_query_broad_phase(|broad| {
            let query = broad.as_query_pipeline(
                self.narrow.query_dispatcher(),
                &self.bodies,
                &self.colliders,
                QueryFilter::new().groups(InteractionGroups::new(
                    Group::ALL,
                    Group::from_bits_truncate(mask),
                    InteractionTestMode::And,
                )),
            );
            let ray = Ray::new(origin, dir);
            let (handle, toi) = query.cast_ray(&ray, max_toi.max(0.0), true)?;
            Some(RaycastHit {
                point: origin + dir * toi,
                distance: toi,
                index: self.collider_owner.get(&handle).copied(),
            })
        })
    }

    /// Renvoie les index d'objets dont le collider recoupe une sphère de `radius`
    /// centrée en `center` (`QueryPipeline::intersect_shape`) — brique
    /// d'`overlap_sphere()` côté Lua : détection de proximité (ennemis dans un rayon,
    /// zone d'effet), sans avoir à lancer un rayon par direction possible. Même
    /// filtrage par couche que `raycast`.
    pub fn overlap_sphere(&self, center: Vec3, radius: f32, mask: u32) -> Vec<usize> {
        self.with_query_broad_phase(|broad| {
            let query = broad.as_query_pipeline(
                self.narrow.query_dispatcher(),
                &self.bodies,
                &self.colliders,
                QueryFilter::new().groups(InteractionGroups::new(
                    Group::ALL,
                    Group::from_bits_truncate(mask),
                    InteractionTestMode::And,
                )),
            );
            let ball = Ball::new(radius.max(0.0));
            query
                .intersect_shape(Pose::from_translation(center), &ball)
                .filter_map(|(handle, _)| self.collider_owner.get(&handle).copied())
                .collect()
        })
    }
}
