//! Picking et gizmo d'édition : conversion écran → NDC/monde, sélection par clic
//! (objet, lumière), poignées de translation/rotation/échelle (`pick_axis`/`pick_ring`),
//! et `handle_input` qui orchestre tout ça à partir d'un `InputEvent` agnostique de la
//! plateforme. Extrait de `app/mod.rs` — aucune dépendance gameplay/réseau,
//! seulement caméra + sélection + scène.

use glam::{Mat4, Quat, Vec3, Vec4};

use super::input::InputEvent;
use super::{AppState, GIZMO_LEN, GizmoMode, RING_SEGMENTS, axis_basis, axis_dir, device_rect};

impl AppState {
    pub fn set_viewport(&mut self, width: u32, height: u32) {
        let w = width.max(1) as f32;
        let h = height.max(1) as f32;
        self.viewport = (w, h);
        self.camera.aspect = w / h;
    }

    /// Traite un événement d'entrée agnostique (gizmo, orbit, zoom, sélection).
    pub fn handle_input(&mut self, event: InputEvent) {
        match event {
            InputEvent::PointerDown => {
                self.press_cursor = self.last_cursor;
                // Aperçu mobile : on joue au tactile, pas d'édition (ni gizmo, ni sélection).
                if self.device_preview {
                    self.dragging = true;
                    return;
                }
                // Gizmo de translation d'une lumière sélectionnée.
                if let (Some(li), Some((cx, cy))) = (self.selected_light, self.last_cursor)
                    && let Some(pl) = self.scene.point_lights.get(li)
                {
                    let origin = Vec3::from_array(pl.position);
                    if let Some(axis) = self.pick_axis_at(origin, cx, cy) {
                        if let Some(p) = self.axis_drag_param(origin, axis_dir(axis), cx, cy) {
                            self.push_undo(); // déplacement de lumière annulable
                            self.active_axis = Some(axis);
                            self.drag_light = Some(li);
                            self.drag_start_t = p;
                            self.drag_orig_pos = origin;
                        }
                        return;
                    }
                }
                // priorité au gizmo : si une poignée est cliquée, on démarre la manipulation.
                if let (Some(sel), Some((cx, cy))) = (self.selection, self.last_cursor) {
                    let origin = self.scene.objects[sel].transform.position;
                    let t = &self.scene.objects[sel].transform;
                    let (orig_rot, orig_scale) = (t.rotation, t.scale);
                    match self.gizmo_mode {
                        GizmoMode::Rotate => {
                            if let Some(axis) = self.pick_ring(sel, cx, cy) {
                                if let Some(ang) =
                                    self.ring_drag_angle(origin, axis_dir(axis), cx, cy)
                                {
                                    self.push_undo(); // un seul snapshot par manipulation
                                    self.active_axis = Some(axis);
                                    self.drag_start_angle = ang;
                                    self.drag_orig_pos = origin;
                                    self.drag_orig_rot = orig_rot;
                                    self.capture_drag_selection();
                                }
                                return;
                            }
                        }
                        _ => {
                            if let Some(axis) = self.pick_axis(sel, cx, cy) {
                                if let Some(p) =
                                    self.axis_drag_param(origin, axis_dir(axis), cx, cy)
                                {
                                    self.push_undo();
                                    self.active_axis = Some(axis);
                                    self.drag_start_t = p;
                                    self.drag_orig_pos = origin;
                                    self.drag_orig_scale = orig_scale;
                                    // mémorise les positions de toute la sélection (translate multi)
                                    self.drag_orig_positions = self
                                        .selected
                                        .iter()
                                        .filter_map(|&i| {
                                            self.scene
                                                .objects
                                                .get(i)
                                                .map(|o| (i, o.transform.position))
                                        })
                                        .collect();
                                    self.capture_drag_selection();
                                }
                                return;
                            }
                        }
                    }
                }
                self.dragging = true;
            }
            InputEvent::PointerUp => {
                if self.active_axis.take().is_some() {
                    self.drag_light = None;
                    self.press_cursor = None;
                    return;
                }
                self.dragging = false;
                // Tap (appui sans déplacement notable) ?
                let tap = matches!(
                    (self.press_cursor, self.last_cursor),
                    (Some((px, py)), Some((cx, cy))) if (px - cx).hypot(py - cy) < 4.0
                );
                // En mode Play : un tap sur un objet « tactile » le notifie à son script.
                if self.playing
                    && !self.paused
                    && tap
                    && let Some((cx, cy)) = self.last_cursor
                    && let Some(i) = self.pick(cx, cy)
                    && self.scene.objects[i].tappable
                {
                    self.tapped_obj = Some(i);
                }
                // Aperçu mobile : pas de sélection éditeur au clic (on joue, on n'édite pas).
                if self.device_preview {
                    self.press_cursor = None;
                    return;
                }
                // appui sans déplacement notable = sélection éditeur
                if let (Some((px, py)), Some((cx, cy))) = (self.press_cursor, self.last_cursor)
                    && (px - cx).hypot(py - cy) < 4.0
                {
                    // Debug drawing : visualise le rayon de picking envoyé.
                    let (ray_origin, ray_dir) = self.ray(cx, cy);
                    self.debug_line(ray_origin, ray_origin + ray_dir * 30.0, [1.0, 0.9, 0.2]);
                    // Priorité au marqueur de lumière (petit), sinon objet 3D.
                    if let Some(li) = self.pick_light(cx, cy) {
                        self.selected_light = Some(li);
                        self.clear_selection();
                    } else {
                        self.selected_light = None;
                        match self.pick(cx, cy) {
                            // Cmd/Maj : ajoute/retire de la sélection ; sinon sélection simple.
                            Some(i) if self.additive => self.toggle_select(i),
                            Some(i) => self.select_single(i),
                            None if !self.additive => self.clear_selection(),
                            None => {}
                        }
                    }
                }
                self.press_cursor = None;
            }
            InputEvent::PointerMove { x, y } => {
                // Déplacement d'une lumière sélectionnée (translate uniquement).
                if let (Some(axis), Some(li)) = (self.active_axis, self.drag_light) {
                    let a = axis_dir(axis);
                    if let Some(t) = self.axis_drag_param(self.drag_orig_pos, a, x, y)
                        && let Some(pl) = self.scene.point_lights.get_mut(li)
                    {
                        let delta = a * (t - self.drag_start_t);
                        pl.position = maybe_snap(self.drag_orig_pos + delta, self.snap).to_array();
                    }
                    self.last_cursor = Some((x, y));
                    return;
                }
                // manipulation via la poignée active
                if let (Some(axis), Some(sel)) = (self.active_axis, self.selection) {
                    let a = axis_dir(axis);
                    match self.gizmo_mode {
                        GizmoMode::Translate => {
                            if let Some(t) = self.axis_drag_param(self.drag_orig_pos, a, x, y) {
                                let delta = a * (t - self.drag_start_t);
                                let snap = self.snap;
                                if self.drag_orig_positions.len() > 1 {
                                    // déplace toute la sélection en bloc
                                    for (i, orig) in &self.drag_orig_positions {
                                        if let Some(o) = self.scene.objects.get_mut(*i) {
                                            o.transform.position = maybe_snap(*orig + delta, snap);
                                        }
                                    }
                                } else {
                                    self.scene.objects[sel].transform.position =
                                        maybe_snap(self.drag_orig_pos + delta, snap);
                                }
                            }
                        }
                        GizmoMode::Scale => {
                            if let Some(t) = self.axis_drag_param(self.drag_orig_pos, a, x, y) {
                                let d = t - self.drag_start_t;
                                // Même delta appliqué à chaque objet sélectionné (multi-scale).
                                for (i, t0) in &self.drag_orig_transforms {
                                    if let Some(o) = self.scene.objects.get_mut(*i) {
                                        let mut s = t0.scale;
                                        match axis {
                                            0 => s.x = (s.x + d).max(0.05),
                                            1 => s.y = (s.y + d).max(0.05),
                                            _ => s.z = (s.z + d).max(0.05),
                                        }
                                        o.transform.scale = s;
                                    }
                                }
                            }
                        }
                        GizmoMode::Rotate => {
                            if let Some(ang) = self.ring_drag_angle(self.drag_orig_pos, a, x, y) {
                                let delta = ang - self.drag_start_angle;
                                let rot = Quat::from_axis_angle(a, delta);
                                // Rotation autour du pivot commun (position + orientation).
                                let pivot = self.drag_pivot;
                                for (i, t0) in &self.drag_orig_transforms {
                                    if let Some(o) = self.scene.objects.get_mut(*i) {
                                        o.transform.rotation = rot * t0.rotation;
                                        o.transform.position = pivot + rot * (t0.position - pivot);
                                    }
                                }
                            }
                        }
                    }
                    self.last_cursor = Some((x, y));
                    return;
                } else if self.dragging
                    && !self.device_preview // en aperçu mobile : pas d'orbite souris (simule le tactile)
                    && let Some((lx, _ly)) = self.last_cursor
                {
                    // Rotation horizontale seulement (le zoom vient du pinch/molette,
                    // cf. `InputEvent::Scroll`) : l'angle de plongée (`pitch`) reste fixe,
                    // façon caméra de suivi à la Zelda — un angle vertical libre rend
                    // le repère visuel instable (le sol/l'horizon basculent au moindre
                    // geste).
                    self.camera.yaw -= (x - lx) as f32 * 0.005;
                }
                self.last_cursor = Some((x, y));
            }
            InputEvent::Scroll { delta } => {
                // En aperçu mobile, la molette ne zoome pas (un téléphone n'a pas de molette).
                if !self.device_preview {
                    self.camera.distance = (self.camera.distance - delta * 0.5).clamp(1.5, 50.0);
                }
            }
        }
    }

    /// Rayon monde (origine, direction) issu d'un point écran en pixels.
    /// `vp_inv` = inverse de la view-projection (calculée une fois par l'appelant).
    /// Convertit un point écran (pixels) en NDC, en tenant compte du rectangle
    /// letterboxé de l'aperçu mobile (sinon : tout le viewport).
    fn screen_to_ndc(&self, px: f64, py: f64) -> (f32, f32) {
        let (ox, oy, w, h) = if self.device_preview {
            let (bx, by, bw, bh) = if self.view_rect_px.2 > 1.0 {
                self.view_rect_px
            } else {
                (0.0, 0.0, self.viewport.0, self.viewport.1)
            };
            let (rx, ry, rw, rh) = device_rect(bw, bh, self.device_portrait);
            (bx + rx, by + ry, rw, rh)
        } else {
            (0.0, 0.0, self.viewport.0, self.viewport.1)
        };
        (
            2.0 * (px as f32 - ox) / w - 1.0,
            1.0 - 2.0 * (py as f32 - oy) / h,
        )
    }

    fn ray_with(&self, vp_inv: Mat4, px: f64, py: f64) -> (Vec3, Vec3) {
        let (ndc_x, ndc_y) = self.screen_to_ndc(px, py);
        let near = vp_inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far = vp_inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let origin = near.truncate() / near.w;
        let dir = (far.truncate() / far.w - origin).normalize();
        (origin, dir)
    }

    /// Variante pratique : recalcule l'inverse de la view-projection à la volée.
    fn ray(&self, px: f64, py: f64) -> (Vec3, Vec3) {
        self.ray_with(self.camera.view_proj().inverse(), px, py)
    }

    /// Projette un point monde vers les coordonnées écran (pixels), si devant la caméra.
    /// `vp` = view-projection (calculée une fois par l'appelant).
    fn project_with(&self, vp: Mat4, world: Vec3) -> Option<(f64, f64)> {
        let clip = vp * world.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        let (w, h) = self.viewport;
        Some((
            ((ndc.x * 0.5 + 0.5) * w) as f64,
            ((1.0 - (ndc.y * 0.5 + 0.5)) * h) as f64,
        ))
    }

    /// Renvoie l'axe du gizmo sous le curseur (test écran à ~10 px), ou None.
    fn pick_axis(&self, sel: usize, px: f64, py: f64) -> Option<usize> {
        self.pick_axis_at(self.scene.objects[sel].transform.position, px, py)
    }

    /// Axe du gizmo de translation sous le curseur, pour une origine quelconque (~10 px).
    fn pick_axis_at(&self, origin: Vec3, px: f64, py: f64) -> Option<usize> {
        let vp = self.camera.view_proj();
        let mut best: Option<(f64, usize)> = None;
        for axis in 0..3 {
            let (Some(p0), Some(p1)) = (
                self.project_with(vp, origin),
                self.project_with(vp, origin + axis_dir(axis) * GIZMO_LEN),
            ) else {
                continue;
            };
            let d = point_segment_dist((px, py), p0, p1);
            if d < 10.0 && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, axis));
            }
        }
        best.map(|(_, a)| a)
    }

    /// Lumière ponctuelle dont le marqueur est sous le curseur (~14 px), ou None.
    fn pick_light(&self, px: f64, py: f64) -> Option<usize> {
        let vp = self.camera.view_proj();
        let mut best: Option<(f64, usize)> = None;
        for (i, pl) in self.scene.point_lights.iter().enumerate() {
            if let Some((sx, sy)) = self.project_with(vp, Vec3::from_array(pl.position)) {
                let d = (px - sx).hypot(py - sy);
                if d < 14.0 && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// Paramètre `t` du point du curseur projeté sur l'axe (via le plan le plus face caméra).
    fn axis_drag_param(&self, origin: Vec3, a: Vec3, px: f64, py: f64) -> Option<f32> {
        let (ro, rd) = self.ray(px, py);
        // plan contenant l'axe, de normale perpendiculaire à l'axe et tournée vers la vue
        let n = a.cross(rd.cross(a));
        if n.length_squared() < 1e-8 {
            return None;
        }
        let n = n.normalize();
        let denom = rd.dot(n);
        if denom.abs() < 1e-6 {
            return None;
        }
        let t_ray = (origin - ro).dot(n) / denom;
        let p = ro + rd * t_ray;
        Some((p - origin).dot(a))
    }

    /// Renvoie l'axe dont l'anneau de rotation est sous le curseur (~10 px), ou None.
    fn pick_ring(&self, sel: usize, px: f64, py: f64) -> Option<usize> {
        const N: usize = RING_SEGMENTS;
        let origin = self.scene.objects[sel].transform.position;
        let vp = self.camera.view_proj();
        let mut best: Option<(f64, usize)> = None;
        for axis in 0..3 {
            let (u, w) = axis_basis(axis_dir(axis));
            let mut prev: Option<(f64, f64)> = None;
            let mut first: Option<(f64, f64)> = None;
            let mut min_d = f64::INFINITY;
            for j in 0..=N {
                let ang = std::f32::consts::TAU * j as f32 / N as f32;
                let pt = origin + (u * ang.cos() + w * ang.sin()) * GIZMO_LEN;
                let Some(sp) = self.project_with(vp, pt) else {
                    continue;
                };
                if first.is_none() {
                    first = Some(sp);
                }
                if let Some(pp) = prev {
                    min_d = min_d.min(point_segment_dist((px, py), pp, sp));
                }
                prev = Some(sp);
            }
            if min_d < 10.0 && best.is_none_or(|(bd, _)| min_d < bd) {
                best = Some((min_d, axis));
            }
        }
        best.map(|(_, a)| a)
    }

    /// Angle (radians) du curseur autour de l'axe, dans le plan perpendiculaire à `a`.
    fn ring_drag_angle(&self, origin: Vec3, a: Vec3, px: f64, py: f64) -> Option<f32> {
        let (ro, rd) = self.ray(px, py);
        let denom = rd.dot(a);
        if denom.abs() < 1e-6 {
            return None;
        }
        let t = (origin - ro).dot(a) / denom;
        let p = ro + rd * t;
        let v = p - origin;
        let (u, w) = axis_basis(a);
        Some(v.dot(w).atan2(v.dot(u)))
    }

    /// Lance un rayon depuis le curseur et renvoie l'objet le plus proche touché.
    fn pick(&self, px: f64, py: f64) -> Option<usize> {
        let (origin, dir) = self.ray(px, py);

        let mut best: Option<(f32, usize)> = None;
        for (i, obj) in self.scene.objects.iter().enumerate() {
            let (lmin, lmax) = self.scene.local_aabb(obj.mesh);
            let m = obj.transform.matrix();
            let mut wmin = Vec3::splat(f32::INFINITY);
            let mut wmax = Vec3::splat(f32::NEG_INFINITY);
            for sx in [lmin.x, lmax.x] {
                for sy in [lmin.y, lmax.y] {
                    for sz in [lmin.z, lmax.z] {
                        let p = (m * Vec3::new(sx, sy, sz).extend(1.0)).truncate();
                        wmin = wmin.min(p);
                        wmax = wmax.max(p);
                    }
                }
            }
            if let Some(t) = ray_aabb(origin, dir, wmin, wmax)
                && best.is_none_or(|(bt, _)| t < bt)
            {
                best = Some((t, i));
            }
        }
        best.map(|(_, i)| i)
    }
}

/// Aligne une position sur la grille (pas de 0.5) si `snap` est actif.
fn maybe_snap(p: Vec3, snap: bool) -> Vec3 {
    if !snap {
        return p;
    }
    const STEP: f32 = 0.5;
    Vec3::new(
        (p.x / STEP).round() * STEP,
        (p.y / STEP).round() * STEP,
        (p.z / STEP).round() * STEP,
    )
}

/// Distance 2D (pixels) entre un point et un segment.
pub(super) fn point_segment_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (px, py) = p;
    let (ax, ay) = a;
    let (bx, by) = b;
    let abx = bx - ax;
    let aby = by - ay;
    let len2 = abx * abx + aby * aby;
    let t = if len2 < 1e-9 {
        0.0
    } else {
        (((px - ax) * abx + (py - ay) * aby) / len2).clamp(0.0, 1.0)
    };
    let cx = ax + t * abx;
    let cy = ay + t * aby;
    (px - cx).hypot(py - cy)
}

/// Intersection rayon / AABB (méthode des slabs). Renvoie le t d'entrée si touché devant.
pub(super) fn ray_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let o = origin.to_array();
    let d = dir.to_array();
    let mn = min.to_array();
    let mx = max.to_array();
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    for i in 0..3 {
        if d[i].abs() < 1e-8 {
            if o[i] < mn[i] || o[i] > mx[i] {
                return None;
            }
        } else {
            let t1 = (mn[i] - o[i]) / d[i];
            let t2 = (mx[i] - o[i]) / d[i];
            let (t1, t2) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
        }
    }
    if tmax >= tmin && tmax >= 0.0 {
        Some(tmin.max(0.0))
    } else {
        None
    }
}
