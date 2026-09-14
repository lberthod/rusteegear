#!/usr/bin/env python3
"""Végétation et rochers « réalistes » de la démo Rivière & cascade — générés
sans Blender (numpy), écrits en GLB avec couleurs par sommet (`COLOR_0`).

    python3 scripts/gen_riviere_vegetation.py

Sortie `assets/models/riviere/` :
- `epicea_a/b.glb`, `epicea_lod.glb` : épicéas — tronc conique, verticilles de
  branches retombantes faites de lames d'aiguilles (≈ 2 500 triangles ;
  ≈ 700 pour le LOD lointain), unité = 10 m de haut ;
- `pin_a.glb` : pin sylvestre — fût nu, houppier irrégulier de bouquets ;
- `hetre_a/b.glb`, `hetre_lod.glb` : hêtres — tronc, charpentières, ≈ 1 800
  feuilles en losange dans une couronne ellipsoïdale, unité = 9 m ;
- `bouleau_a.glb` : bouleau — tronc blanc à marques sombres, petites feuilles ;
- `herbe_touffe.glb`, `herbe_haute.glb` : touffes de brins courbés ;
- `fougere.glb` : frondes arquées à pinnules ;
- `rocher_a/b/c.glb`, `galet_a/b.glb` : icosphères déplacées par bruit,
  fissures sombres, mousse sur les faces tournées vers le haut ;
- `tronc_mort.glb` : tronc couché, écorce et mousse.

Éclairage du feuillage : les normales ne suivent pas les triangles (chaque lame
serait noire d'un côté) mais la direction « axe du tronc → sommet », mélangée
au haut — l'ombrage arrondi des arbres réels, l'astuce de tous les moteurs.
"""

import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from gen_riviere_cascade import OUT, write_glb  # noqa: E402

rng = np.random.default_rng(20260911)


# ----------------------------------------------------------------------------
# Accumulateur de maillage
# ----------------------------------------------------------------------------
class Mesh:
    def __init__(self):
        self.pos, self.nrm, self.col, self.uv, self.idx = [], [], [], [], []
        self.n = 0

    def add(self, pos, nrm, col, idx, uv=None):
        pos = np.asarray(pos, dtype=np.float32).reshape(-1, 3)
        nrm = np.asarray(nrm, dtype=np.float32).reshape(-1, 3)
        col = np.asarray(col, dtype=np.float32).reshape(-1, 3)
        idx = np.asarray(idx, dtype=np.uint32).reshape(-1) + self.n
        if uv is None:
            uv = np.zeros((len(pos), 2), dtype=np.float32)
        self.pos.append(pos)
        self.nrm.append(nrm)
        self.col.append(col)
        self.uv.append(np.asarray(uv, dtype=np.float32).reshape(-1, 2))
        self.idx.append(idx)
        self.n += len(pos)

    def write(self, name):
        pos = np.concatenate(self.pos)
        write_glb(
            os.path.join(OUT, name + ".glb"), pos, np.concatenate(self.nrm), np.concatenate(self.uv),
            np.clip(np.concatenate(self.col), 0, 1), np.concatenate(self.idx), name, double_sided=True,
        )


def normalize(v):
    v = np.asarray(v, dtype=np.float64)
    n = np.linalg.norm(v, axis=-1, keepdims=True)
    return v / np.maximum(n, 1e-9)


def jitter(c, amount, n=None):
    """Couleur(s) ± bruit multiplicatif."""
    c = np.asarray(c, dtype=np.float64)
    if n is None:
        return c * (1.0 + rng.uniform(-amount, amount, size=c.shape[-1]))
    return c[None, :] * (1.0 + rng.uniform(-amount, amount, size=(n, 1))) * (1.0 + rng.uniform(-amount * 0.4, amount * 0.4, size=(n, 3)))


def frame(axis):
    """Deux vecteurs orthogonaux à `axis` (unitaire)."""
    a = normalize(axis)
    helper = np.array([0.0, 0.0, 1.0]) if abs(a[1]) > 0.9 else np.array([0.0, 1.0, 0.0])
    u = normalize(np.cross(a, helper))
    v = np.cross(a, u)
    return u, v


def tube(mesh, points, radii, color_fn, segments=8, cap_top=True):
    """Tube balayé le long d'une polyligne (points N×3, rayons N)."""
    points = np.asarray(points, dtype=np.float64)
    n = len(points)
    ring_pos, ring_nrm, ring_col = [], [], []
    for i in range(n):
        d = points[min(i + 1, n - 1)] - points[max(i - 1, 0)]
        u, v = frame(d)
        for s in range(segments):
            th = 2 * math.pi * s / segments
            radial = math.cos(th) * u + math.sin(th) * v
            ring_pos.append(points[i] + radial * radii[i])
            ring_nrm.append(radial)
            ring_col.append(color_fn(i / max(n - 1, 1), th, points[i]))
    idx = []
    for i in range(n - 1):
        for s in range(segments):
            a = i * segments + s
            b = i * segments + (s + 1) % segments
            c = a + segments
            d = b + segments
            idx += [a, c, b, b, c, d]
    if cap_top:
        base = len(ring_pos)
        ring_pos.append(points[-1])
        ring_nrm.append(normalize(points[-1] - points[-2]))
        ring_col.append(color_fn(1.0, 0.0, points[-1]))
        for s in range(segments):
            a = (n - 1) * segments + s
            b = (n - 1) * segments + (s + 1) % segments
            idx += [a, b, base]
    mesh.add(ring_pos, ring_nrm, ring_col, idx)


def bark_color(base, dark, t, th, p):
    n = (math.sin(p[1] * 9.0 + th * 3.0) + math.sin(th * 7.0 + p[1] * 2.0)) * 0.25 + 0.5
    c = np.asarray(base) * (0.75 + 0.5 * n) * (0.85 + 0.15 * t)
    return c * (1.0 - 0.35 * dark * (1.0 - t))


# ----------------------------------------------------------------------------
# Conifères
# ----------------------------------------------------------------------------
def needle_blade(mesh, base, direction, length, width, droop, col_in, col_out, normal_dir, n_seg=3, sprigs=4):
    """Une branche de conifère : lame centrale retombante + brindilles latérales
    (triangles), colorée du sombre (intérieur) au clair (pointe)."""
    d = normalize(direction)
    u, _ = frame(d)
    side = normalize(np.cross(d, np.array([0, 1.0, 0])))
    if np.linalg.norm(side) < 1e-6:
        side = u
    pts = []
    for i in range(n_seg + 1):
        t = i / n_seg
        p = base + d * length * t + np.array([0, -droop * t * t * length, 0])
        pts.append(p)
    pts = np.array(pts)
    # lame centrale (bande)
    pos, col, nrm, idx = [], [], [], []
    for i, p in enumerate(pts):
        t = i / n_seg
        w = width * (1.0 - 0.7 * t)
        c = col_in * (1 - t) + col_out * t
        pos += [p - side * w, p + side * w]
        col += [c * rng.uniform(0.85, 1.15), c * rng.uniform(0.85, 1.15)]
        nrm += [normal_dir, normal_dir]
    for i in range(n_seg):
        a = 2 * i
        idx += [a, a + 2, a + 1, a + 1, a + 2, a + 3]
    mesh.add(pos, nrm, col, idx)
    # brindilles latérales : triangles obliques de part et d'autre
    pos, col, nrm, idx = [], [], [], []
    for k in range(sprigs):
        t = (k + 0.7) / (sprigs + 0.5)
        p = pts[0] + (pts[-1] - pts[0]) * t + np.array([0, -droop * t * t * length * 0.6, 0])
        for sgn in (-1, 1):
            tip = p + (side * sgn * 0.7 + d * 0.6 + np.array([0, -0.35, 0])) * width * 2.6 * (1.0 - 0.5 * t)
            c = (col_in * (1 - t) + col_out * t) * rng.uniform(0.85, 1.15)
            b = len(pos)
            pos += [p - d * width * 0.5, p + d * width * 0.5, tip]
            col += [c * 0.9, c * 0.9, c * 1.1]
            nrm += [normal_dir] * 3
            idx += [b, b + 1, b + 2]
    mesh.add(pos, nrm, col, idx)


def needle_puff(mesh, p, size, col, nd, count=2):
    """Houppe d'aiguilles : quelques losanges à orientation aléatoire autour de `p`
    — remplit la branche entre les lames, donne son volume au conifère."""
    pos, nrm, cols, idx = [], [], [], []
    for _ in range(count):
        n = normalize(rng.normal(size=3))
        a, b = frame(n)
        q = p + rng.normal(size=3) * size * 0.35
        s = size * rng.uniform(0.7, 1.3)
        c = col * rng.uniform(0.8, 1.2)
        base = len(pos)
        pos += [q + a * s * 1.6, q + b * s * 0.5, q - a * s * 1.6, q - b * s * 0.5]
        nrm += [nd] * 4
        cols += [c * 0.9, c, c * 1.1, c]
        idx += [base, base + 1, base + 2, base, base + 2, base + 3]
    mesh.add(pos, nrm, cols, idx)


def conifer(name, height=10.0, kind="spruce", detail=1.0, seed=0):
    global rng
    rng = np.random.default_rng(20260911 + seed)
    m = Mesh()
    lean = rng.uniform(-0.03, 0.03, size=2)
    n_pts = 14
    pts = np.array([[lean[0] * (i / n_pts) ** 2 * height, height * i / n_pts, lean[1] * (i / n_pts) ** 2 * height] for i in range(n_pts + 1)])
    r0 = 0.045 * height
    radii = [r0 * (1.0 - 0.92 * (i / n_pts)) ** 1.1 + 0.015 for i in range(n_pts + 1)]
    bark = np.array([0.30, 0.20, 0.12]) if kind == "spruce" else np.array([0.55, 0.32, 0.18])
    tube(m, pts, radii, lambda t, th, p: bark_color(bark, 0.5, t, th, p), segments=9)

    if kind == "spruce":
        crown_start, crown_end = 0.14, 0.985
        col_in = np.array([0.045, 0.10, 0.035])
        col_out = np.array([0.15, 0.27, 0.10])
        whorl_step = 0.05 * height / max(detail, 0.35)
        per_whorl = int(round(7 * detail)) + 1
        max_len = 0.26 * height
        droop = 0.5
    else:  # pin : fût nu, couronne haute et irrégulière
        crown_start, crown_end = 0.52, 0.99
        col_in = np.array([0.06, 0.12, 0.04])
        col_out = np.array([0.20, 0.30, 0.10])
        whorl_step = 0.065 * height / max(detail, 0.35)
        per_whorl = int(round(7 * detail)) + 1
        max_len = 0.30 * height
        droop = -0.25  # branches remontantes

    y = crown_start * height
    while y < crown_end * height:
        t = (y - crown_start * height) / ((crown_end - crown_start) * height)
        # longueur décroissante vers la cime, houppier de pin plus arrondi
        length = max_len * ((1.0 - t) ** 0.85 if kind == "spruce" else math.sin(min(1, 0.25 + t) * math.pi) ** 0.7 + 0.15)
        length *= rng.uniform(0.75, 1.15)
        phase = rng.uniform(0, 2 * math.pi)
        for b in range(per_whorl):
            az = phase + 2 * math.pi * b / per_whorl + rng.uniform(-0.3, 0.3)
            radial = np.array([math.cos(az), 0.0, math.sin(az)])
            base = np.array([lean[0] * (y / height) ** 2 * height, y + rng.uniform(-0.15, 0.15), lean[1] * (y / height) ** 2 * height]) + radial * radii[int(y / height * n_pts)] * 0.8
            tilt = rng.uniform(-0.15, 0.15) + (0.1 if kind == "spruce" else 0.45)
            direction = radial + np.array([0, tilt, 0])
            nd = normalize(radial * 0.75 + np.array([0, 0.65, 0]))
            width = (0.34 + 0.16 * (1 - t)) * (height / 10.0) * (1.0 if kind == "spruce" else 1.2)
            needle_blade(m, base, direction, length, width, droop, col_in, col_out, nd,
                         n_seg=3, sprigs=max(2, int(round(3 * detail))))
            # houppes le long de la branche (volume), plus denses en détail plein
            d_unit = normalize(direction)
            for tp in (0.4, 0.85):
                q = base + d_unit * length * tp + np.array([0, -droop * tp * tp * length, 0])
                needle_puff(m, q, 0.2 * height / 10.0, col_in * (1 - tp) + col_out * tp, nd,
                            count=max(1, int(round(1.5 * detail))))
            if kind == "pine":
                end = base + d_unit * length + np.array([0, -droop * length, 0])
                leaf_cloud(m, end, (0.09 * height, 0.05 * height, 0.09 * height), int(38 * detail),
                           0.05 * height, col_in, col_out, tint_var=0.12, flatten=0.8)
        y += whorl_step * rng.uniform(0.85, 1.15)
    # flèche terminale
    if kind == "spruce":
        needle_blade(m, np.array([lean[0] * height, height * 0.96, lean[1] * height]), np.array([0, 1.0, 0]), height * 0.06, 0.06 * height / 10, 0.0, col_in, col_out, np.array([0, 1.0, 0]), n_seg=2, sprigs=2)
    m.write(name)


# ----------------------------------------------------------------------------
# Feuillus
# ----------------------------------------------------------------------------
def leaf_cloud(mesh, center, radii, n_leaves, size, col_in, col_out, tint_var=0.18, flatten=1.0):
    """Nuage de feuilles en losange dans un ellipsoïde, denses vers la coque ;
    normale = direction centre → feuille mêlée au haut (ombrage arrondi)."""
    cx, cy, cz = center
    rx, ry, rz = radii
    u = rng.uniform(0, 1, n_leaves)
    r = 0.45 + 0.55 * u ** (1.0 / 3.0)
    theta = rng.uniform(0, 2 * math.pi, n_leaves)
    phi = np.arccos(rng.uniform(-1, 1, n_leaves))
    dirs = np.stack([np.sin(phi) * np.cos(theta), np.cos(phi) * flatten, np.sin(phi) * np.sin(theta)], axis=1)
    p = np.array(center) + dirs * r[:, None] * np.array([rx, ry, rz])
    shell = np.clip((r - 0.45) / 0.55, 0, 1) ** 1.5
    pos, nrm, col, idx = [], [], [], []
    for i in range(n_leaves):
        # orientation : normale aléatoire penchée vers le bas (feuilles pendantes)
        n = normalize(rng.normal(size=3) + np.array([0, -0.6, 0]))
        a, b = frame(n)
        rot = rng.uniform(0, 2 * math.pi)
        a2 = a * math.cos(rot) + b * math.sin(rot)
        b2 = -a * math.sin(rot) + b * math.cos(rot)
        s = size * rng.uniform(0.7, 1.3)
        c = (col_in * (1 - shell[i]) + col_out * shell[i]) * (1.0 + rng.uniform(-tint_var, tint_var))
        c = c * np.array([1.0 + rng.uniform(-0.12, 0.12), 1.0, 1.0 + rng.uniform(-0.08, 0.08)])
        shade = normalize(normalize(dirs[i] * np.array([1, 0.6, 1])) * 0.7 + np.array([0, 0.75, 0]))
        base = len(pos)
        pos += [p[i] + a2 * s, p[i] + b2 * s * 0.6, p[i] - a2 * s, p[i] - b2 * s * 0.6]
        nrm += [shade] * 4
        col += [c * 1.05, c, c * 0.9, c]
        idx += [base, base + 1, base + 2, base, base + 2, base + 3]
    mesh.add(pos, nrm, col, idx)


def broadleaf(name, height=9.0, kind="beech", detail=1.0, seed=0):
    global rng
    rng = np.random.default_rng(20260901 + seed)
    m = Mesh()
    if kind == "beech":
        bark = np.array([0.36, 0.33, 0.29])
        col_in, col_out = np.array([0.05, 0.13, 0.035]), np.array([0.24, 0.42, 0.12])
        leaf = 0.27 * height / 9
        crown_r = (0.42 * height, 0.30 * height, 0.42 * height)
        trunk_top = 0.55
    else:  # bouleau
        bark = np.array([0.86, 0.85, 0.80])
        col_in, col_out = np.array([0.08, 0.16, 0.05]), np.array([0.32, 0.50, 0.16])
        leaf = 0.20 * height / 9
        crown_r = (0.30 * height, 0.32 * height, 0.30 * height)
        trunk_top = 0.5

    def bark_fn(t, th, p):
        if kind == "birch":
            band = (math.sin(p[1] * 13.0 + th * 2.0) * 0.5 + 0.5) ** 6 * (math.sin(th * 5 + p[1] * 3) * 0.5 + 0.5)
            c = bark * (0.9 + 0.15 * math.sin(th * 11 + p[1] * 7))
            return c * (1.0 - 0.85 * band) * (0.8 + 0.2 * t)
        return bark_color(bark, 0.3, t, th, p)

    n_pts = 8
    lean = rng.uniform(-0.05, 0.05, size=2)
    trunk = np.array([[lean[0] * (i / n_pts) ** 2 * height, trunk_top * height * i / n_pts, lean[1] * (i / n_pts) ** 2 * height] for i in range(n_pts + 1)])
    r0 = (0.035 if kind == "beech" else 0.022) * height
    radii = [r0 * (1.0 - 0.6 * i / n_pts) for i in range(n_pts + 1)]
    tube(m, trunk, radii, bark_fn, segments=10, cap_top=False)
    top = trunk[-1]
    # charpentières
    n_branches = 4 + int(2 * detail)
    ends = []
    for b in range(n_branches):
        az = 2 * math.pi * b / n_branches + rng.uniform(-0.4, 0.4)
        up = rng.uniform(0.6, 1.3)
        d = normalize(np.array([math.cos(az), up, math.sin(az)]))
        length = height * rng.uniform(0.28, 0.42)
        pts = [top + d * length * (i / 4) + np.array([0, 0.08 * length * (i / 4) ** 2, 0]) for i in range(5)]
        rr = [radii[-1] * (1.0 - 0.8 * i / 4) + 0.01 for i in range(5)]
        tube(m, pts, rr, bark_fn, segments=7)
        ends.append(pts[-1])
        # sous-branches
        for _ in range(2):
            az2 = az + rng.uniform(-1.2, 1.2)
            d2 = normalize(np.array([math.cos(az2), rng.uniform(0.3, 1.0), math.sin(az2)]))
            start = pts[2]
            l2 = length * rng.uniform(0.45, 0.7)
            p2 = [start + d2 * l2 * (i / 3) for i in range(4)]
            tube(m, p2, [rr[2] * (1 - 0.8 * i / 3) + 0.008 for i in range(4)], bark_fn, segments=6)
            ends.append(p2[-1])
    # couronne : un grand nuage + un nuage par bout de branche
    n_main = int(560 * detail)
    center = top + np.array([0, crown_r[1] * 0.75, 0])
    leaf_cloud(m, center, crown_r, n_main, leaf, col_in, col_out, flatten=1.0)
    for e in ends:
        leaf_cloud(m, e + np.array([0, leaf * 2, 0]), (crown_r[0] * 0.4, crown_r[1] * 0.35, crown_r[2] * 0.4), int(70 * detail), leaf, col_in, col_out)
    m.write(name)


# ----------------------------------------------------------------------------
# Sous-bois
# ----------------------------------------------------------------------------
def grass(name, blades=26, height=0.5, spread=0.22, seed=0):
    global rng
    rng = np.random.default_rng(20260801 + seed)
    m = Mesh()
    base_c, tip_c = np.array([0.10, 0.20, 0.04]), np.array([0.36, 0.50, 0.16])
    for _ in range(blades):
        az = rng.uniform(0, 2 * math.pi)
        lean = rng.uniform(0.15, 0.7)
        h = height * rng.uniform(0.6, 1.2)
        w = 0.018 * rng.uniform(0.8, 1.3)
        root = np.array([rng.uniform(-spread, spread), 0.0, rng.uniform(-spread, spread)]) * 0.5
        d = np.array([math.cos(az) * lean, 1.0, math.sin(az) * lean])
        side = normalize(np.cross(d, [0, 1, 0]))
        if np.linalg.norm(side) < 1e-6:
            side = np.array([1.0, 0, 0])
        pos, nrm, col, idx = [], [], [], []
        yellow = rng.uniform(0.0, 0.35)
        n_seg = 3
        for i in range(n_seg + 1):
            t = i / n_seg
            p = root + d * h * t + np.array([0, -lean * 0.5 * t * t * h, 0])
            ww = w * (1.0 - t * 0.9)
            c = (base_c * (1 - t) + tip_c * t) * np.array([1 + yellow * 0.6, 1 + yellow * 0.2, 1 - yellow * 0.3])
            pos += [p - side * ww, p + side * ww]
            col += [c, c]
            nn = normalize(np.array([math.cos(az), 1.4, math.sin(az)]))
            nrm += [nn, nn]
        for i in range(n_seg):
            a = 2 * i
            idx += [a, a + 2, a + 1, a + 1, a + 2, a + 3]
        m.add(pos, nrm, col, idx)
    m.write(name)


def fern(name, fronds=8, length=0.75, seed=0):
    global rng
    rng = np.random.default_rng(20260802 + seed)
    m = Mesh()
    c_in, c_out = np.array([0.06, 0.18, 0.05]), np.array([0.18, 0.36, 0.10])
    for f in range(fronds):
        az = 2 * math.pi * f / fronds + rng.uniform(-0.3, 0.3)
        L = length * rng.uniform(0.7, 1.15)
        rise = rng.uniform(0.5, 0.9)
        d = np.array([math.cos(az), rise, math.sin(az)])
        side = normalize(np.cross(d, [0, 1, 0]))
        n_seg = 10
        spine = [np.array([0, 0.03, 0]) + d * L * (i / n_seg) + np.array([0, -0.9 * (i / n_seg) ** 2 * L * rise, 0]) for i in range(n_seg + 1)]
        nn = normalize(np.array([math.cos(az) * 0.5, 1.0, math.sin(az) * 0.5]))
        pos, nrm, col, idx = [], [], [], []
        # rachis
        for i, p in enumerate(spine):
            w = 0.008 * (1 - i / n_seg) + 0.002
            pos += [p - side * w, p + side * w]
            col += [c_in * 0.9, c_in * 0.9]
            nrm += [nn, nn]
        for i in range(n_seg):
            a = 2 * i
            idx += [a, a + 2, a + 1, a + 1, a + 2, a + 3]
        # pinnules
        for i in range(1, n_seg):
            t = i / n_seg
            p = spine[i]
            pl = 0.16 * L * math.sin(t * math.pi) ** 0.6 + 0.02
            for sgn in (-1, 1):
                tip = p + side * sgn * pl + d * 0.25 * pl + np.array([0, -0.3 * pl, 0])
                c = (c_in * (1 - t) + c_out * t) * rng.uniform(0.85, 1.15)
                b = len(pos)
                pos += [p - d * 0.02 * L, p + d * 0.02 * L, tip]
                col += [c * 0.85, c * 0.85, c * 1.15]
                nrm += [nn] * 3
                idx += [b, b + 1, b + 2]
        m.add(pos, nrm, col, idx)
    m.write(name)


# ----------------------------------------------------------------------------
# Rochers
# ----------------------------------------------------------------------------
def _hash3(ix, iy, iz, seed):
    h = (ix * 374761393 + iy * 668265263 + iz * 1274126177 + seed * 982451653) & 0x7FFFFFFF
    h = (h ^ (h >> 13)) * 1103515245 & 0x7FFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF) / 65535.0


def vnoise3(p, seed=0):
    p = np.asarray(p, dtype=np.float64)
    i = np.floor(p).astype(np.int64)
    f = p - i
    u = f * f * (3 - 2 * f)
    res = np.zeros(len(p))
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (u[:, 0] if dx else 1 - u[:, 0]) * (u[:, 1] if dy else 1 - u[:, 1]) * (u[:, 2] if dz else 1 - u[:, 2])
                res += w * _hash3(i[:, 0] + dx, i[:, 1] + dy, i[:, 2] + dz, seed)
    return res


def fbm3(p, octaves=4, freq=1.0, seed=0):
    total, amp, norm, f = np.zeros(len(p)), 1.0, 0.0, freq
    for o in range(octaves):
        total += amp * vnoise3(np.asarray(p) * f + 3.7 * o, seed + o)
        norm += amp
        amp *= 0.5
        f *= 2.1
    return total / norm


def icosphere(subdiv=3):
    t = (1 + 5 ** 0.5) / 2
    v = np.array([[-1, t, 0], [1, t, 0], [-1, -t, 0], [1, -t, 0], [0, -1, t], [0, 1, t], [0, -1, -t], [0, 1, -t], [t, 0, -1], [t, 0, 1], [-t, 0, -1], [-t, 0, 1]], dtype=np.float64)
    v = normalize(v)
    f = [[0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8], [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1]]
    verts = [tuple(x) for x in v]
    cache = {}

    def mid(a, b):
        key = (min(a, b), max(a, b))
        if key not in cache:
            p = normalize((np.array(verts[a]) + np.array(verts[b])) / 2)
            verts.append(tuple(p))
            cache[key] = len(verts) - 1
        return cache[key]

    for _ in range(subdiv):
        nf = []
        for a, b, c in f:
            ab, bc, ca = mid(a, b), mid(b, c), mid(c, a)
            nf += [[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]
        f = nf
    return np.array(verts), np.array(f, dtype=np.uint32)


def rock(name, size=(1.0, 0.7, 0.9), rough=0.22, moss=0.6, wet=False, seed=0):
    v, f = icosphere(3 if not wet else 2)
    p = v * np.array(size)
    n1 = fbm3(v * 1.6 + seed, octaves=4, freq=1.0, seed=seed)
    n2 = fbm3(v * 4.0 + seed * 3, octaves=3, freq=1.0, seed=seed + 7)
    disp = 1.0 + rough * (n1 - 0.5) * 2.0 + rough * 0.35 * (n2 - 0.5)
    # facettes : quelques plans d'érosion (aplatissement selon des directions aléatoires)
    r = np.random.default_rng(seed)
    for _ in range(3):
        d = normalize(r.normal(size=3))
        k = (v @ d)
        disp = disp * (1.0 - 0.12 * np.clip(k - 0.6, 0, 1) / 0.4)
    p = p * disp[:, None]
    p[:, 1] -= p[:, 1].min()  # base au sol
    # normales par moyenne des faces
    fn = np.cross(p[f[:, 1]] - p[f[:, 0]], p[f[:, 2]] - p[f[:, 0]])
    nrm = np.zeros_like(p)
    for k in range(3):
        np.add.at(nrm, f[:, k], fn)
    nrm = normalize(nrm)
    base = np.array([0.30, 0.30, 0.28]) if wet else np.array([0.42, 0.40, 0.36])
    tone = 0.7 + 0.6 * n1[:, None]
    col = base * tone
    crack = np.clip((0.42 - n2) / 0.12, 0, 1)[:, None]
    col = col * (1.0 - 0.55 * crack)
    lichen = np.clip((fbm3(v * 6 + 11, 3, 1.0, seed + 20) - 0.6) / 0.15, 0, 1)[:, None]
    col = col * (1 - 0.5 * lichen) + np.array([0.55, 0.52, 0.35]) * 0.5 * lichen
    if moss > 0:
        up = np.clip(nrm[:, 1], 0, 1) ** 1.5
        mn = fbm3(v * 3 + 5, 3, 1.0, seed + 40)
        mw = np.clip(up * moss * (0.4 + mn) * 1.6 - 0.25, 0, 1)[:, None]
        col = col * (1 - mw) + np.array([0.16, 0.28, 0.07]) * (0.8 + 0.4 * mn[:, None]) * mw
    if wet:
        col = col * 0.8
    m = Mesh()
    m.add(p, nrm, col, f.reshape(-1))
    m.write(name)


def dead_log(name, length=3.2, radius=0.28, seed=0):
    global rng
    rng = np.random.default_rng(20260803 + seed)
    m = Mesh()
    n = 10
    pts = [np.array([-length / 2 + length * i / n, radius * 0.95 + 0.03 * math.sin(i * 1.7), 0.06 * math.sin(i * 0.9)]) for i in range(n + 1)]
    radii = [radius * (1.0 - 0.25 * i / n) * (1.0 + 0.08 * math.sin(i * 2.3)) for i in range(n + 1)]
    bark = np.array([0.24, 0.17, 0.10])

    def col(t, th, p):
        c = bark_color(bark, 0.3, t, th, p)
        up = max(0.0, math.sin(th) if False else 0.0)
        # mousse sur le dessus (th ≈ angle autour de l'axe x : dessus = y>0)
        y_dir = math.sin(th)
        moss = max(0.0, y_dir) ** 2 * (0.6 + 0.4 * math.sin(p[0] * 3.1))
        return c * (1 - moss) + np.array([0.15, 0.27, 0.07]) * moss

    tube(m, pts, radii, col, segments=12)
    m.write(name)


if __name__ == "__main__":
    print("Conifères…")
    conifer("epicea_a", 10.0, "spruce", detail=1.0, seed=1)
    conifer("epicea_b", 10.0, "spruce", detail=1.0, seed=2)
    conifer("epicea_lod", 10.0, "spruce", detail=0.4, seed=3)
    conifer("pin_a", 10.0, "pine", detail=1.0, seed=4)
    print("Feuillus…")
    broadleaf("hetre_a", 9.0, "beech", detail=1.0, seed=1)
    broadleaf("hetre_b", 9.0, "beech", detail=1.0, seed=2)
    broadleaf("hetre_lod", 9.0, "beech", detail=0.35, seed=3)
    broadleaf("bouleau_a", 9.0, "birch", detail=0.9, seed=4)
    print("Sous-bois…")
    grass("herbe_touffe", blades=26, height=0.45, spread=0.3, seed=1)
    grass("herbe_haute", blades=34, height=0.8, spread=0.4, seed=2)
    fern("fougere", fronds=9, length=0.8, seed=1)
    print("Rochers…")
    rock("rocher_a", (1.0, 0.75, 0.9), rough=0.22, moss=0.7, seed=1)
    rock("rocher_b", (1.0, 0.6, 1.1), rough=0.28, moss=0.5, seed=2)
    rock("rocher_c", (0.9, 0.9, 0.8), rough=0.18, moss=0.8, seed=3)
    rock("galet_a", (1.0, 0.55, 0.8), rough=0.10, moss=0.0, wet=True, seed=4)
    rock("galet_b", (1.0, 0.5, 1.2), rough=0.12, moss=0.15, wet=True, seed=5)
    dead_log("tronc_mort", seed=1)
    print("Terminé.")
