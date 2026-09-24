//! Reproduction LOCALE (headless, sans réseau) des deux symptômes de
//! collision galets signalés par capture d'écran (session du 15 septembre
//! 2026) : joueur coincé entre deux galets de berge proches, et joueur
//! perché sur un galet au lieu d'être bloqué par lui.
//!
//! Charge `Scene::riviere_demo()` directement (même scène que la prod), sans
//! rendu ni réseau — juste `Physics::build`/`control`/`step`, comme les tests
//! unitaires de `src/runtime/physics/tests.rs`.
//!
//! Usage : `cargo run --example probe_riviere_rock_collision`

use glam::Vec3;
use motor3derust::runtime::physics::{ColliderShape, PhysicsKind};
use motor3derust::runtime::physics::Physics;
use motor3derust::scene::{MeshKind, Scene};

/// Tout décor rond/quasi-convexe planté par `place()` : galets, troncs
/// couchés, blocs de falaise — mêmes candidats que `riviere.rs` (cf.
/// `is_round_solid_decor`), passés ici en revue tous ensemble plutôt qu'un
/// seul fichier à la fois.
fn is_round_decor(scene: &Scene, o: &motor3derust::scene::SceneObject) -> bool {
    let MeshKind::Imported(idx) = o.mesh else {
        return false;
    };
    let Some(m) = scene.imported.get(idx as usize) else {
        return false;
    };
    m.path.contains("galet") || m.path.contains("tronc_mort") || m.path.contains("rocher")
}

fn main() {
    let scene = Scene::riviere_demo();
    println!(
        "Scène chargée : {} objets, {} imports.",
        scene.objects.len(),
        scene.imported.len()
    );

    // ------------------------------------------------------------------
    // Inventaire des galets solides (Berge + Rivière), avec footprint
    // approximatif (AABB locale x/z * échelle) — même formule que le
    // collider `capsule()`/`cuboid()` de `Physics::build` pour la
    // comparaison, même si le collider réel est un ConvexHull.
    // ------------------------------------------------------------------
    let mut galets: Vec<(usize, Vec3, f32, f32, String, ColliderShape)> = Vec::new(); // (idx obj, pos, footprint_r, height_local_top*scale, groupe, shape)
    for (i, o) in scene.objects.iter().enumerate() {
        if !is_round_decor(&scene, o) {
            continue;
        }
        if o.physics != PhysicsKind::Static {
            continue;
        }
        let (lmin, lmax) = scene.local_aabb(o.mesh);
        let sc = o.transform.scale;
        let footprint_r = (lmax.x - lmin.x).max(lmax.z - lmin.z) * 0.5 * sc.x.max(sc.z);
        let top = lmax.y * sc.y;
        galets.push((
            i,
            o.transform.position,
            footprint_r,
            top,
            o.group.clone(),
            o.collider_shape,
        ));
    }
    println!("Décor rond solide trouvé : {}", galets.len());
    // Correctif A (15 septembre 2026) : les galets solides sont passés de
    // ConvexHull à Cylinder (flancs verticaux, non franchissables par la
    // pente) — cf. `riviere.rs::place()`. Correctif B (élargissement) : même
    // traitement pour tout décor rond, pas seulement les galets. Informatif
    // seulement (pas d'assert) — cette sonde sert justement à mesurer l'état
    // AVANT correctif sur `tronc_mort`/`rocher_*` autant qu'à vérifier l'état
    // APRÈS sur `galet_*`.
    let mut shape_counts: Vec<(String, ColliderShape, u32)> = Vec::new();
    for (_, _, _, _, group, shape) in &galets {
        match shape_counts.iter_mut().find(|(g, s, _)| g == group && s == shape) {
            Some((_, _, n)) => *n += 1,
            None => shape_counts.push((group.clone(), *shape, 1)),
        }
    }
    for (group, shape, n) in &shape_counts {
        println!("  {group:<8} {shape:?}: {n}");
    }

    // Rayon (footprint) du joueur, même formule que `capsule()` dans
    // `Physics::build` (`he.x.max(he.z)`), pour juger ce qu'est un
    // "passage trop étroit" en pratique.
    let player_idx = scene
        .objects
        .iter()
        .position(|o| o.controller.as_ref().is_some_and(|c| c.input))
        .expect("joueur");
    let (plmin, plmax) = scene.local_aabb(scene.objects[player_idx].mesh);
    let player_r = (plmax.x - plmin.x).max(plmax.z - plmin.z) * 0.5;
    println!("\nRayon capsule joueur ≈ {player_r:.3} m (diamètre ≈ {:.3} m).", player_r * 2.0);

    // ------------------------------------------------------------------
    // SYMPTÔME 1 : paires de galets dont le PASSAGE (distance centre à
    // centre - somme des rayons) est le plus étroit — plus pertinent qu'un
    // simple seuil de distance centre-à-centre, qui ne dit rien de la place
    // réellement laissée entre les deux enveloppes.
    // ------------------------------------------------------------------
    println!("\n== 1. Paires de galets au passage le plus étroit ==");
    let mut pairs: Vec<(f32, usize, usize, f32)> = Vec::new(); // (gap, a, b, center_dist)
    for a in 0..galets.len() {
        for b in (a + 1)..galets.len() {
            let pa = galets[a].1;
            let pb = galets[b].1;
            let d = ((pa.x - pb.x).powi(2) + (pa.z - pb.z).powi(2)).sqrt();
            if d > 6.0 {
                continue; // hors de propos, pas un "voisinage"
            }
            let gap = d - galets[a].2 - galets[b].2;
            pairs.push((gap, a, b, d));
        }
    }
    pairs.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    println!("{} paire(s) dans un voisinage de 6m analysées, 10 plus étroites :", pairs.len());
    for (gap, a, b, d) in pairs.iter().take(10) {
        let (ia, pa, ra, _, ga, _) = &galets[*a];
        let (ib, pb, rb, _, gb, _) = &galets[*b];
        let (pax, paz, pbx, pbz) = (pa.x, pa.z, pb.x, pb.z);
        println!(
            "  gap≈{gap:+.3}m (d={d:.3}m)  obj#{ia} [{ga}] r≈{ra:.2} ({pax:.2},{paz:.2})  <->  obj#{ib} [{gb}] r≈{rb:.2} ({pbx:.2},{pbz:.2})"
        );
    }

    // Teste les 3 paires au passage le plus étroit : chute au milieu, puis
    // poussée dans 5 directions pendant 3s (`slide:true` du contrôleur
    // cinématique — un piège en coin concave ferait dévier la trajectoire
    // sans jamais s'échapper).
    for &(gap, a, b, d) in pairs.iter().take(3) {
        let (ia, pa, _, _, _, _) = galets[a].clone();
        let (ib, pb, _, _, _, _) = galets[b].clone();
        println!(
            "\n-- Test de coincement : obj#{ia}/#{ib} (gap≈{gap:+.3}m, d={d:.3}m, diamètre joueur {:.3}m) --",
            player_r * 2.0
        );
        let mid = (pa + pb) * 0.5;

        for (label, dir) in [
            ("+X", Vec3::new(1.0, 0.0, 0.0)),
            ("-X", Vec3::new(-1.0, 0.0, 0.0)),
            ("+Z", Vec3::new(0.0, 0.0, 1.0)),
            ("-Z", Vec3::new(0.0, 0.0, -1.0)),
            ("+X+Z", Vec3::new(1.0, 0.0, 1.0).normalize()),
        ] {
            let mut s = scene.clone();
            let mut phys = Physics::build(&s);
            let start = mid + Vec3::new(0.0, 2.0, 0.0);
            phys.set_position(player_idx, start);
            // laisse retomber au sol (quelques pas sans input)
            let dt = 1.0 / 60.0;
            for _ in 0..40 {
                phys.control(player_idx, 0.0, 0.0, false, 0.0, 0.0, dt);
                phys.step(dt, &mut s);
            }
            let settled = s.objects[player_idx].transform.position;
            for _ in 0..180 {
                phys.control(player_idx, dir.x * 3.0, dir.z * 3.0, false, 0.0, 0.0, dt);
                phys.step(dt, &mut s);
            }
            let end = s.objects[player_idx].transform.position;
            let net = ((end.x - settled.x).powi(2) + (end.z - settled.z).powi(2)).sqrt();
            let (sx, sy, sz, ex, ey, ez) = (settled.x, settled.y, settled.z, end.x, end.y, end.z);
            println!(
                "  posé=({sx:.2},{sy:.2},{sz:.2}) poussée {label:6} 3s -> fin=({ex:.2},{ey:.2},{ez:.2}) | déplacement net={net:.3}m"
            );
            if net < 0.15 {
                println!("    -> BLOQUÉ quasi sur place dans cette direction (déplacement net {net:.3}m < 0.15m).");
            }
        }
    }
    if pairs.is_empty() {
        println!("Aucune paire de galets dans un voisinage de 6m — pas de candidat de coincement.");
    }

    // ------------------------------------------------------------------
    // SYMPTÔME 2 : hauteur hors-sol réelle de quelques galets représentatifs
    // (mesurée par raycast sur une scène SANS aucun galet, donc sur le vrai
    // sol/terrain nu, comparée à la hauteur du sommet du galet).
    // ------------------------------------------------------------------
    println!("\n== 2. Hauteur hors-sol réelle des galets (vs autostep 0.3m) ==");
    println!("(mesurée par raycast sur une scène réduite au terrain nu + enceinte)");
    // Ne garde QUE le terrain et l'enceinte : un simple filtre "pas de
    // galet" laissait les arbres/sous-bois en place, dont le ConvexHull
    // (large en hauteur, cf. commentaire ligne ~551 de riviere.rs) peut
    // recouvrir la colonne verticale au-dessus d'un galet voisin et fausser
    // la mesure (constaté : un point à 5m au-dessus d'un galet déjà "dans"
    // un collider, `raycast` renvoyant toi=0). Isoler le terrain nu donne la
    // vraie hauteur de sol, indépendamment du reste du décor.
    let mut bare = scene.clone();
    bare.objects
        .retain(|o| o.name == "Vallée" || o.group == "Enceinte");
    let phys_bare = Physics::build(&bare);

    // Balayage COMPLET des 40 galets : hauteur hors-sol réelle (raycast sur
    // terrain nu) + test comportemental réel (approche frontale, le joueur
    // monte-t-il dessus ?). Corrèle les deux pour trancher : est-ce que
    // seuls les galets sous 0.3m sont escaladés (autostep normal), ou est-ce
    // qu'un galet plus haut se laisse quand même gravir (rampe artefact du
    // ConvexHull) ?
    println!("\n== 2b. Balayage complet : hauteur hors-sol vs comportement réel ==");
    let mut rows: Vec<(usize, String, f32, f32)> = Vec::new(); // (obj_i, group, exposed, climbed_dy)
    for &(obj_i, pos, footprint_r, top_local, ref group, _) in &galets {
        let ray_origin = pos + Vec3::new(0.0, 15.0, 0.0);
        let Some(gy) = phys_bare
            .raycast(ray_origin, Vec3::new(0.0, -1.0, 0.0), 40.0, u32::MAX)
            .map(|h| h.point.y)
        else {
            println!("  obj#{obj_i} [{group}] : raycast raté, ignoré");
            continue;
        };
        let exposed = (pos.y + top_local) - gy;

        // Approche frontale réelle (+Z) : parcourt depuis 2m avant le galet.
        let mut s = scene.clone();
        let mut phys = Physics::build(&s);
        let start = pos + Vec3::new(0.0, 1.0, -(footprint_r + 2.0));
        phys.set_position(player_idx, start);
        let dt = 1.0 / 60.0;
        for _ in 0..20 {
            phys.control(player_idx, 0.0, 0.0, false, 0.0, 0.0, dt);
            phys.step(dt, &mut s);
        }
        let ground_before = s.objects[player_idx].transform.position.y;
        // Ne retient la montée que TANT QUE le joueur reste à proximité
        // immédiate de l'objet (empreinte + 1,5m) — même filtre que le test
        // `riviere_demo_player_cannot_climb_round_decor`. Sans ce filtre,
        // un décor proche d'une pente/falaise (Falaise, Z_LIP) fausse
        // totalement la mesure : le joueur poussé 4s à 3 m/s peut quitter le
        // voisinage de l'objet et tomber d'une tout autre pente (constaté :
        // Δy≈-146m, chute dans le bassin, rien à voir avec l'objet testé).
        let mut climbed_near = 0.0f32;
        for _ in 0..150 {
            phys.control(player_idx, 0.0, 3.0, false, 0.0, 0.0, dt);
            phys.step(dt, &mut s);
            let p = s.objects[player_idx].transform.position;
            let horiz = ((p.x - pos.x).powi(2) + (p.z - pos.z).powi(2)).sqrt();
            if horiz <= footprint_r + 1.5 {
                climbed_near = climbed_near.max(p.y - ground_before);
            }
        }
        let climbed = climbed_near;
        println!(
            "  obj#{obj_i:<5} [{group:<8}] hors-sol≈{exposed:+.3}m   Δy près de l'objet={climbed:+.3}m   {}",
            if climbed > 0.15 { "MONTÉ" } else { "bloqué" }
        );
        rows.push((obj_i, group.clone(), exposed, climbed));
    }

    let climbed_rows: Vec<&(usize, String, f32, f32)> = rows.iter().filter(|r| r.3 > 0.15).collect();
    let blocked_rows: Vec<&(usize, String, f32, f32)> = rows.iter().filter(|r| r.3 <= 0.15).collect();
    let max_climbed_exposed = climbed_rows.iter().map(|r| r.2).fold(f32::MIN, f32::max);
    let min_blocked_exposed = blocked_rows.iter().map(|r| r.2).fold(f32::MAX, f32::min);
    println!(
        "\nRésumé : {} galet(s) montés, {} bloqué(s) sur {} testés.",
        climbed_rows.len(),
        blocked_rows.len(),
        rows.len()
    );
    if !climbed_rows.is_empty() {
        println!("  Plus haut galet quand même MONTÉ : hors-sol={max_climbed_exposed:.3}m (seuil autostep=0.300m)");
        if max_climbed_exposed > 0.35 {
            println!("  -> ANOMALIE : un galet monté a un hors-sol nettement supérieur à l'autostep (0.3m) : possible rampe/artefact ConvexHull.");
        } else {
            println!("  -> cohérent avec un autostep normal (0.3m), pas d'anomalie détectée.");
        }
    }
    if !blocked_rows.is_empty() {
        println!("  Plus bas galet quand même BLOQUÉ : hors-sol={min_blocked_exposed:.3}m");
    }

    // ------------------------------------------------------------------
    // SYMPTÔME 2 (suite) : la montée observée (jusqu'à >1m, bien au-delà de
    // l'autostep 0.3m) vient-elle du ConvexHull, ou est-elle inhérente à
    // tout galet arrondi combiné à `max_slope_climb_angle=50°` (indépendant
    // du type de collider) ? Rejoue l'approche frontale sur les mêmes
    // galets en forçant `ColliderShape::TriMesh` (silhouette exacte) au lieu
    // de `ConvexHull` — si le résultat ne change pas, la montée n'est pas un
    // artefact du ConvexHull mais de la géométrie arrondie + l'angle de
    // pente franchissable.
    // ------------------------------------------------------------------
    println!("\n== 2c. Même approche frontale, collider TriMesh (silhouette exacte) au lieu de ConvexHull ==");
    for row in climbed_rows.iter() {
        let (obj_i, _group, exposed, dy_hull) = (row.0, &row.1, row.2, row.3);
        let pos = galets.iter().find(|g| g.0 == obj_i).unwrap().1;
        let footprint_r = galets.iter().find(|g| g.0 == obj_i).unwrap().2;
        let mut s = scene.clone();
        s.objects[obj_i].collider_shape = ColliderShape::TriMesh;
        let mut phys = Physics::build(&s);
        let start = pos + Vec3::new(0.0, 1.0, -(footprint_r + 2.0));
        phys.set_position(player_idx, start);
        let dt = 1.0 / 60.0;
        for _ in 0..20 {
            phys.control(player_idx, 0.0, 0.0, false, 0.0, 0.0, dt);
            phys.step(dt, &mut s);
        }
        let ground_before = s.objects[player_idx].transform.position.y;
        for _ in 0..240 {
            phys.control(player_idx, 0.0, 3.0, false, 0.0, 0.0, dt);
            phys.step(dt, &mut s);
        }
        let dy_trimesh = s.objects[player_idx].transform.position.y - ground_before;
        println!(
            "  obj#{obj_i} hors-sol≈{exposed:.3}m : Δy avec ConvexHull={dy_hull:+.3}m  vs  Δy avec TriMesh={dy_trimesh:+.3}m  {}",
            if dy_trimesh > 0.15 { "-> TOUJOURS monté avec TriMesh (pas un artefact ConvexHull)" }
            else { "-> BLOQUÉ avec TriMesh (le ConvexHull est bien la cause de la montée)" }
        );
    }

    println!("\nFin de la sonde locale.");
}
