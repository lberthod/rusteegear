use super::*;

impl Physics {
    /// Avance la simulation de `dt` et recopie les poses des corps dynamiques
    /// **et** kinématiques (Sprint 103b). `pipeline.step` déplace un corps
    /// kinématique vers la translation programmée par `control_kinematic` via
    /// `set_next_kinematic_translation` — la recopie ci-dessous ne fait que
    /// refléter ce résultat dans `transform`, comme pour un corps dynamique.
    /// Sprint 125 : ajoute la vitesse de chaque zone de vent (`SceneObject::wind`,
    /// `trigger: true`) aux corps dynamiques dont l'AABB la touche, avant l'intégration
    /// de ce pas — un corps qui quitte la zone n'est plus poussé dès le pas suivant
    /// (pas de vitesse résiduelle stockée), et un corps traversé par deux zones cumule
    /// les deux forces.
    fn apply_wind_zones(&mut self, dt: f32, scene: &Scene) {
        let zones: Vec<(Vec3, usize)> = scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.trigger && o.visible)
            .filter_map(|(i, o)| o.wind.map(|w| (w, i)))
            .collect();
        if zones.is_empty() {
            return;
        }
        for &(i, handle) in &self.dynamic {
            let Some(body_obj) = scene.objects.get(i) else {
                continue;
            };
            let mut push = Vec3::ZERO;
            for &(wind, zi) in &zones {
                if zi == i {
                    continue;
                }
                if let Some(zone_obj) = scene.objects.get(zi)
                    && scene.world_aabb_intersects(body_obj, zone_obj)
                {
                    push += wind;
                }
            }
            if push == Vec3::ZERO {
                continue;
            }
            if let Some(body) = self.bodies.get_mut(handle) {
                let v = body.linvel();
                body.set_linvel(v + push * dt, true);
            }
        }
    }

    pub fn step(&mut self, dt: f32, scene: &mut Scene) {
        self.invalidate_query_cache();
        self.integration.dt = dt.clamp(1.0 / 240.0, 1.0 / 20.0);
        self.apply_wind_zones(dt, scene);
        self.pipeline.step(
            self.gravity,
            &self.integration,
            &mut self.islands,
            &mut self.broad,
            &mut self.narrow,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse,
            &mut self.multibody,
            &mut self.ccd,
            &(),
            &(),
        );

        for &(i, handle) in &self.dynamic {
            if let (Some(body), Some(obj)) = (self.bodies.get(handle), scene.objects.get_mut(i)) {
                let t = body.translation();
                obj.transform.position = Vec3::new(t.x, t.y, t.z);
                let r = body.rotation();
                obj.transform.rotation = Quat::from_xyzw(r.x, r.y, r.z, r.w);
            }
        }
        for &(i, handle, _) in &self.kinematic {
            if let (Some(body), Some(obj)) = (self.bodies.get(handle), scene.objects.get_mut(i)) {
                let t = body.translation();
                obj.transform.position = Vec3::new(t.x, t.y, t.z);
                let r = body.rotation();
                obj.transform.rotation = Quat::from_xyzw(r.x, r.y, r.z, r.w);
            }
        }
    }
}
