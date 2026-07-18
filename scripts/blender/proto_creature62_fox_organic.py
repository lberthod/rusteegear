"""PROTOTYPE (Option B du rapport qualité) : renard à corps organique lissé,
squelette à poids auto-diffusés (Automatic Weights), au lieu de l'assemblage
de primitives à 1 os/pièce poids 1.0 de `gen_creature62_fox.py`.

Ne remplace PAS creature62.glb — exporte à part
(`creature62_organic_proto.glb`) pour comparaison. Sert à vérifier que le
moteur (skinning 4 os/poids par vertex, cf. src/gfx/mesh.rs) digère bien un
mesh organique blend-skinné avant d'investir dans cette pipeline pour de
vrai.

Technique :
- **Corps/tête/museau/pattes/queue** : un objet Metaball (éléments
  ellipsoïdes qui fusionnent en surface lisse continue), converti en mesh
  puis Shade Smooth — remplace le sculpt manuel pour rester scriptable.
- **Accessoires nets** (oreilles, yeux, truffe, bas des pattes, pointe de
  queue, plastron) : primitives rigides classiques de `creature_kit.py`,
  jointes après coup — même logique que le corps lisse + accessoires durs
  du diablotin de référence.
- **Poids** : Automatic Weights (heat diffusion) sur le corps organique
  SEUL avant de joindre les accessoires rigides (poids 1.0 nommés par os) —
  évite les îlots de géométrie isolés (yeux, truffe) mal pondérés par la
  diffusion de chaleur.

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/proto_creature62_fox_organic.py
"""

import math
import os
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from creature_kit import (  # noqa: E402
    LEGS4, OUT_DIR, PARTS, cone, cylinder, fresh_scene, material, sphere,
)


def meta_elem(mb_data, co, radius, size=(1.0, 1.0, 1.0), stiffness=2.0):
    e = mb_data.elements.new()
    e.type = "ELLIPSOID"
    e.co = Vector(co)
    e.radius = radius
    e.size_x, e.size_y, e.size_z = size
    e.stiffness = stiffness
    return e


def build_organic_core():
    mb_data = bpy.data.metaballs.new("FoxCoreMeta")
    mb_data.resolution = 0.032
    mb_data.render_resolution = 0.032
    mb_data.threshold = 0.22
    mb_obj = bpy.data.objects.new("FoxCoreMeta", mb_data)
    bpy.context.collection.objects.link(mb_obj)

    # Éléments volontairement rapprochés/surdimensionnés par rapport à la
    # distance entre centres : le seuil de fusion des métaballes exige un
    # net chevauchement des rayons, sinon on obtient des blobs disjoints.
    #
    # Anatomie plus fidèle à un vrai renard (passe 2, sur retour
    # utilisateur « rendre plus naturel ») : torse allongé et plus bas au
    # lieu d'une boule, bosses d'épaule/hanche pour marquer la musculature
    # au lieu d'un tube uniforme, cou qui relie franchement au lieu d'un
    # raccord tête-sur-boule, museau allongé en deux segments, pattes plus
    # fines et plus longues (allure cursoriale) au lieu de troncs épais.
    #
    # Torse : long, bas, plat dessus-dessous plutôt que sphérique.
    # Chaîne colonne vertébrale (torse/épaule/cou/tête/museau) en
    # `stiffness` réduite : vu de dessus/derrière, une fusion trop raide
    # laisse chaque élément visible comme une bosse séparée le long du dos
    # (arête constatée au rendu) — même remède que la queue.
    SPINE_K = 1.3
    meta_elem(mb_data, (0, 0.00, 0.44), 0.30, (0.88, 1.75, 0.78), stiffness=SPINE_K)
    # Épaule (avant) et hanche (arrière) : bosses qui épaississent le torse
    # aux ceintures et servent de pont de fusion vers les pattes.
    meta_elem(mb_data, (0, -0.42, 0.50), 0.24, (0.92, 0.95, 0.85), stiffness=SPINE_K)
    meta_elem(mb_data, (0, 0.38, 0.48), 0.23, (0.92, 0.95, 0.85), stiffness=SPINE_K)
    # Cou : bride le torse à la tête au lieu de laisser deux boules se
    # toucher à peine.
    meta_elem(mb_data, (0, -0.60, 0.58), 0.19, (0.90, 1.35, 0.85), stiffness=SPINE_K)
    # Tête, plus petite que le torse (proportion animale, pas « bébé »).
    meta_elem(mb_data, (0, -0.80, 0.64), 0.25, (1.00, 1.00, 0.92), stiffness=SPINE_K)
    # Museau : deux segments qui s'amincissent vers la truffe.
    meta_elem(mb_data, (0, -1.05, 0.58), 0.15, (0.85, 1.30, 0.75), stiffness=SPINE_K)
    meta_elem(mb_data, (0, -1.22, 0.54), 0.09, (0.80, 1.10, 0.70), stiffness=SPINE_K)
    # Pattes : hanche/épaule fine (silhouette élancée) → genou → cheville,
    # nettement plus fines que la passe précédente. Écartées latéralement
    # (x plus grand) pour ne chevaucher franchement QUE la bosse
    # épaule/hanche et non le torse directement — un triple chevauchement
    # torse+bosse+hanche-de-patte trop symétrique crée une fine membrane
    # parasite (artefact « Mesh is not valid » constaté au rendu).
    for x, y in ((-0.20, -0.38), (0.20, -0.38), (-0.20, 0.38), (0.20, 0.38)):
        meta_elem(mb_data, (x, y, 0.40), 0.18, (0.95, 0.95, 1.35), stiffness=SPINE_K)
        meta_elem(mb_data, (x, y, 0.25), 0.12, (0.90, 0.90, 1.35), stiffness=SPINE_K)
        meta_elem(mb_data, (x, y, 0.12), 0.085, (0.90, 0.90, 1.30), stiffness=SPINE_K)
    # Queue : éléments ÉTIRÉS le long de l'axe de la queue (size_y > 1) au
    # lieu de sphères quasi isotropes — une sphère ne chevauche sa voisine
    # que par ses bords, ce qui laisse chaque bille visible comme une bosse
    # séparée (le « collier de perles » constaté au rendu, y compris après
    # avoir baissé stiffness/threshold) ; un élément allongé couvre lui-même
    # une grande partie de l'intervalle et fond dans le suivant.
    meta_elem(mb_data, (0, 0.42, 0.56), 0.22, (0.90, 1.55, 0.90), stiffness=1.2)
    meta_elem(mb_data, (0, 0.68, 0.64), 0.17, (0.88, 1.55, 0.88), stiffness=1.2)
    meta_elem(mb_data, (0, 0.90, 0.70), 0.13, (0.85, 1.50, 0.85), stiffness=1.2)
    meta_elem(mb_data, (0, 1.08, 0.75), 0.09, (0.80, 1.40, 0.80), stiffness=1.2)

    bpy.ops.object.select_all(action="DESELECT")
    mb_obj.select_set(True)
    bpy.context.view_layer.objects.active = mb_obj
    bpy.context.view_layer.update()
    bpy.ops.object.convert(target="MESH")
    core = bpy.context.active_object
    core.name = "Creature62OrganicCore"
    bpy.ops.object.shade_smooth()

    # Garde-fou : aucun vertex sous z=0,02 (gel par TriMesh incrusté).
    core.data.update()
    min_z = min(v.co.z for v in core.data.vertices)
    if min_z < 0.02:
        core.location.z += 0.02 - min_z
        bpy.ops.object.transform_apply(location=True, rotation=False, scale=False)

    return core


def renard_organique():
    fresh_scene()
    fur = material("Creature62OrgFur", (0.82, 0.38, 0.14))
    cream = material("Creature62OrgCream", (0.95, 0.90, 0.80))
    white = material("Creature62OrgWhite", (0.97, 0.97, 0.94))
    dark = material("Creature62OrgDark", (0.08, 0.07, 0.09))

    core = build_organic_core()
    core.data.materials.append(fur)

    # Squelette : même disposition que la version primitives, pour
    # réutiliser les mêmes clips Idle/Walk.
    bpy.ops.object.armature_add(location=(0, 0, 0))
    arm = bpy.context.active_object
    arm.name = "Creature62OrganicRig"
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    root = eb[0]
    root.name = "Root"
    root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.35))
    bones_def = {
        "Body": ("Root", (0, 0.40, 0.46), (0, -0.55, 0.52)),
        "Head": ("Body", (0, -0.55, 0.56), (0, -1.10, 0.60)),
        "Tail": ("Body", (0, 0.42, 0.52), (0, 1.10, 0.75)),
        "LegFL": ("Body", (-0.20, -0.38, 0.40), (-0.20, -0.38, 0.02)),
        "LegFR": ("Body", (0.20, -0.38, 0.40), (0.20, -0.38, 0.02)),
        "LegBL": ("Body", (-0.20, 0.38, 0.40), (-0.20, 0.38, 0.02)),
        "LegBR": ("Body", (0.20, 0.38, 0.40), (0.20, 0.38, 0.02)),
    }
    for bname, (parent, head, tail) in bones_def.items():
        b = eb.new(bname)
        b.head, b.tail = Vector(head), Vector(tail)
        b.parent = eb[parent]
    bpy.ops.object.mode_set(mode="OBJECT")

    # Poids auto-diffusés sur le corps lisse SEUL (avant les accessoires).
    bpy.ops.object.select_all(action="DESELECT")
    core.select_set(True)
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")

    # Accessoires nets (poids rigide 1.0, comme la version primitives),
    # repositionnés sur la tête/pattes plus fines de la passe 2.
    PARTS.clear()
    for sx in (-1, 1):
        # Oreille en 2 pièces, comme un vrai renard roux : dos noir +
        # fourrure claire à l'intérieur du pavillon, au lieu d'un cône
        # uni roux avec juste une pointe sombre.
        ear_rot = (math.radians(-10), 0, math.radians(sx * 5))
        cone("Head", dark, (sx * 0.17, -0.72, 0.86), (0.085, 0.085, 0.20),
             rotation=ear_rot)
        # Fine lame crème plaquée sur la face avant du cône noir (un cône
        # est à symétrie radiale : un cône intérieur simplement plus petit
        # reste englouti sans jamais dépasser — il faut une tranche fine et
        # décalée pour qu'elle perce la face avant).
        cone("Head", cream, (sx * 0.17, -0.775, 0.83), (0.058, 0.022, 0.155),
             rotation=ear_rot)
        # Œil en 3 pièces (sclère blanche + pupille sombre décalée vers
        # l'avant-extérieur + micro-reflet) au lieu d'un simple point sombre
        # — lecture beaucoup plus nette et « vivante » du regard.
        # Positionnés sur la partie la plus pleine de la tête (proche du
        # centre, pas sur l'effilement vers le museau) pour rester bien
        # nichés dans le volume au lieu de déborder du contour vu de 3/4.
        sphere("Head", white, (sx * 0.13, -0.87, 0.76), (0.042, 0.036, 0.042))
        sphere("Head", dark, (sx * 0.148, -0.905, 0.76), (0.022, 0.015, 0.022))
        sphere("Head", white, (sx * 0.155, -0.912, 0.772), (0.007, 0.005, 0.007))
    sphere("Head", dark, (0, -1.30, 0.53), (0.045, 0.045, 0.045))
    sphere("Body", cream, (0, -0.30, 0.38), (0.20, 0.28, 0.16))
    sphere("Head", cream, (0, -1.05, 0.53), (0.13, 0.20, 0.11))
    sphere("Tail", white, (0, 1.16, 0.77), (0.11, 0.13, 0.11))
    for bone, x, y in (("LegFL", -0.20, -0.38), ("LegFR", 0.20, -0.38),
                       ("LegBL", -0.20, 0.38), ("LegBR", 0.20, 0.38)):
        cylinder(bone, dark, (x, y, 0.10), (0.10, 0.10, 0.13))
        sphere(bone, dark, (x, y, 0.06), (0.10, 0.11, 0.035))

    # Shade Smooth sur les accessoires aussi : le diablotin de référence a
    # des cornes/ailes lisses, pas facettées — cohérence avec le corps
    # organique une fois joints.
    for ob in PARTS:
        for poly in ob.data.polygons:
            poly.use_smooth = True

    bpy.ops.object.select_all(action="DESELECT")
    for ob in PARTS:
        ob.select_set(True)
    core.select_set(True)
    bpy.context.view_layer.objects.active = core
    bpy.ops.object.join()
    creature = bpy.context.active_object
    creature.name = "Creature62Organic"

    # Anim : mêmes clips que gen_creature62_fox.py (mêmes noms d'os).
    bpy.ops.object.select_all(action="DESELECT")
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
    for pb in arm.pose.bones:
        pb.rotation_mode = "XYZ"

    def key_rot(bone, frame, xyz):
        pb = arm.pose.bones[bone]
        pb.rotation_euler = xyz
        pb.keyframe_insert("rotation_euler", frame=frame)

    def key_loc(bone, frame, xyz):
        pb = arm.pose.bones[bone]
        pb.location = xyz
        pb.keyframe_insert("location", frame=frame)

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, roll in ((1, 0.06), (20, -0.06), (40, 0.06)):
            key_rot("Head", f, (0, roll, 0))
        for f, sway in ((1, 0.30), (20, -0.30), (40, 0.30)):
            key_rot("Tail", f, (0, 0, sway))

    def walk(key_rot, key_loc):
        swing = math.radians(25)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sway in ((1, 0.35), (13, -0.35), (24, 0.35)):
            key_rot("Tail", f, (0, 0, sway))

    def run(key_rot, key_loc):
        # Version accélérée de Walk : cycle plus court (16 f au lieu de 24),
        # foulée plus ample, corps qui plonge plus bas puis rebondit plus
        # haut, queue tendue à l'horizontale au lieu de se balancer.
        swing = math.radians(42)
        for f, s in ((1, swing), (9, -swing), (16, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, dz in ((1, -0.02), (5, 0.09), (9, -0.02), (13, 0.09), (16, -0.02)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.08), (9, -0.08), (16, 0.08)):
            key_rot("Head", f, (nod, 0, 0))
        for f in (1, 16):
            key_rot("Tail", f, (0.35, 0, 0))

    def jump(key_rot, key_loc):
        # Saut sur place : anticipation accroupie → poussée → envol jambes
        # repliées → réception amortie → retour debout. Mêmes 7 temps que
        # `gen_fairy_hero.py::jump_keys()`, adaptés au quadrupède (les 4
        # pattes bougent ensemble, pas en trot diagonal).
        f0, f_crouch, f_launch, f_apex, f_fall, f_land, f_end = 1, 6, 12, 20, 28, 34, 40
        for leg in LEGS4:
            for f, a in ((f0, 0), (f_crouch, -15), (f_launch, 32),
                         (f_apex, -22), (f_fall, 12), (f_land, 26), (f_end, 0)):
                key_rot(leg, f, (math.radians(a), 0, 0))
        for f, dz in ((f0, 0.0), (f_crouch, -0.05), (f_launch, 0.10),
                      (f_apex, 0.17), (f_fall, 0.05), (f_land, -0.06), (f_end, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((f0, 0.0), (f_crouch, 0.08), (f_launch, -0.12),
                       (f_apex, -0.08), (f_fall, 0.02), (f_land, 0.10), (f_end, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sway in ((f0, 0), (f_crouch, -10), (f_launch, 20),
                        (f_apex, 32), (f_fall, 15), (f_land, -5), (f_end, 0)):
            key_rot("Tail", f, (math.radians(sway), 0, 0))

    def sit(key_rot, key_loc):
        # Pose statique (2 clés identiques, comme un « hold ») : pattes
        # arrière ramenées, pattes avant tendues, queue sur le côté, tête
        # droite. Rotation plafonnée à ~38° : le rig n'a qu'un seul os par
        # patte (pas de genou), et les poids sont diffusés en douceur sur
        # le corps organique (pas rigides à 1.0 comme la version
        # primitives) — au-delà de ~45° la hanche se plie et entraîne toute
        # la croupe dans une torsion visible (constaté à 78°, corrigé ici).
        for f in (1, 20):
            key_rot("LegFL", f, (math.radians(-4), 0, 0))
            key_rot("LegFR", f, (math.radians(-4), 0, 0))
            key_rot("LegBL", f, (math.radians(38), 0, 0))
            key_rot("LegBR", f, (math.radians(38), 0, 0))
            key_loc("Body", f, (0, 0.05, -0.045))
            key_rot("Head", f, (-0.06, 0, 0))
            key_rot("Tail", f, (0, 0, math.radians(55)))

    def attack(key_rot, key_loc):
        # Charge/morsure : recul-anticipation, jaillissement en avant (corps
        # translaté le long de l'axe local du bassin, qui pointe vers
        # l'avant -Y comme les autres bonds), tête plongeante façon
        # attaque-museau (pas d'os de mâchoire sur ce rig), puis retour.
        # `key_loc` place toujours son décalage dans la même composante que
        # les autres clips (Idle/Walk/Jump/Run/Sit) — axe local du bassin,
        # qui pointe vers l'avant -Y comme les autres bonds.
        f0, f_crouch, f_lunge, f_bite, f_recover, f_end = 1, 6, 13, 17, 23, 28
        for f, dy in ((f0, 0.0), (f_crouch, -0.06), (f_lunge, 0.20),
                      (f_bite, 0.22), (f_recover, 0.04), (f_end, 0.0)):
            key_loc("Body", f, (0, dy, 0))
        for leg in ("LegFL", "LegFR"):
            for f, a in ((f0, 0), (f_crouch, -14), (f_lunge, 30),
                         (f_bite, 32), (f_recover, 5), (f_end, 0)):
                key_rot(leg, f, (math.radians(a), 0, 0))
        for leg in ("LegBL", "LegBR"):
            for f, a in ((f0, 0), (f_crouch, 18), (f_lunge, -20),
                         (f_bite, -22), (f_recover, -4), (f_end, 0)):
                key_rot(leg, f, (math.radians(a), 0, 0))
        for f, nod in ((f0, 0.0), (f_crouch, 0.10), (f_lunge, -0.30),
                       (f_bite, -0.38), (f_recover, -0.05), (f_end, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sway in ((f0, 0), (f_crouch, -15), (f_lunge, 25),
                        (f_bite, 30), (f_recover, 5), (f_end, 0)):
            key_rot("Tail", f, (math.radians(sway), 0, 0))

    def flip(key_rot, key_loc):
        # Saut périlleux — v4 : la tête est à ~1,74 unité du pivot de
        # rotation (l'articulation de `Body`) — à 90°/270° de bascule, une
        # tête non repliée pointe donc tout droit vers le bas et perce le
        # sol de plus d'1 unité, mesuré avec un script de vérification
        # (min Z de tous les vertices sur chaque frame du clip, cf. note de
        # session) : v3 réduisait juste l'amplitude visuelle sans jamais
        # vérifier le sol, insuffisant. Ici on combine :
        # 1) un vrai saut (arc vertical nettement plus haut) ;
        # 2) tête ET queue repliées vers le corps pendant la bascule
        #    (translation locale vers l'arrière, comme un animal qui
        #    rentre la tête pour une culbute) — réduit le rayon balayé
        #    plutôt que de compter uniquement sur la hauteur.
        f0, f_crouch, f_launch, f_apex, f_fall, f_land, f_end = 1, 6, 12, 18, 24, 28, 34
        for f, deg in ((f0, 0), (f_crouch, -10), (f_launch, 80),
                       (f_apex, 190), (f_fall, 290), (f_land, 350), (f_end, 360)):
            key_rot("Body", f, (math.radians(deg), 0, 0))
        for f, dz in ((f0, 0.0), (f_crouch, -0.04), (f_launch, 0.55),
                      (f_apex, 0.78), (f_fall, 0.55), (f_land, -0.04), (f_end, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, tuck in ((f0, 0.0), (f_crouch, -0.20), (f_launch, -0.62),
                        (f_apex, -0.75), (f_fall, -0.62), (f_land, -0.20), (f_end, 0.0)):
            key_loc("Head", f, (0, tuck, 0))
        for f, tuck in ((f0, 0.0), (f_crouch, -0.14), (f_launch, -0.45),
                        (f_apex, -0.55), (f_fall, -0.45), (f_land, -0.14), (f_end, 0.0)):
            key_loc("Tail", f, (0, tuck, 0))
        for leg in LEGS4:
            for f, a in ((f0, 0), (f_crouch, -12), (f_launch, 22),
                         (f_apex, -24), (f_fall, -16), (f_land, 20), (f_end, 0)):
                key_rot(leg, f, (math.radians(a), 0, 0))
        for f, sway in ((f0, 0), (f_crouch, -8), (f_launch, 18),
                        (f_apex, 22), (f_fall, 18), (f_land, -8), (f_end, 0)):
            key_rot("Tail", f, (math.radians(sway), 0, 0))

    def turn(key_rot, key_loc):
        # Demi-tour normal (pas un saut) : le bassin pivote en lacet (axe Z)
        # de 0 à 180°, avec un léger roulis dans le virage et un cycle de
        # pas ralenti pour donner l'impression de pivoter sur place plutôt
        # que de glisser.
        # Lacet (tourne) + roulis (penche dans le virage) posés en un seul
        # appel par frame : deux `key_rot("Body", f, ...)` séparés à la même
        # frame s'écraseraient l'un l'autre (chaque appel fixe les 3 axes).
        f0, f1, f2, f3, f_end = 1, 8, 16, 24, 32
        for f, yaw, lean in ((f0, 0, 0), (f1, 55, -8), (f2, 115, 8),
                              (f3, 165, -4), (f_end, 180, 0)):
            key_rot("Body", f, (math.radians(lean), 0, math.radians(yaw)))
        for f, dz in ((f0, 0.0), (f1, 0.03), (f2, -0.02), (f3, 0.03), (f_end, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        swing = math.radians(18)
        for f, s in ((f0, 0), (f1, swing), (f2, -swing), (f3, swing), (f_end, 0)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, nod in ((f0, 0.0), (f1, 0.05), (f2, -0.05), (f3, 0.05), (f_end, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sway in ((f0, 0), (f1, 20), (f2, -20), (f3, 20), (f_end, 0)):
            key_rot("Tail", f, (0, 0, math.radians(sway)))

    def walk_circle(key_rot, key_loc):
        # Marche en rond, en boucle : contrairement au Flip (tangage — la
        # tête, loin du pivot, balaie un grand arc vertical et sort du
        # cadre si l'amplitude est trop grande), ici c'est un lacet (axe Z)
        # pendant que la créature reste plantée sur ses pattes — pas de
        # grand débattement vertical, donc le même pivot `Body` reste
        # lisible sans réduction d'amplitude. 3 foulées complètes (24 f
        # chacune, comme Walk) pendant que le lacet parcourt 360° au total
        # (60°/foulée) : boucle parfaite, la pose de la frame finale
        # (patte, lacet=360°) est identique à la frame de départ (lacet=0°).
        cycles, per_cycle = 3, 24
        swing = math.radians(25)
        lean = math.radians(6)  # roulis constant, penché vers l'intérieur du virage
        for i in range(cycles * 2 + 1):  # 0..6 -> frames 1,13,25,...,73
            f = 1 + i * (per_cycle // 2)
            s = swing if i % 2 == 0 else -swing
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
            key_rot("Body", f, (lean, 0, math.radians(i * 60)))
            nod = 0.05 if i % 2 == 0 else -0.05
            key_rot("Head", f, (nod, 0, 0))
            sway = 0.35 if i % 2 == 0 else -0.35
            key_rot("Tail", f, (0, 0, sway))
        for i in range(cycles * 4 + 1):  # frames 1,7,13,...,73 (bob 2x/foulée)
            f = 1 + i * (per_cycle // 4)
            key_loc("Body", f, (0, 0.05 if i % 2 else 0.0, 0))

    def bake_clip(clip, keyer):
        ad = arm.animation_data_create()
        ad.action = None
        keyer(key_rot, key_loc)
        act = ad.action
        act.name = clip
        track = ad.nla_tracks.new()
        track.name = clip
        track.strips.new(clip, 1, act)
        ad.action = None

    bake_clip("Idle", idle)
    bake_clip("Walk", walk)
    bake_clip("Run", run)
    bake_clip("Jump", jump)
    bake_clip("Sit", sit)
    bake_clip("Attack", attack)
    bake_clip("Flip", flip)
    bake_clip("Turn", turn)
    bake_clip("WalkCircle", walk_circle)
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    bpy.ops.object.mode_set(mode="OBJECT")

    out = os.path.join(OUT_DIR, "creature62_organic_proto.glb")
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=out,
        export_format="GLB",
        export_skins=True,
        export_animations=True,
        export_animation_mode="NLA_TRACKS",
        export_force_sampling=True,
        export_yup=True,
    )
    print("EXPORTED", out)
    print("VERTS", len(creature.data.vertices))

    # Vignette : pose neutre + deux soleils, comme creature_kit.build_creature.
    ad = arm.animation_data
    ad.action = None
    for t in list(ad.nla_tracks):
        ad.nla_tracks.remove(t)
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    scene = bpy.context.scene
    scene.frame_set(1)
    bpy.context.view_layer.update()
    bpy.ops.object.camera_add(
        location=(4.6, -6.2, 3.2),
        rotation=(math.radians(74), 0, math.radians(37)),
    )
    scene.camera = bpy.context.active_object
    bpy.ops.object.light_add(
        type="SUN", location=(2, -3, 6),
        rotation=(math.radians(35), math.radians(20), 0),
    )
    bpy.context.active_object.data.energy = 3.0
    bpy.ops.object.light_add(
        type="SUN", location=(-3, 2, 4),
        rotation=(math.radians(55), math.radians(-30), 0),
    )
    bpy.context.active_object.data.energy = 1.6
    # Lumière de remplissage douce depuis la caméra : adoucit les creux
    # d'ombre entre pièces rapprochées (ex. base des oreilles) sans aplatir
    # le rendu des deux soleils.
    bpy.ops.object.light_add(
        type="SUN", location=(4.6, -6.2, 3.2),
        rotation=(math.radians(74), 0, math.radians(37)),
    )
    bpy.context.active_object.data.energy = 0.8
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = out.replace(".glb", "_preview.png")
    bpy.ops.render.render(write_still=True)
    print("RENDERED", scene.render.filepath)


renard_organique()
print("PROTO DONE")
