use super::*;

mod terrain;

mod creatures;

mod fauna;

mod decor_data;
use decor_data::*;

mod decor_terrain;

impl Scene {
    pub fn mmorpg_demo() -> Self {
        let half = Self::MMORPG_HALF;
        // Sprint 24 (Phase K, `sprintreflecion.md`) : `MeshKind::Terrain` remplace
        // l'ancien `MeshKind::Plane` plat — relief réel (collines) sur la marge
        // ouest de la carte, nul (y≈0, tolérance sub-centimétrique) partout où le
        // hameau/la forêt/l'eau/les rizières/la route sont déjà placés à la main
        // ci-dessous (cf. `gfx::mesh::mmorpg_terrain_local_height` pour le détail
        // du découpage en zones). Échelle X/Z = 2×`half` pour couvrir toute la
        // carte 72×72 m ; échelle Y = 1.0 (découplée) car la fonction de hauteur
        // renvoie déjà des mètres directement, pas un facteur à re-multiplier.
        let mut sol = demo_obj("Sol", MeshKind::Terrain, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        // Vert prairie (l'arène est habillée en coin de campagne, cf. le décor
        // nature plus bas) — l'ancien gris-vert sombre jurait avec les aplats.
        sol.color = [0.26, 0.38, 0.21];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        // Tag lu par les scripts de comportement des créatures 12/13/19
        // (`find_tag("joueur")` — rôdeur qui maintient sa distance, méduse qui
        // fuit, lanterne qui dérive vers lui) : sans lui, elles retombent sur
        // leur comportement sans cible (immobiles pour la lanterne).
        joueur.tag = "joueur".into();
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];
        let mut imported: Vec<ImportedMesh> = Vec::new();

        terrain::add_walls_and_landmarks(&mut objects, half);

        creatures::add_named_creatures(&mut objects, &mut imported, half);

        fauna::add_ambient_fauna(&mut objects, &mut imported, half);

        decor_terrain::add_terrain_layers(&mut objects, half);

        // Chargeur commun aux landmarks et au scatter : un même fichier n'est
        // chargé qu'une fois, les instances partagent l'entrée `imported`.
        let mut anim_count = 0usize;
        let mut poser = |name: &str,
                         file: &'static str,
                         x: f32,
                         z: f32,
                         scale: f32,
                         yaw_deg: f32,
                         solide: bool,
                         anim: Option<&'static str>| {
            let mesh_index = match imported.iter().position(|m| m.path.ends_with(file)) {
                Some(i) => i as u32,
                None => {
                    let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file);
                    match crate::scene::import::load_gltf(&path) {
                        Ok((data, aabb_min, aabb_max)) => {
                            let mut mesh = ImportedMesh {
                                path,
                                data,
                                aabb_min,
                                aabb_max,
                                ..Default::default()
                            };
                            // Statiques : ne peuple que les tangentes du rendu.
                            // Riggés (moulins…) : charge squelette et clips.
                            mesh.load_skinning();
                            imported.push(mesh);
                            (imported.len() - 1) as u32
                        }
                        Err(e) => {
                            log::error!("{name} MMORPG ({file}) : {e}");
                            return;
                        }
                    }
                }
            };
            let mut deco = demo_obj(name, MeshKind::Imported(mesh_index), Vec3::new(x, 0.0, z));
            deco.transform = deco.transform.with_scale(Vec3::splat(scale));
            if yaw_deg != 0.0 {
                deco.transform.rotation = glam::Quat::from_rotation_y(yaw_deg.to_radians());
            }
            if solide {
                deco.physics = PhysicsKind::Static;
                deco.collider_shape = crate::runtime::physics::ColliderShape::TriMesh;
            }
            if let Some(clip) = anim {
                anim_count += 1;
                deco.animation = Some(AnimationState {
                    clip: clip.into(),
                    // Phases de départ décalées : deux instances du même clip ne
                    // pulsent jamais à l'unisson (l'œil le repère tout de suite).
                    time: anim_count as f32 * 0.37,
                    ..Default::default()
                });
            }
            objects.push(deco);
        };
        for spec in NATURE_DECOR
            .iter()
            .chain(VILLAGE_PROPS.iter())
            .chain(MONSTER_DECOR.iter())
        {
            poser(
                spec.name,
                spec.file,
                spec.pos.0,
                spec.pos.1,
                spec.scale,
                spec.yaw_deg,
                spec.solide,
                spec.anim,
            );
        }

        // 3) Scatter procédural seedé : peuple forêt, lisières, prairie et
        //    rizières sans énumérer 100 entrées à la main. Graine LITTÉRALE →
        //    même carte à chaque chargement (et donc testable : densités et
        //    dégagements sont des invariants, pas des coups de dés).
        //    Rejection sampling : un candidat est rejeté s'il tombe dans une
        //    zone aménagée (eau, routes, rizières, hameau, promontoire, col
        //    venté, clairières), à < 4 m d'un spawn de créature (marge sur les
        //    3,5 m des sondes) ou, pour un solide, à < 2,5 m d'un autre solide
        //    (pas de troncs fusionnés). Budget solides borné (~75 avec les
        //    landmarks) : chaque TriMesh pèse sur la broad-phase des raycasts
        //    des sondes — la densité visuelle vient du végétal traversable.
        type Rect = (f32, f32, f32, f32); // (x0, z0, x1, z1)
        const EXCL_EAU_ROUTES: &[Rect] = &[
            (-28.0, -36.0, -24.0, -6.0), // rivière nord
            (-28.0, -8.0, -16.0, -4.0),  // coude
            (-26.0, -2.0, -12.0, 10.0),  // lac
            (-18.0, 10.0, -14.0, 36.0),  // rivière sud
            (-26.0, 10.5, -12.0, 13.5),  // berge de sable
            (-36.0, 12.3, 36.0, 15.7),   // route principale
            (8.8, -10.0, 11.2, 14.0),    // chemin du hameau
            // Chemin du pont nord + son prolongement « sentier du
            // promontoire » (même bande z, x poussé de 6 à 34).
            (-26.0, -10.9, 34.0, -9.1),
            (-5.6, 14.0, -3.4, 26.0), // chemin des rizières
        ];
        const EXCL_ZONES_AMENAGEES: &[Rect] = &[
            (-11.0, 23.5, -5.0, 28.5), // rizière 1
            (-4.0, 23.5, 2.0, 28.5),   // rizière 2
            (-11.0, 29.5, -5.0, 34.5), // rizière 3
            (-4.0, 29.5, 2.0, 34.5),   // rizière 4
            (3.0, 26.5, 9.0, 31.5),    // rizière 5
            (2.0, 4.0, 20.0, 22.0),    // hameau
            (20.0, 2.0, 34.0, 16.0),   // promontoire
            (9.0, -9.0, 19.0, 1.0),    // col venté (zone de vent lisible)
        ];
        // Clairières de la forêt : rayon 6 autour des spawns des créatures 6
        // (chauve-souris) et 12 (félin) — leurs territoires restent dégagés.
        const EXCL_CLAIRIERES: &[Rect] = &[(14.0, -26.0, 26.0, -14.0), (18.0, -20.0, 30.0, -8.0)];

        let spawns: Vec<(f32, f32)> = creatures::MMORPG_CREATURES
            .iter()
            .map(|c| (c.spawn.x, c.spawn.z))
            .collect();
        let mut solid_spots: Vec<(f32, f32)> = NATURE_DECOR
            .iter()
            .chain(VILLAGE_PROPS.iter())
            .filter(|d| d.solide)
            .map(|d| d.pos)
            .collect();
        let mut rng = crate::runtime::rng::Rng::new(0x4E41_5455_5245_3732); // « NATURE72 »
        type Poser<'a> =
            dyn FnMut(&str, &'static str, f32, f32, f32, f32, bool, Option<&'static str>) + 'a;
        #[allow(clippy::too_many_arguments)]
        fn scatter(
            rng: &mut crate::runtime::rng::Rng,
            poser: &mut Poser<'_>,
            solid_spots: &mut Vec<(f32, f32)>,
            spawns: &[(f32, f32)],
            exclusions: &[&[Rect]],
            files: &[&'static str],
            prefix: &str,
            rect: Rect,
            n: usize,
            scale: (f32, f32),
            solide: bool,
        ) {
            let mut poses = 0usize;
            let mut essais = 0usize;
            while poses < n && essais < n * 40 {
                essais += 1;
                let x = rng.next_range(rect.0, rect.2);
                let z = rng.next_range(rect.1, rect.3);
                if exclusions
                    .iter()
                    .flat_map(|g| g.iter())
                    .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
                {
                    continue;
                }
                let d2 = |(sx, sz): (f32, f32)| (sx - x) * (sx - x) + (sz - z) * (sz - z);
                if spawns.iter().any(|&s| d2(s) < 16.0) {
                    continue; // < 4 m d'un spawn de créature
                }
                if solide && solid_spots.iter().any(|&s| d2(s) < 6.25) {
                    continue; // < 2,5 m d'un autre solide
                }
                poses += 1;
                let file = files[rng.next_below(files.len())];
                let s = rng.next_range(scale.0, scale.1);
                let yaw = rng.next_range(0.0, 360.0);
                poser(
                    &format!("{prefix} {poses}"),
                    file,
                    x,
                    z,
                    s,
                    yaw,
                    solide,
                    None,
                );
                if solide {
                    solid_spots.push((x, z));
                }
            }
        }

        // Variante « en bosquets » de `scatter` : au lieu d'un tirage uniforme
        // dans tout le rectangle (visuellement un peu quadrillé malgré le
        // RNG), tire d'abord `n_clusters` centres, puis disperse
        // `per_cluster.0..per_cluster.1` instances autour de chacun dans un
        // disque de rayon `cluster_radius` — tirage en AIRE uniforme
        // (`r = radius * sqrt(u)`, pas `r = radius * u` qui sur-représenterait
        // le centre) pour un semis de bosquet crédible, façon sous-bois réel.
        // Mêmes règles de rejet que `scatter` (exclusions, spawns, solides).
        #[allow(clippy::too_many_arguments)]
        fn scatter_clustered(
            rng: &mut crate::runtime::rng::Rng,
            poser: &mut Poser<'_>,
            solid_spots: &mut Vec<(f32, f32)>,
            spawns: &[(f32, f32)],
            exclusions: &[&[Rect]],
            files: &[&'static str],
            prefix: &str,
            rect: Rect,
            n_clusters: usize,
            per_cluster: (usize, usize),
            cluster_radius: f32,
            scale: (f32, f32),
            solide: bool,
        ) {
            let mut poses = 0usize;
            for c in 0..n_clusters {
                // Centre du bosquet : rejeté s'il tombe dans une exclusion (le
                // bosquet entier resterait alors coincé contre une zone
                // aménagée) — pas de contrainte spawn/solide ici, seules les
                // instances individuelles comptent pour ces règles.
                let mut center = None;
                for _ in 0..20 {
                    let cx = rng.next_range(rect.0, rect.2);
                    let cz = rng.next_range(rect.1, rect.3);
                    if !exclusions
                        .iter()
                        .flat_map(|g| g.iter())
                        .any(|&(x0, z0, x1, z1)| cx >= x0 && cx <= x1 && cz >= z0 && cz <= z1)
                    {
                        center = Some((cx, cz));
                        break;
                    }
                }
                let Some((cx, cz)) = center else { continue };
                let n = per_cluster.0
                    + if per_cluster.1 > per_cluster.0 {
                        rng.next_below(per_cluster.1 - per_cluster.0 + 1)
                    } else {
                        0
                    };
                let mut placed_in_cluster = 0usize;
                let mut essais = 0usize;
                while placed_in_cluster < n && essais < n * 40 {
                    essais += 1;
                    let r = cluster_radius * rng.next_range(0.0, 1.0).sqrt();
                    let a = rng.next_range(0.0, std::f32::consts::TAU);
                    let x = cx + r * a.cos();
                    let z = cz + r * a.sin();
                    if x < rect.0 || x > rect.2 || z < rect.1 || z > rect.3 {
                        continue;
                    }
                    if exclusions
                        .iter()
                        .flat_map(|g| g.iter())
                        .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
                    {
                        continue;
                    }
                    let d2 = |(sx, sz): (f32, f32)| (sx - x) * (sx - x) + (sz - z) * (sz - z);
                    if spawns.iter().any(|&s| d2(s) < 16.0) {
                        continue;
                    }
                    if solide && solid_spots.iter().any(|&s| d2(s) < 6.25) {
                        continue;
                    }
                    placed_in_cluster += 1;
                    poses += 1;
                    let file = files[rng.next_below(files.len())];
                    let s = rng.next_range(scale.0, scale.1);
                    let yaw = rng.next_range(0.0, 360.0);
                    poser(
                        &format!("{prefix} {c}-{poses}"),
                        file,
                        x,
                        z,
                        s,
                        yaw,
                        solide,
                        None,
                    );
                    if solide {
                        solid_spots.push((x, z));
                    }
                }
            }
        }

        // Forêt dense du nord-est (évite les deux clairières) : feuillus mêlés
        // aux sapins, souches, sous-bois traversable.
        let foret: Rect = (8.0, -34.0, 34.0, -8.0);
        let excl_foret: &[&[Rect]] = &[EXCL_EAU_ROUTES, EXCL_ZONES_AMENAGEES, EXCL_CLAIRIERES];
        let excl_std: &[&[Rect]] = &[EXCL_EAU_ROUTES, EXCL_ZONES_AMENAGEES];

        // Quatre petites « Halte » à mi-distance (10-20 m du spawn, un point
        // par biome principal) : le regard n'avait aucun échelon entre le vide
        // proche et le mur lointain du biome. Chaque halte = un solide (arbre/
        // rocher/souche) + un compagnon non solide tout proche (la contrainte
        // de 2 m ne s'applique qu'aux solides entre eux, cf.
        // `mmorpg_solid_decor_stays_inside_and_spaced`) ; positions choisies à
        // ≥ 4 m de tout spawn de créature et hors zones aménagées. Posées AVANT
        // tout le scatter procédural ci-dessous (leurs positions rejoignent
        // `solid_spots` immédiatement) pour que forêt/prairie/lisières les
        // évitent d'elles-mêmes plutôt que de risquer une fusion visuelle
        // découverte après coup par `mmorpg_solid_decor_stays_inside_and_spaced`.
        struct Halte {
            name: &'static str,
            file: &'static str,
            pos: (f32, f32),
            scale: f32,
            yaw_deg: f32,
            solide: bool,
        }
        const MMORPG_HALTES: &[Halte] = &[
            // Vers la forêt nord-est.
            Halte {
                name: "Halte NE rocher",
                file: "nature_rock.glb",
                pos: (7.5, -11.5),
                scale: 0.8,
                yaw_deg: 35.0,
                solide: true,
            },
            Halte {
                name: "Halte NE fougère",
                file: "nature_fern.glb",
                pos: (8.3, -10.5),
                scale: 1.1,
                yaw_deg: 0.0,
                solide: false,
            },
            // Vers le lac et les rivières (ouest).
            Halte {
                name: "Halte Ouest souche",
                file: "nature_stump.glb",
                pos: (-11.5, -6.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
            },
            Halte {
                name: "Halte Ouest fleurs",
                file: "nature_daisies.glb",
                pos: (-10.3, -5.3),
                scale: 1.1,
                yaw_deg: 0.0,
                solide: false,
            },
            // Vers les rizières (sud-ouest).
            Halte {
                name: "Halte Sud-Ouest arbre",
                file: "nature_willow.glb",
                pos: (-6.0, 12.0),
                scale: 0.9,
                yaw_deg: 200.0,
                solide: true,
            },
            Halte {
                name: "Halte Sud-Ouest fleurs",
                file: "nature_lavender.glb",
                pos: (-4.8, 11.3),
                scale: 1.1,
                yaw_deg: 0.0,
                solide: false,
            },
            // Vers le hameau et le promontoire (est).
            Halte {
                name: "Halte Est rocher",
                file: "nature_rock.glb",
                // Pas (13, 4) : à 1,4 m de « Bannière » (landmark posé à la
                // main du hameau, cf. `NATURE_DECOR`) — constaté par
                // `mmorpg_solid_decor_stays_inside_and_spaced`. Décalé plus au
                // nord, dans la bande étroite (z 1..4) qui échappe à la fois
                // au col venté (z ≤ 1) et au hameau (z ≥ 4).
                pos: (15.0, 2.5),
                scale: 0.85,
                yaw_deg: 150.0,
                solide: true,
            },
            Halte {
                name: "Halte Est fleurs",
                file: "nature_sunflowers.glb",
                pos: (15.8, 1.7),
                scale: 1.1,
                yaw_deg: 0.0,
                solide: false,
            },
        ];
        for h in MMORPG_HALTES {
            poser(
                h.name, h.file, h.pos.0, h.pos.1, h.scale, h.yaw_deg, h.solide, None,
            );
            if h.solide {
                solid_spots.push(h.pos);
            }
        }

        // Variété de la lisière forêt/prairie (bord sud-ouest, x 9..15,5, z
        // -17,5..-12 — le segment de forêt le plus proche du regard du
        // joueur depuis la prairie) : le remplissage `Arbre`/`Sapin`
        // ci-dessous y répète surtout arbres/arbres2/sapins/sapins2, un mur
        // de silhouettes similaires constaté sur une capture en jeu.
        // Positions FIXES (pas de tirage RNG, contrairement à `scatter`) :
        // cette portion de `foret` est déjà saturée à ~69 % par le
        // remplissage suivant (cf. son propre commentaire) — un tirage
        // aléatoire y échoue près de 100 % du temps (constaté : 8 demandés,
        // 0 placés). Réservées dans `solid_spots` avant ledit remplissage,
        // qui les évite de lui-même. Même préfixe « Arbre exotique » que le
        // bosquet A) plus bas : aucun nouveau préfixe à ajouter à l'outil de
        // synchro. Espacées ≥ 2,5 m entre elles et du Halte NE tout proche,
        // ≥ 4 m des spawns des créatures dont le territoire touche ce coin
        // (RAY_DIST 3,5 m + marge, cf. `mmorpg_demo_contains_walkable_nature_decor`).
        for (name, file, x, z) in [
            (
                "Arbre exotique bouleau de lisière",
                "nature_birch.glb",
                9.0,
                -15.0,
            ),
            (
                "Arbre exotique chêne de lisière",
                "nature_oak.glb",
                13.5,
                -15.0,
            ),
            (
                "Arbre exotique érable de lisière",
                "nature_maple_autumn.glb",
                11.0,
                -17.5,
            ),
            (
                "Arbre exotique ginkgo de lisière",
                "nature_ginkgo.glb",
                15.5,
                -12.0,
            ),
        ] {
            poser(name, file, x, z, 1.0, 0.0, true, None);
            solid_spots.push((x, z));
        }
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_tree.glb", "nature_tree2.glb"],
            "Arbre",
            foret,
            // 22 → 28 : le hameau fortifié (Medieval Village Pack) mord sur ce
            // rectangle et son décor solide fait échouer plus de tirages
            // (rejection sampling à ≥ 2,5 m d'un autre solide) — on compense
            // pour garder ≥ 30 arbres/sapins (cf. test de densité).
            // 28 → 31 : les spawns des créatures 21-26 (éléphanteau + pack
            // savane & terreurs) décalent le flux RNG du scatter — marge pour
            // garder l'invariant ≥ 30 sans rejouer cette compensation à chaque
            // nouveau spawn.
            // 31 → 40 : les 4 arbres de lisière + le rocher du Halte NE
            // ci-dessus réservent désormais des places dans `foret` avant ce
            // tirage (rejection sampling à ≥ 2,5 m), qui en trouve donc moins
            // — reconstaté par comptage direct (25 arbres pour n=31) ; 40
            // restaure une marge confortable au-dessus du minimum testé.
            40,
            (0.9, 1.3),
            true,
        );
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_pine.glb", "nature_pine2.glb"],
            "Sapin",
            foret,
            // 15 → 20 : même compensation que ci-dessus pour `Arbre` (places
            // réservées par la lisière/Halte), constaté par comptage direct.
            20,
            (0.9, 1.25),
            true,
        );
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_stump.glb"],
            "Souche",
            foret,
            5,
            (0.9, 1.2),
            true,
        );
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_bush.glb"],
            "Buisson",
            foret,
            4,
            (2, 4),
            2.5,
            (0.9, 1.4),
            false,
        );
        // Couverture d'herbe/fougères du sous-bois : non solide, coût nul sur
        // le budget physique (aucun collider), rendu batché — la densité
        // vient de là plutôt que de multiplier les solides.
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_grass_tuft.glb", "nature_fern.glb"],
            "Sous-bois",
            foret,
            18,
            (4, 8),
            1.8,
            (0.85, 1.3),
            false,
        );
        // Lisières : quelques arbres épars le long du mur ouest (au-delà de la
        // rivière nord) et au sud-est (la lande du ver des sables reste ouverte).
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_tree.glb", "nature_pine.glb"],
            "Arbre de lisière",
            (-35.0, -34.0, -29.0, -12.0),
            5,
            (0.9, 1.2),
            true,
        );
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_tree2.glb", "nature_pine2.glb"],
            "Arbre du sud",
            (22.0, 30.0, 34.0, 35.0),
            5,
            (0.9, 1.2),
            true,
        );
        // Prairie centrale : fleurs et buissons traversables uniquement — les
        // cinq errants et le joueur y circulent sans obstacle.
        let prairie: Rect = (-10.0, -12.0, 8.0, 8.0);
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_flowers.glb"],
            "Fleurs",
            prairie,
            12,
            (0.9, 1.4),
            false,
        );
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_bush.glb"],
            "Buisson fleuri",
            prairie,
            3,
            (2, 3),
            2.2,
            (0.9, 1.3),
            false,
        );
        // Herbe basse de la prairie centrale : même logique de bosquets que
        // le sous-bois, teinte plus claire (grass_tuft seul, pas de fougère
        // sombre de forêt).
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_grass_tuft.glb"],
            "Herbe",
            prairie,
            10,
            (6, 10),
            2.0,
            (0.85, 1.3),
            false,
        );

        // --- Élargissement de la prairie centrale ---------------------------
        // Le rect `prairie` ci-dessus (-10,-12)-(8,8) concentre tout le semis
        // près du spawn ; entre lui et les quatre « Repère » (±15, ±15) qui
        // bordent la prairie restait un large anneau d'herbe nue (constaté sur
        // une capture en jeu : grand aplat vert vide). Comble cet anneau sans
        // toucher aux biomes voisins (forêt/hameau/lac/rizières/promontoire),
        // ni au rect déjà dense, ni aux abords du spawn joueur et des Repère
        // (rien ne doit gêner la vue au tout premier coup d'œil).
        const PRAIRIE_DEJA_SEMEE: &[Rect] = &[(-10.0, -12.0, 8.0, 8.0)];
        // Même bornes que le rect `foret` : la prairie élargie ne doit jamais
        // mordre sur la forêt (son propre semis gère sa densité).
        const EXCL_FORET_ZONE: &[Rect] = &[(8.0, -34.0, 34.0, -8.0)];
        // Dégagement (6×6 m) autour de chacun des quatre Repère.
        const EXCL_REPERES: &[Rect] = &[
            (-18.0, -18.0, -12.0, -12.0),
            (12.0, -18.0, 18.0, -12.0),
            (-18.0, 12.0, -12.0, 18.0),
            (12.0, 12.0, 18.0, 18.0),
        ];
        // Dégagement (8×8 m) autour du spawn du joueur (0, 0).
        const EXCL_SPAWN_JOUEUR: &[Rect] = &[(-4.0, -4.0, 4.0, 4.0)];
        let excl_prairie_large: &[&[Rect]] = &[
            EXCL_EAU_ROUTES,
            EXCL_ZONES_AMENAGEES,
            EXCL_FORET_ZONE,
            PRAIRIE_DEJA_SEMEE,
            EXCL_REPERES,
            EXCL_SPAWN_JOUEUR,
        ];
        let prairie_large: Rect = (-16.0, -16.0, 16.0, 16.0);

        // Touffes/fougères éparses (non solides) : le gros de la densité
        // visuelle, sans peser sur la broad-phase des raycasts.
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_prairie_large,
            &["nature_grass_tuft.glb", "nature_fern.glb"],
            "Prairie centrale herbe",
            prairie_large,
            8,
            (3, 6),
            2.2,
            (0.85, 1.25),
            false,
        );
        // Fleurs des prés (non solides), variété différente de celles déjà
        // semées dans le rect dense pour ne pas juste répéter le motif.
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_prairie_large,
            &[
                "nature_daisies.glb",
                "nature_irises.glb",
                "nature_lavender.glb",
            ],
            "Prairie centrale fleur",
            prairie_large,
            10,
            (0.85, 1.3),
            false,
        );
        // Petits rochers isolés (solides, échelle réduite pour rester
        // discrets — pas l'anneau du promontoire).
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_prairie_large,
            &["nature_rock.glb"],
            "Prairie centrale rocher",
            prairie_large,
            4,
            (0.5, 0.75),
            true,
        );
        // Un ou deux arbres isolés (solides) : jamais un bosquet, juste de
        // quoi casser la platitude du grand aplat vert.
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_prairie_large,
            &["nature_oak.glb", "nature_tree.glb"],
            "Prairie centrale arbre isolé",
            prairie_large,
            2,
            (0.8, 1.0),
            true,
        );

        // Plants de riz dans chaque bassin (traversables) : le scatter vise
        // l'intérieur du bassin, seules les zones eau/routes le repoussent.
        let excl_riz: &[&[Rect]] = &[EXCL_EAU_ROUTES];
        for (i, &(x0, z0, x1, z1)) in EXCL_ZONES_AMENAGEES[..5].iter().enumerate() {
            scatter(
                &mut rng,
                &mut poser,
                &mut solid_spots,
                &spawns,
                excl_riz,
                &["nature_rice.glb"],
                &format!("Riz {}", ["A", "B", "C", "D", "E"][i]),
                (x0 + 0.7, z0 + 0.7, x1 - 0.7, z1 - 0.7),
                3,
                (0.9, 1.3),
                false,
            );
        }

        // Roseaux/nénuphars systématiques le long des 4 berges (en plus des
        // quelques instances posées à la main dans `NATURE_DECOR`, gardées
        // telles quelles) : un point tiré sur le PÉRIMÈTRE de chaque rect
        // d'eau (`EXCL_EAU_ROUTES[..4]`, les 4 vrais plans d'eau — pas la
        // berge de sable ni les routes), décalé perpendiculairement côté
        // terre pour les roseaux, côté eau pour les nénuphars (flottent, non
        // solides dans les deux cas). Bien plus systématique que les 7
        // instances isolées d'origine.
        for &(x0, z0, x1, z1) in &EXCL_EAU_ROUTES[..4] {
            for i in 0..7 {
                let side = rng.next_below(4);
                let along = rng.next_range(0.0, 1.0);
                let (bx, bz, nx, nz): (f32, f32, f32, f32) = match side {
                    0 => (x0 + along * (x1 - x0), z0, 0.0, -1.0), // nord
                    1 => (x0 + along * (x1 - x0), z1, 0.0, 1.0),  // sud
                    2 => (x0, z0 + along * (z1 - z0), -1.0, 0.0), // ouest
                    _ => (x1, z0 + along * (z1 - z0), 1.0, 0.0),  // est
                };
                let is_reed = i % 2 == 0;
                let offset = rng.next_range(0.3, 0.8);
                let (px, pz) = if is_reed {
                    (bx + nx * offset, bz + nz * offset)
                } else {
                    (bx - nx * offset, bz - nz * offset)
                };
                if spawns.iter().any(|&(sx, sz)| {
                    let dx = sx - px;
                    let dz = sz - pz;
                    dx * dx + dz * dz < 16.0
                }) {
                    continue;
                }
                let file = if is_reed {
                    "nature_reeds.glb"
                } else {
                    "nature_lily.glb"
                };
                let scale = rng.next_range(0.85, 1.15);
                let yaw = rng.next_range(0.0, 360.0);
                poser(
                    &format!(
                        "Berge {} {i}",
                        if is_reed { "Roseaux" } else { "Nénuphars" }
                    ),
                    file,
                    px,
                    pz,
                    scale,
                    yaw,
                    false,
                    None,
                );
            }
        }

        // --- Flore complémentaire + objets décoratifs (packs générés Blender
        //     headless, gen_creature_pack*.py / gen_nature_pack*.py) ------------
        // Contrairement à `scatter`/`scatter_clustered` (tirage aléatoire avec
        // remise dans une liste de fichiers), chaque fichier ci-dessous doit
        // apparaître AU MOINS une fois dans la scène (sinon un asset généré
        // resterait inutilisé) : `scatter_each` place chaque fichier de la liste
        // exactement une fois, avec le même rejet de zones aménagées / spawns /
        // chevauchement de solides que `scatter`, mais sans tirage avec remise.
        #[allow(clippy::too_many_arguments)]
        fn scatter_each(
            rng: &mut crate::runtime::rng::Rng,
            poser: &mut Poser<'_>,
            solid_spots: &mut Vec<(f32, f32)>,
            spawns: &[(f32, f32)],
            exclusions: &[&[Rect]],
            files: &[&'static str],
            prefix: &str,
            rect: Rect,
            scale: (f32, f32),
            solide: bool,
        ) {
            for &file in files {
                let mut placed = false;
                for _ in 0..4000 {
                    let x = rng.next_range(rect.0, rect.2);
                    let z = rng.next_range(rect.1, rect.3);
                    if exclusions
                        .iter()
                        .flat_map(|g| g.iter())
                        .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
                    {
                        continue;
                    }
                    let d2 = |(sx, sz): (f32, f32)| (sx - x) * (sx - x) + (sz - z) * (sz - z);
                    if spawns.iter().any(|&s| d2(s) < 16.0) {
                        continue;
                    }
                    if solide && solid_spots.iter().any(|&s| d2(s) < 6.25) {
                        continue;
                    }
                    let s = rng.next_range(scale.0, scale.1);
                    let yaw = rng.next_range(0.0, 360.0);
                    let short = file
                        .trim_start_matches("nature_")
                        .trim_start_matches("item_")
                        .trim_end_matches(".glb");
                    poser(
                        &format!("{prefix} {short}"),
                        file,
                        x,
                        z,
                        s,
                        yaw,
                        solide,
                        None,
                    );
                    if solide {
                        solid_spots.push((x, z));
                    }
                    placed = true;
                    break;
                }
                if !placed {
                    log::error!("« {prefix} » : impossible de placer {file} sans chevauchement");
                }
            }
        }

        // A) Arbres exotiques (packs faune d'Asie / fantastique / marin, décor
        //    végétal) : forêt nord-est, solides comme les arbres existants.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &[
                "nature_apple_tree.glb",
                "nature_bamboo.glb",
                "nature_birch.glb",
                "nature_cherry_blossom.glb",
                "nature_cypress.glb",
                "nature_dead_tree.glb",
                "nature_ginkgo.glb",
                "nature_hazel.glb",
                "nature_magnolia.glb",
                "nature_maple_autumn.glb",
                "nature_oak.glb",
                "nature_olive.glb",
                "nature_palm.glb",
                "nature_pine_parasol.glb",
                "nature_plum.glb",
                "nature_poplar.glb",
                "nature_sequoia.glb",
                "nature_tree_windswept.glb",
            ],
            "Arbre exotique",
            // Pas `foret` (déjà saturé à ~69 % de sa surface utile par les
            // arbres/sapins/souches du scatter existant, au-delà du seuil de
            // remplissage aléatoire (« jamming ») pour un rejet à ≥ 2,5 m —
            // 300 tirages par fichier n'y trouvaient plus jamais de place,
            // constaté par comptage direct). Bosquet complémentaire au
            // sud-est de la forêt, hors rizières (x max 9) et hors hameau/
            // promontoire (`EXCL_ZONES_AMENAGEES`), quasi vierge de décor
            // solide.
            (9.0, 20.0, 35.5, 35.5),
            (0.85, 1.2),
            true,
        );
        // B) Mobilier villageois (fontaine, meule, puits à poulie, balançoire,
        //    topiaire, tonnelle glycine…) : posé dans le hameau lui-même (zone
        //    aménagée non exclue ici), à ≥ 2,5 m de tout autre solide déjà posé
        //    à la main dans `NATURE_DECOR`/`VILLAGE_PROPS`.
        let excl_hameau_only: &[&[Rect]] = &[EXCL_EAU_ROUTES];
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "nature_fountain.glb",
                "nature_well_pulley.glb",
                "nature_grindstone.glb",
                "nature_hay_roller.glb",
                "nature_potters_wheel.glb",
                "nature_wheelbarrow.glb",
                "nature_swing_bench.glb",
                "nature_topiary.glb",
                "nature_vine_trellis.glb",
                "nature_wisteria_arch.glb",
                "nature_sundial.glb",
                "nature_birdhouse.glb",
            ],
            "Mobilier du hameau",
            (1.0, 2.0, 21.0, 23.0),
            (0.9, 1.1),
            true,
        );
        // C) Rochers moussus de bord de lac (moss_boulder/mossy_log) : rive
        //    ouest du lac. Pas le rect exact de la « Berge du lac » (aplat
        //    sable) : ce rect est LUI-MÊME une entrée d'`EXCL_EAU_ROUTES`, donc
        //    100 % des tirages y étaient rejetés (constaté : 0/2 placés).
        //    Juste à l'ouest, hors de tout aplat eau/route/berge.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &["nature_moss_boulder.glb", "nature_mossy_log.glb"],
            "Rocher moussu",
            (-30.0, -4.0, -26.0, 8.0),
            (0.9, 1.2),
            true,
        );
        // D) Sous-bois exotique (non solide : champignons, houx, ronces,
        //    girouettes/oriflammes de prière plantées au sol).
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &[
                "nature_giant_mushroom.glb",
                "nature_mushrooms.glb",
                "nature_holly.glb",
                "nature_bramble.glb",
                "nature_mast_flag.glb",
                "nature_prayer_flags.glb",
            ],
            "Sous-bois exotique",
            foret,
            (0.85, 1.2),
            false,
        );
        // E) Fleurs des prés (non solides) : prairie centrale.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &[
                "nature_daisies.glb",
                "nature_irises.glb",
                "nature_lavender.glb",
                "nature_sunflowers.glb",
                "nature_sunflowers_sway.glb",
                "nature_thistle.glb",
                "nature_windsock.glb",
            ],
            "Fleur des prés",
            prairie,
            (0.85, 1.3),
            false,
        );
        // F) Cultures complémentaires (non solides) : bande des rizières,
        //    exclusions restreintes à l'eau/aux routes (comme le riz existant)
        //    pour pouvoir tomber DANS les bassins.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_riz,
            &[
                "nature_cabbages.glb",
                "nature_carrots.glb",
                "nature_corn.glb",
                "nature_pumpkins.glb",
                "nature_tomatoes.glb",
                "nature_wheat.glb",
                "nature_wheat_sway.glb",
            ],
            "Culture",
            (-11.0, 23.0, 9.0, 35.0),
            (0.85, 1.25),
            false,
        );
        // G) Rives du lac et des rivières (non solides) : roseaux/nénuphars
        //    existants complétés par saules, bambou et barque flottante.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "nature_cattails.glb",
                "nature_reeds_sway.glb",
                "nature_willow.glb",
                "nature_willow_sway.glb",
                "nature_boat_bob.glb",
                "nature_bamboo_sway.glb",
            ],
            "Rive du lac",
            (-30.0, -14.0, -10.0, 16.0),
            (0.85, 1.2),
            false,
        );
        // H) Petit décor du hameau (non solide) : lanterne suspendue, buisson à
        //    baies.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &["nature_lantern_hanging.glb", "nature_berry_bush.glb"],
            "Décor du hameau",
            (1.0, 2.0, 21.0, 23.0),
            (0.9, 1.1),
            false,
        );

        // I) Objets décoratifs (`item_*`, packs générés) : PURE décor visuel,
        //    aucune mécanique de ramassage (pas d'`ItemPickup`, à ne pas
        //    confondre avec `MMORPG_ITEMS` plus bas) — regroupés en petites
        //    scènes cohérentes posées au sol près du hameau, non solides,
        //    petite échelle.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "item_axe.glb",
                "item_bow.glb",
                "item_hammer.glb",
                "item_shield.glb",
                "item_sword.glb",
                "item_ball.glb",
                "item_bomb.glb",
            ],
            "Établi d'armes",
            (13.0, 5.0, 16.0, 7.0),
            (0.3, 0.4),
            false,
        );
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "item_apple.glb",
                "item_bread.glb",
                "item_carrot.glb",
                "item_cheese.glb",
                "item_egg.glb",
                "item_fish.glb",
                "item_meat.glb",
                "item_mushroom.glb",
                "item_berry.glb",
            ],
            // Pas « Étal du marché » : déjà pris par `VILLAGE_PROPS` (« Étal du
            // marché 1/2 », `hamlet_market_stand_*`) — un préfixe partagé
            // ferait retirer/réinjecter ces deux landmarks par erreur dans
            // l'outil de synchro du décor ambiant (cf. `AMBIENT_DECOR_PREFIXES`
            // dans `scene::mod`).
            "Étal des vivres",
            (16.5, 7.5, 19.5, 9.5),
            (0.3, 0.4),
            false,
        );
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "item_coin.glb",
                "item_gem.glb",
                "item_crown.glb",
                "item_ring.glb",
                "item_star.glb",
            ],
            "Coin trésor",
            (6.5, 5.0, 9.0, 6.5),
            (0.3, 0.4),
            false,
        );
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "item_potion.glb",
                "item_mana.glb",
                "item_scroll.glb",
                "item_book.glb",
                "item_feather.glb",
                "item_heart.glb",
                "item_key.glb",
                "item_lantern.glb",
                "item_pouch.glb",
            ],
            "Table d'apothicaire",
            (3.0, 5.5, 6.0, 6.5),
            (0.3, 0.4),
            false,
        );

        // Objets d'inventaire (cf. `ItemPickup`) à trouver en explorant, posés
        // sur la narration de chaque biome : potions devant la Cabane 1 et aux
        // sorties des ponts, baies au bord des rizières, clé au pied de la tour
        // de guet, gemmes au promontoire et au fond de la forêt. Non solides
        // (`demo_obj` = `PhysicsKind::None`) : invisibles aux sondes raycast
        // des créatures, donc sans effet sur les patrouilles.
        struct DemoItem {
            name: &'static str,
            kind: ItemKind,
            count: u32,
            pos: (f32, f32, f32),
            /// > 0 ⇒ l'objet repousse (buisson à baies) ; 0 ⇒ trouvaille unique.
            respawn: f32,
        }
        const MMORPG_ITEMS: &[DemoItem] = &[
            // Devant la porte de la Cabane 1 (6, 9 — porte côté route).
            DemoItem {
                name: "Potion de soin",
                kind: ItemKind::Potion,
                count: 1,
                pos: (6.0, 0.35, 11.2),
                respawn: 0.0,
            },
            // Sortie est du Pont 1 (-16, 14).
            DemoItem {
                name: "Potion de soin 2",
                kind: ItemKind::Potion,
                count: 1,
                pos: (-12.6, 0.35, 14.0),
                respawn: 0.0,
            },
            // Sortie est du Pont 2 (-26, -10), sur le chemin de la forêt.
            DemoItem {
                name: "Potion de soin 3",
                kind: ItemKind::Potion,
                count: 1,
                pos: (-23.0, 0.35, -10.0),
                respawn: 0.0,
            },
            // Bord nord des rizières : 2 baies par cueillette, repousse en 20 s.
            DemoItem {
                name: "Buisson à baies",
                kind: ItemKind::Baie,
                count: 2,
                pos: (-4.0, 0.3, 22.5),
                respawn: 20.0,
            },
            // Au pied de la tour de guet (27, 9).
            DemoItem {
                name: "Clé du village",
                kind: ItemKind::Cle,
                count: 1,
                pos: (25.9, 0.3, 7.6),
                respawn: 0.0,
            },
            // Dans l'anneau de rochers du promontoire.
            DemoItem {
                name: "Gemme",
                kind: ItemKind::Gemme,
                count: 1,
                pos: (30.9, 0.3, 12.6),
                respawn: 0.0,
            },
            // Au fond de la forêt dense : la récompense de l'exploration.
            DemoItem {
                name: "Gemme de la forêt",
                kind: ItemKind::Gemme,
                count: 1,
                pos: (30.0, 0.3, -30.0),
                respawn: 0.0,
            },
        ];
        for spec in MMORPG_ITEMS {
            let mesh = if spec.kind == ItemKind::Potion {
                MeshKind::Capsule
            } else {
                MeshKind::Sphere
            };
            let mut item = demo_obj(
                spec.name,
                mesh,
                Vec3::new(spec.pos.0, spec.pos.1, spec.pos.2),
            );
            item.transform = item.transform.with_scale(Vec3::splat(0.35));
            item.color = spec.kind.color();
            item.emissive = 0.8;
            item.respawn_delay = spec.respawn;
            item.item_pickup = Some(ItemPickup {
                kind: spec.kind,
                count: spec.count,
            });
            objects.push(item);
        }

        // Les Braises (GDD §2.1) sont la fiction du jeu : « c'est [le feu
        // communal] qui attire les hordes ». La charte (§10.1 « au centre,
        // les braises ; au loin, le danger », §10.2 orange = système
        // feu/joueur) exige que ce soit le point chaud/saturé le plus
        // lisible de la carte — jusqu'ici posé comme n'importe quel décor
        // inerte (pas d'émissif). Marquage a posteriori (pas de champ
        // couleur/émissif sur `DemoDecor`, partagé par ~150 entrées neutres)
        // plutôt qu'une extension de la table pour deux objets seulement.
        for name in ["Feu du hameau", "Feu de camp"] {
            if let Some(feu) = objects.iter_mut().find(|o| o.name == name) {
                feu.emissive = 1.2;
            }
        }

        // Convoi (GDD §4, mode Escorte) : jusqu'ici absent de la scène réseau
        // réelle (`mmorpg_demo`, embarquée via `player_scene.json`) — seule
        // `Scene::escorte_demo()` (solo) en avait un. Conséquence mécanique
        // vérifiée (Phase L, `sprintreflecion.md`) : un salon réseau qui
        // choisit `RoundObjective::Escorte` ne se terminait jamais
        // (`AppState::update_escorte`/`is_convoy_destroyed` retournent tôt
        // sans rien faire quand aucun objet `convoy` n'existe, cf.
        // `src/app/combat.rs`/`src/app/health.rs`). Même modèle
        // (`nature_cart.glb`) et mêmes composants (`Combat`/`Convoy`) que
        // `escorte_demo`, positionné sur la route principale (bande z
        // 12.3–15.7, exclue du scatter procédural ci-dessus — cf.
        // `EXCL_EAU_ROUTES`), entre le col venté et le hameau : x -18 → -2,
        // à l'écart des collines de l'Ouest (x < -27) et des bâtiments du
        // hameau (x > 2).
        {
            let convoi_mesh = import_single_model(&mut imported, "nature_cart.glb", MeshKind::Cube);
            let mut convoi = demo_obj(
                "Convoi — chariot de braises",
                convoi_mesh,
                Vec3::new(-18.0, 0.0, 14.0),
            );
            convoi.transform.rotation = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
            convoi.emissive = 0.3;
            convoi.combat = Some(Combat {
                attackable: true,
                hp: 8,
                ..Default::default()
            });
            convoi.convoy = Some(Convoy {
                destination: Vec3::new(-2.0, 0.0, 14.0),
                speed: 1.2,
            });
            objects.push(convoi);
        }

        Scene {
            objects,
            imported,
            camera_follow: true,
            point_lights: vec![
                // Éclairage général : hissé et élargi avec la carte 72×72 (l'ancien
                // range de 30 m laissait les biomes périphériques dans le noir).
                PointLight {
                    position: [0.0, 18.0, 0.0],
                    color: [0.9, 0.95, 1.0],
                    intensity: 1.4,
                    range: 90.0,
                    ..PointLight::default()
                },
                // Deux lampes chaudes au hameau (cf. les lanternes du décor) : la
                // zone habitée se repère de loin, même à contre-jour du soleil.
                PointLight {
                    position: [10.0, 3.0, 12.0],
                    color: [1.0, 0.75, 0.4],
                    intensity: 1.1,
                    range: 12.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [2.0, 3.0, 12.0],
                    color: [1.0, 0.75, 0.4],
                    intensity: 1.1,
                    range: 12.0,
                    ..PointLight::default()
                },
                // Les Braises (GDD §2.1/§10.1) : le feu communal du hameau
                // (forge/scierie, x≈28/z≈-29) est hors de portée des deux
                // lampes ci-dessus — il n'avait aucune source de lumière
                // propre alors qu'il est la fiction centrale du jeu.
                PointLight {
                    position: [28.0, 1.2, -29.0],
                    color: [1.0, 0.55, 0.15],
                    intensity: 1.6,
                    range: 14.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [19.0, 1.0, -6.0],
                    color: [1.0, 0.6, 0.2],
                    intensity: 0.9,
                    range: 8.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            // Soleil orienté sud-est : les ombres portées de la forêt, de la
            // tour et du hameau donnent le relief qu'une carte plate n'a pas.
            light: Light {
                dir: [0.55, 1.0, -0.45],
                color: [1.0, 0.96, 0.88],
                ambient: 0.35,
            },
            // Ciel de journée chaude (cohérent avec la palette du pack nature,
            // cf. gen_nature_pack.py) + brume légère : donne la profondeur
            // atmosphérique sur 72 m, adoucit les murs d'enceinte au loin, et
            // masque le pop de détail — à coût GPU nul (déjà dans le shader).
            sky: Sky {
                horizon_color: [0.85, 0.78, 0.62],
                zenith_color: [0.30, 0.52, 0.78],
                fog_color: [0.78, 0.74, 0.62],
                fog_density: 0.012,
                ..Sky::default()
            },
            ..Default::default()
        }
    }
}
