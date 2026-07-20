use super::*;

// Créatures qui errent (glb rigués/animés, cf. la doc de
// `creature_wander_script`) : seule différence avec les repères ci-dessus,
// des meshes importés skinnés plutôt que des primitives — chemins disque du
// dépôt (pas `bundle://`) car cette démo tourne depuis les sources, jamais
// depuis un export packagé. Table data-driven plutôt que dix blocs
// copiés-collés (l'historique en comptait cinq avant les créatures 6-10) :
// chaque entrée = (nom, fichier, spawn, bit de couche, préfixe de clés
// `save`, attaque au contact éventuelle).
//
// Communs à toutes : échelle 0.35 (les meshes bruts font ~2-3 m de haut,
// bbox locale non affectée par l'échelle Blender de l'objet, ignorée par
// `import::load_gltf` qui ne lit que les sommets des primitives) ; corps
// physique `Kinematic` (ne traversent ni joueur ni murs/objets fixes, cf.
// `Physics::resolve_scripted_moves`) ; couche de collision dédiée (bit
// propre) pour que leurs sondes raycast s'ignorent elles-mêmes tout en
// voyant les autres ; spawns espacés, à bonne distance des murs/repères
// (RAY_DIST 3,5 m devant chaque créature doit démarrer dégagé).
//
// Attaques : au contact (`bite`, cf. `creature_bite_script` — morsure de
// la n°1, morsure rapide de la chauve-souris n°6, pincement lourd du crabe
// n°7, chacun son tempérament et son `salt` de tirage) ; les attaques à
// distance des n°3/8/9/10 sont natives, déclenchées par **nom d'objet**
// (cf. `app::creature_attack::RANGED_CREATURE_ATTACKS`), rien à câbler ici.
pub(super) struct DemoCreature {
    name: &'static str,
    file: &'static str,
    pub(super) spawn: Vec3,
    layer_bit: u32,
    prefix: &'static str,
    /// `Some((cooldown, chance, dégâts, salt))` : attaque au contact.
    bite: Option<(f32, f32, f32, f32)>,
    /// Cap initial (degrés) et déphasage du bruit de méandre — cf. la
    /// doc de `creature_wander_script` sur le bug qu'ils corrigent
    /// (créatures qui partaient toutes dans la même direction, en bloc
    /// contre le même mur). Passés à **tous** les générateurs pour un
    /// type de pointeur de fonction uniforme ; ceux qui n'en ont pas
    /// l'usage (sentinelle, rôdeur…) les ignorent (`_heading0`/`_phase`).
    heading0: f32,
    phase: f32,
    /// Générateur du script de comportement (patrouille à sondes,
    /// sentinelle, rôdeur, dérive, artillerie, zigzag…) — signature
    /// commune (demi-arène, préfixe `save`, masque de sonde, cap
    /// initial, déphasage), chaque générateur ignore ce dont il n'a
    /// pas besoin.
    script: fn(f32, &str, u32, f32, f32) -> String,
    /// Manche d'apparition (`Combat::wave`, GDD_MMORPG.md §5.5 — la
    /// dent de scie de « La Horde ») : la vague 1 est l'échauffement
    /// (errantes inoffensives), les suivantes montent en intensité.
    /// Règles d'authoring verrouillées par le test
    /// `mmorpg_demo_waves_follow_the_gdd_authoring_rules` : budget de
    /// PV strictement croissant par vague, au moins un chef à 3 PV
    /// dès la vague 2, dernière vague ≥ 4/3 de l'avant-dernière.
    wave: u32,
    /// PV (`Combat::hp`) : 1 pour la troupe, 3 pour un « chef » — la
    /// cible qui justifie le Boulet (GDD §5.5 : « un chef à 3 PV
    /// tombe d'un coup », sans elle l'arme lourde est objectivement
    /// inférieure).
    hp: u32,
    /// Casting de chasse (GDD §5.4, chantier 4.1 audit 2026-07-20) :
    /// la grammaire Traqueuse/Meute/Colosse/Furtive appliquée à la
    /// scène servie via un `AiChaser` — comportement SEULEMENT, jamais
    /// les PV (`Archetype::hp_multiplier` n'est PAS appliqué ici : le
    /// budget de vagues ci-dessus est un contrat verrouillé par test).
    archetype: Archetype,
    /// Vitesse de base de la poursuite (m/s), multipliée par
    /// `Archetype::speed_multiplier` une fois la chasse engagée —
    /// calée pour rester sous les 4,5 m/s du joueur, sauf la pointe
    /// Furtive (rattrapable : éveil à courte portée seulement).
    chase_speed: f32,
}
pub(super) const MMORPG_CREATURES: &[DemoCreature] = &[
    DemoCreature {
        name: "Créature",
        file: "creature.glb",
        spawn: Vec3::new(0.0, 0.0, -4.0),
        layer_bit: 1,
        prefix: "creature_",
        heading0: 0.0,
        // `phase` décale le méandre du script d'errance (terme
        // `sin(time*0.35+phase)`, cf. `creature_wander_script`) : à
        // `phase = 0`, sur 30 s la dérive nette pousse systématiquement
        // vers le hameau (SE, façades planes abordées en tangente —
        // 3 rayons d'égale distance, quasi aucun signal d'évitement),
        // provoquant un arrêt prolongé. Inoffensif sur l'ancienne
        // arène 24 m (rien à heurter à cette distance), redevenu un
        // piège sur la carte 72 m. `-π/2` retarde et adoucit le
        // virage initial : la dérive longe la rizière sud, à
        // distance de tout décor solide sur les 30 s du test. Preuve :
        // `mmorpg_creature_never_gets_stuck_walking_into_a_wall`.
        phase: -std::f32::consts::FRAC_PI_2,
        bite: Some((2.2, 0.4, 0.12, 12.9898)),
        script: creature_wander_script,
        wave: 1,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 2",
        file: "creature2.glb",
        spawn: Vec3::new(6.0, 0.0, 2.0),
        layer_bit: 2,
        prefix: "creature2_",
        heading0: 72.0,
        phase: 0.9,
        bite: None,
        script: creature_wander_script,
        wave: 1,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 3",
        file: "creature3.glb",
        spawn: Vec3::new(-6.0, 0.0, 4.0),
        layer_bit: 3,
        prefix: "creature3_",
        heading0: 144.0,
        phase: 1.8,
        bite: None,
        script: creature_wander_script,
        wave: 1,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 4",
        file: "creature4.glb",
        spawn: Vec3::new(-4.0, 0.0, -9.0),
        layer_bit: 4,
        prefix: "creature4_",
        heading0: 216.0,
        phase: 2.7,
        bite: None,
        script: creature_wander_script,
        wave: 1,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 5",
        file: "creature5.glb",
        spawn: Vec3::new(7.0, 0.0, -7.0),
        layer_bit: 5,
        prefix: "creature5_",
        heading0: 288.0,
        phase: 3.6,
        bite: None,
        script: creature_wander_script,
        wave: 1,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    // Chauve-souris : morsure rapide mais faible — harcèle plus qu'elle
    // ne punit.
    DemoCreature {
        name: "Créature 6",
        file: "creature6.glb",
        spawn: Vec3::new(20.0, 0.0, -20.0),
        layer_bit: 6,
        prefix: "creature6_",
        heading0: 24.0,
        phase: 4.5,
        bite: Some((1.6, 0.5, 0.08, 19.4142)),
        script: creature_wander_script,
        wave: 2,
        hp: 1,
        archetype: Archetype::Meute,
        chase_speed: 3.0,
    },
    // Crabe : pincement rare mais lourd — l'inverse de la chauve-souris.
    DemoCreature {
        name: "Créature 7",
        file: "creature7.glb",
        // Berge est du lac, à l'écart du nouveau mur d'eau (le crabe
        // spawnait auparavant quasi sur le bord du Lac, l:[-26,-12]
        // z:[-2,10], désormais bordé de murs invisibles).
        spawn: Vec3::new(-9.0, 0.0, 14.0),
        layer_bit: 7,
        prefix: "creature7_",
        heading0: 96.0,
        phase: 5.4,
        bite: Some((3.5, 0.6, 0.18, 27.1828)),
        script: creature_wander_script,
        wave: 2,
        hp: 3,
        archetype: Archetype::Colosse,
        chase_speed: 3.4,
    },
    DemoCreature {
        name: "Créature 8",
        file: "creature8.glb",
        spawn: Vec3::new(24.0, 0.0, 18.0),
        layer_bit: 8,
        prefix: "creature8_",
        heading0: 168.0,
        phase: 6.3,
        bite: None,
        script: creature_wander_script,
        wave: 3,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 9",
        file: "creature9.glb",
        spawn: Vec3::new(-4.0, 0.0, 29.0),
        layer_bit: 9,
        prefix: "creature9_",
        heading0: 240.0,
        phase: 7.2,
        bite: None,
        script: creature_wander_script,
        wave: 3,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 10",
        file: "creature10.glb",
        spawn: Vec3::new(12.0, 0.0, -10.0),
        layer_bit: 10,
        prefix: "creature10_",
        heading0: 312.0,
        phase: 8.1,
        bite: None,
        script: creature_wander_script,
        wave: 3,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    // 11-15 : la génération « qualité supérieure » — attaques à
    // distance toutes différentes (cf. `creature_attack::AttackStyle`)
    // et comportements dédiés, cohérents avec leur attaque.
    DemoCreature {
        name: "Créature 11",
        file: "creature11.glb",
        spawn: Vec3::new(10.0, 0.0, 16.0),
        layer_bit: 11,
        prefix: "creature11_",
        heading0: 48.0,
        phase: 9.0,
        bite: None,
        script: creature_guard_script,
        wave: 3,
        hp: 3,
        archetype: Archetype::Colosse,
        chase_speed: 3.4,
    },
    DemoCreature {
        name: "Créature 12",
        file: "creature12.glb",
        spawn: Vec3::new(24.0, 0.0, -14.0),
        layer_bit: 12,
        prefix: "creature12_",
        heading0: 120.0,
        phase: 9.9,
        bite: None,
        script: creature_kite_script,
        wave: 3,
        hp: 1,
        archetype: Archetype::Meute,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 13",
        file: "creature13.glb",
        spawn: Vec3::new(-19.0, 0.0, 2.0),
        layer_bit: 13,
        prefix: "creature13_",
        heading0: 192.0,
        phase: 10.8,
        bite: None,
        script: creature_drift_script,
        wave: 3,
        hp: 1,
        archetype: Archetype::Furtive,
        chase_speed: 2.8,
    },
    DemoCreature {
        name: "Créature 14",
        file: "creature14.glb",
        spawn: Vec3::new(-12.0, 0.0, 22.0),
        layer_bit: 14,
        prefix: "creature14_",
        heading0: 264.0,
        phase: 11.7,
        bite: None,
        script: creature_artillery_script,
        wave: 4,
        hp: 3,
        archetype: Archetype::Colosse,
        chase_speed: 3.4,
    },
    DemoCreature {
        name: "Créature 15",
        file: "creature15.glb",
        spawn: Vec3::new(-20.5, 0.0, 15.5),
        layer_bit: 15,
        prefix: "creature15_",
        heading0: 336.0,
        phase: 12.6,
        bite: None,
        script: creature_zigzag_script,
        wave: 4,
        hp: 1,
        archetype: Archetype::Meute,
        chase_speed: 3.0,
    },
    // 16-20 : même palier de qualité que 11-15 — attaques et
    // comportements tout aussi variés, cf. `creature_attack.rs`.
    DemoCreature {
        name: "Créature 16",
        file: "creature16.glb",
        // Audit gameplay : l'ancien spawn (1.5, 7.5) mettait le cercle de
        // patrouille (R 3,5) à cheval sur la cabane et le mur nord —
        // blocages permanents. Est de l'arène, dégagé : rochers, arbres,
        // repères et murs tous à > 1,5 m du cercle, et hors de la zone
        // d'errance des créatures 1-5 (un premier essai au centre faisait
        // s'arrêter la n°1 à chaque passage du griffon devant ses sondes).
        spawn: Vec3::new(-2.0, 0.0, -18.0),
        layer_bit: 16,
        prefix: "creature16_",
        heading0: 12.0,
        phase: 13.5,
        bite: None,
        script: creature_soar_script,
        wave: 4,
        hp: 3,
        archetype: Archetype::Colosse,
        chase_speed: 3.4,
    },
    DemoCreature {
        name: "Créature 17",
        file: "creature17.glb",
        // Audit gameplay : l'ancien spawn (-0.5, -10.5) faisait mordre
        // l'aile sud du huit (± 1,5 en z) sur le mur — remonté pour que
        // toute la courbe reste dans l'arène, loin des rochers 2/3.
        spawn: Vec3::new(-20.0, 0.0, -26.0),
        layer_bit: 17,
        prefix: "creature17_",
        heading0: 84.0,
        phase: 14.4,
        bite: None,
        script: creature_lemniscate_script,
        wave: 4,
        hp: 1,
        archetype: Archetype::Furtive,
        chase_speed: 2.8,
    },
    DemoCreature {
        name: "Créature 18",
        file: "creature18.glb",
        spawn: Vec3::new(18.0, 0.0, 27.0),
        layer_bit: 18,
        prefix: "creature18_",
        heading0: 156.0,
        phase: 15.3,
        bite: None,
        script: creature_burrow_script,
        wave: 4,
        hp: 3,
        archetype: Archetype::Furtive,
        chase_speed: 2.8,
    },
    DemoCreature {
        name: "Créature 19",
        file: "creature19.glb",
        spawn: Vec3::new(19.5, 0.0, 2.5),
        layer_bit: 19,
        prefix: "creature19_",
        heading0: 228.0,
        phase: 16.2,
        bite: None,
        script: creature_hover_script,
        wave: 4,
        hp: 1,
        archetype: Archetype::Meute,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 20",
        file: "creature20.glb",
        spawn: Vec3::new(26.0, 0.0, 13.0),
        layer_bit: 20,
        prefix: "creature20_",
        heading0: 300.0,
        phase: 17.1,
        bite: None,
        script: creature_turret_script,
        wave: 3,
        hp: 3,
        archetype: Archetype::Colosse,
        chase_speed: 3.4,
    },
    DemoCreature {
        name: "Créature 21",
        file: "creature21.glb",
        spawn: Vec3::new(4.0, 0.0, 26.0),
        layer_bit: 21,
        prefix: "creature21_",
        heading0: 12.0,
        phase: 18.0,
        bite: None,
        script: creature_wander_script,
        wave: 2,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 22",
        file: "creature22.glb",
        spawn: Vec3::new(24.0, 0.0, -4.0),
        layer_bit: 22,
        prefix: "creature22_",
        heading0: 84.0,
        phase: 18.9,
        bite: None,
        script: creature_wander_script,
        wave: 2,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 23",
        file: "creature23.glb",
        spawn: Vec3::new(-2.0, 0.0, 22.0),
        layer_bit: 23,
        prefix: "creature23_",
        heading0: 156.0,
        phase: 19.8,
        bite: None,
        script: creature_wander_script,
        wave: 2,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 24",
        file: "creature24.glb",
        spawn: Vec3::new(-14.0, 0.0, -22.0),
        layer_bit: 24,
        prefix: "creature24_",
        heading0: 228.0,
        phase: 20.7,
        bite: None,
        // Pas `creature_burrow_script` : la charge garde un cap constant
        // pendant 1,1 s, et un corps scripté dont le TriMesh s'est
        // incrusté dans le sol (chute du 1er tick non arrêtée,
        // trimesh-vs-trimesh) peut se coincer sur un cap « malchanceux »
        // (normales de contact des triangles) — gel complet observé sur
        // ce mesh. Le méandre de wander varie son cap à chaque frame et
        // se décoince naturellement.
        script: creature_wander_script,
        wave: 2,
        hp: 1,
        archetype: Archetype::Traqueuse,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 25",
        file: "creature25.glb",
        spawn: Vec3::new(14.0, 0.0, 26.0),
        layer_bit: 25,
        prefix: "creature25_",
        heading0: 300.0,
        phase: 21.6,
        bite: None,
        script: creature_zigzag_script,
        wave: 4,
        hp: 1,
        archetype: Archetype::Meute,
        chase_speed: 3.0,
    },
    DemoCreature {
        name: "Créature 26",
        file: "creature26.glb",
        spawn: Vec3::new(28.0, 0.0, 26.0),
        layer_bit: 26,
        prefix: "creature26_",
        heading0: 12.0,
        phase: 22.5,
        bite: None,
        script: creature_lemniscate_script,
        wave: 4,
        hp: 3,
        archetype: Archetype::Colosse,
        chase_speed: 3.4,
    },
];

pub(super) fn add_named_creatures(
    objects: &mut Vec<SceneObject>,
    imported: &mut Vec<ImportedMesh>,
    half: f32,
) {
    for spec in MMORPG_CREATURES {
        let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), spec.file);
        match crate::scene::import::load_gltf(&path) {
            Ok((data, aabb_min, aabb_max)) => {
                let mut mesh = ImportedMesh {
                    path: path.clone(),
                    data,
                    aabb_min,
                    aabb_max,
                    ..Default::default()
                };
                mesh.load_skinning();
                let mesh_index = imported.len() as u32;
                let mut creature = demo_obj(spec.name, MeshKind::Imported(mesh_index), spec.spawn);
                creature.transform = creature.transform.with_scale(Vec3::splat(0.35));
                creature.animation = Some(AnimationState {
                    clip: "Idle".into(),
                    ..Default::default()
                });
                creature.physics = PhysicsKind::Kinematic;
                creature.collision_layer = 1 << spec.layer_bit;
                // Tuable (boule de feu/mêlée) et synchronisée en réseau — même
                // pipeline générique que les autres monstres `Combat::attackable`
                // (`fireball_impact`/`attack_at`, `AppState::network_snapshot`) :
                // rien de spécifique aux créatures scriptées à ajouter côté mise à
                // mort, cf. GAMEDESIGN_EN_LIGNE.md et ROADMAP (synchro réseau).
                // `wave`/`hp` : la dent de scie de « La Horde » (GDD §5.5),
                // cf. les docs de `DemoCreature::wave`/`hp`.
                creature.combat = Some(Combat {
                    attackable: true,
                    wave: spec.wave,
                    hp: spec.hp,
                    ..Default::default()
                });
                // Grammaire de chasse (GDD §5.4, chantier 4.1) : le casting
                // par archétype, appliqué comme COMPORTEMENT seulement — les
                // PV ci-dessus restent ceux de la table (`hp_multiplier`
                // volontairement non appliqué, contrat de vagues verrouillé).
                // Le corps reste Kinematic scripté : patrouille Lua par
                // défaut, chasse à portée (cf. `app::simulation`, boucle
                // chasseurs, et `Physics::build` — le scripté prime).
                creature.ai_chaser = Some(crate::scene::AiChaser {
                    speed: spec.chase_speed,
                    archetype: spec.archetype,
                });
                let wander = (spec.script)(
                    half,
                    spec.prefix,
                    !(1_u32 << spec.layer_bit),
                    spec.heading0,
                    spec.phase,
                );
                creature.script = match spec.bite {
                    Some((cooldown, chance, damage, salt)) => {
                        // `trigger = true` active la détection de contact
                        // (`obj.triggered`) nécessaire à l'attaque, sans
                        // changer son collider (toujours solide).
                        creature.trigger = true;
                        // Persisté aussi nativement (`SceneObject::bite`), en plus
                        // du script Lua ci-dessous : le script pilote la version
                        // solo, ce champ permet à `app::health` de retrouver
                        // « quelles créatures mordent » côté réseau sans
                        // redécouvrir chaque nom en dur (cf. sa doc).
                        creature.bite = Some(crate::scene::BiteAttack {
                            cooldown,
                            chance,
                            damage,
                        });
                        format!(
                            "{wander}\n{}",
                            creature_bite_script(spec.prefix, cooldown, chance, damage, salt)
                        )
                    }
                    None => wander,
                };
                objects.push(creature);
                imported.push(mesh);
            }
            Err(e) => log::error!("{} MMORPG ({path}) : {e}", spec.name),
        }
    }
}
