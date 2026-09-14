#!/usr/bin/env python3
"""Génère les assets de la démo « Rivière & cascade » (`Scene::riviere_demo`),
sans Blender : numpy + Pillow suffisent.

    python3 scripts/gen_riviere_cascade.py

Sortie (`assets/models/riviere/`) :
- `terrain_vallee.glb`        : vallée en U, plateau amont, falaise, lit de rivière
                                 et bassin creusés — grille régulière (N+1)² sommets,
                                 rangée par rangée (z croissant, puis x croissant) :
                                 `Scene::riviere_demo` s'en sert pour poser la forêt
                                 au niveau du sol (`terrain_height`).
- `terrain_vallee_albedo.png` : albédo « cuit » 2048² (herbe, mousse, roche sur
                                 les pentes, galets mouillés dans le lit, vase des
                                 berges) — une seule texture par objet dans le
                                 moteur, donc le mélange des matières est fait ici.
- `eau_haute.glb`             : nappe de la rivière amont (plateau).
- `cascade.glb`               : nappe tombante (trajectoire balistique, deux
                                 feuillets pour l'épaisseur).
- `eau_basse.glb`             : bassin de réception + rivière aval.

Les nappes d'eau portent une couleur par sommet lue par le shader d'eau
(`main.wgsl`, `Model.water`) : R = masque d'écume (berges, impact de la chute,
rapides), G = profondeur relative (0 = bord, 1 = plein chenal).

Les formules du chenal (`river_center`, `river_half_width`) sont **recopiées à
l'identique** dans `src/scene/demos/riviere.rs` (exclusion des arbres du lit) :
changer l'une sans l'autre plante des pins dans l'eau.
"""

import json
import os
import struct

import numpy as np
from PIL import Image

OUT = os.path.normpath(os.path.join(os.path.dirname(__file__), "../assets/models/riviere"))
os.makedirs(OUT, exist_ok=True)

# --- Paramètres du monde (mètres, Y vers le haut, la rivière coule vers +Z) ---
WORLD = 150.0  # côté du terrain (x, z ∈ [-75, 75])
N = 220  # cellules par côté → (N+1)² sommets
H_UP = 9.0  # altitude du plateau amont
Z_LIP = -30.0  # arête de la falaise (lèvre de la cascade)
POOL_Z = -24.5  # centre du bassin de réception
TEX = 2048

rng = np.random.default_rng(20260911)


# ----------------------------------------------------------------------------
# Bruit (value noise lissé, octaves) — vectorisé numpy.
# ----------------------------------------------------------------------------
def _hash(ix, iy, seed):
    h = (ix.astype(np.int64) * 374761393 + iy.astype(np.int64) * 668265263 + seed * 982451653) & 0x7FFFFFFF
    h = (h ^ (h >> 13)) * 1274126177 & 0x7FFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF) / 65535.0


def vnoise(x, y, seed=0):
    ix = np.floor(x)
    iy = np.floor(y)
    fx = x - ix
    fy = y - iy
    ux = fx * fx * (3.0 - 2.0 * fx)
    uy = fy * fy * (3.0 - 2.0 * fy)
    ix = ix.astype(np.int64)
    iy = iy.astype(np.int64)
    a = _hash(ix, iy, seed)
    b = _hash(ix + 1, iy, seed)
    c = _hash(ix, iy + 1, seed)
    d = _hash(ix + 1, iy + 1, seed)
    return (a * (1 - ux) + b * ux) * (1 - uy) + (c * (1 - ux) + d * ux) * uy


def fbm(x, y, octaves=5, freq=1.0, seed=0, gain=0.5, lac=2.03):
    total = np.zeros_like(x, dtype=np.float64)
    amp = 1.0
    norm = 0.0
    f = freq
    for o in range(octaves):
        total += amp * vnoise(x * f + 17.3 * o, y * f - 9.1 * o, seed + o)
        norm += amp
        amp *= gain
        f *= lac
    return total / norm


def smoothstep(lo, hi, v):
    t = np.clip((v - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


# ----------------------------------------------------------------------------
# Chenal de la rivière (recopié dans riviere.rs).
# ----------------------------------------------------------------------------
def river_center(z):
    """x du centre du chenal en fonction de z (aval sinueux, amont presque droit)."""
    z = np.asarray(z, dtype=np.float64)
    lower = 3.0 * np.sin((z - POOL_Z) * 0.075) + 1.2 * np.sin((z - POOL_Z) * 0.21 + 1.0)
    upper = 1.5 * np.sin((z - Z_LIP) * 0.12)
    # raccord continu sur 3 m autour de la lèvre (les deux valent ≈0 là)
    t = smoothstep(Z_LIP - 1.5, Z_LIP + 1.5, z)
    return upper * (1 - t) + lower * t


def river_half_width(z):
    z = np.asarray(z, dtype=np.float64)
    pool = np.exp(-(((z - POOL_Z) / 4.5) ** 2))
    lower = 3.6 + 3.4 * pool
    upper = 2.6
    t = smoothstep(Z_LIP - 1.5, Z_LIP + 1.5, z)
    return upper * (1 - t) + lower * t


def channel_depth(z):
    z = np.asarray(z, dtype=np.float64)
    pool = np.exp(-(((z - POOL_Z) / 4.5) ** 2))
    lower = 1.3 + 1.4 * pool
    upper = 1.0
    t = smoothstep(Z_LIP - 1.5, Z_LIP + 1.5, z)
    return upper * (1 - t) + lower * t


def floor_level(z):
    """Niveau de base du fond de vallée (sans relief) : plateau amont, chute, pente aval."""
    z = np.asarray(z, dtype=np.float64)
    downstream = -0.02 * np.maximum(z - (POOL_Z + 0.5), 0.0)
    return downstream


def water_level(z):
    z = np.asarray(z, dtype=np.float64)
    return np.where(z < Z_LIP, H_UP - 0.35, floor_level(z) - 0.35)


def terrain_height(x, z):
    x = np.asarray(x, dtype=np.float64)
    z = np.asarray(z, dtype=np.float64)
    ax = np.abs(x)
    # Falaise : chute de H_UP à 0 entre Z_LIP et Z_LIP + longueur ; la brèche de la
    # rivière est raide, les flancs s'adoucissent avec |x| (éboulis, pente boisée).
    t_len = 2.2 + np.minimum(0.28 * np.maximum(ax - 5.0, 0.0), 13.0)
    cliff = H_UP * (1.0 - smoothstep(Z_LIP, Z_LIP + t_len, z))
    # Flancs de vallée en U : montée douce puis franche vers les crêtes boisées.
    side = 17.0 * smoothstep(9.0, 64.0, ax) ** 1.25
    # Relief : grandes ondulations + détail fin.
    big = (fbm(x, z, octaves=4, freq=1 / 22.0, seed=3) - 0.5) * 2.4
    fine = (fbm(x, z, octaves=3, freq=1 / 4.5, seed=11) - 0.5) * 0.5
    # Rides rocheuses sur la falaise et les pentes fortes.
    face = 4.0 * (cliff / H_UP) * (1.0 - cliff / H_UP)  # 1 à mi-hauteur de la falaise, 0 sur les plats
    strata = (vnoise(x * 0.35, z * 2.2, 21) - 0.5) * 0.45 * smoothstep(0.15, 0.6, face)
    base = floor_level(z) + cliff + side + big + fine + strata
    # Chenal creusé (profil parabolique), relief étouffé dedans.
    xc = river_center(z)
    half = river_half_width(z)
    d = np.abs(x - xc) / half
    # Profil en U (d⁴) : fond plat, berges franches — la bande de galets émergée
    # entre l'eau et l'herbe reste étroite (~25 cm) au lieu d'un talus pâle.
    inside = np.clip(1.0 - d ** 4, 0.0, 1.0)
    carve = channel_depth(z) * inside
    damp = 1.0 - smoothstep(0.6, 1.3, d)  # 1 dans le lit, 0 sur la berge
    base = base - damp * (big + fine) * 0.9  # lit plus lisse que la berge
    # Le fond du bassin/lit suit le niveau de base local, pas la falaise :
    # sous la chute (z ∈ [Z_LIP, Z_LIP+2.2]) le chenal descend avec la falaise.
    h = base - carve
    # Garde-fou des berges : le relief aléatoire ne doit jamais descendre la
    # berge sous le niveau de l'eau (sinon la nappe flotte au-dessus du sol
    # comme une dalle). Entre d = 1 et d ≈ 2,3 la berge est remontée au moins
    # 0,35 m au-dessus de l'eau, avec un fondu jusqu'à d = 3,5.
    guard = water_level(z) + 0.35
    gate = smoothstep(0.85, 1.0, d) * (1.0 - smoothstep(2.3, 3.5, d))
    h = h + gate * np.maximum(guard - h, 0.0)
    return h


# ----------------------------------------------------------------------------
# Écriture GLB minimale (un mesh, une primitive, POSITION/NORMAL/TEXCOORD_0/COLOR_0).
# ----------------------------------------------------------------------------
def write_glb(path, positions, normals, uvs, colors, indices, name, double_sided=False):
    positions = np.ascontiguousarray(positions, dtype=np.float32)
    normals = np.ascontiguousarray(normals, dtype=np.float32)
    uvs = np.ascontiguousarray(uvs, dtype=np.float32)
    colors = np.ascontiguousarray(colors, dtype=np.float32)
    indices = np.ascontiguousarray(indices, dtype=np.uint32)

    blobs = [positions.tobytes(), normals.tobytes(), uvs.tobytes(), colors.tobytes(), indices.tobytes()]
    views = []
    bin_data = bytearray()
    for i, b in enumerate(blobs):
        off = len(bin_data)
        bin_data += b
        while len(bin_data) % 4:
            bin_data += b"\0"
        view = {"buffer": 0, "byteOffset": off, "byteLength": len(b)}
        view["target"] = 34963 if i == 4 else 34962
        views.append(view)

    accessors = [
        {
            "bufferView": 0,
            "componentType": 5126,
            "count": len(positions),
            "type": "VEC3",
            "min": positions.min(axis=0).tolist(),
            "max": positions.max(axis=0).tolist(),
        },
        {"bufferView": 1, "componentType": 5126, "count": len(normals), "type": "VEC3"},
        {"bufferView": 2, "componentType": 5126, "count": len(uvs), "type": "VEC2"},
        {"bufferView": 3, "componentType": 5126, "count": len(colors), "type": "VEC3"},
        {"bufferView": 4, "componentType": 5125, "count": len(indices), "type": "SCALAR"},
    ]
    gltf = {
        "asset": {"version": "2.0", "generator": "gen_riviere_cascade.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": name}],
        "meshes": [
            {
                "name": name,
                "primitives": [
                    {
                        "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2, "COLOR_0": 3},
                        "indices": 4,
                        "material": 0,
                    }
                ],
            }
        ],
        "materials": [
            {
                "name": name,
                "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.9},
                "doubleSided": double_sided,
            }
        ],
        "buffers": [{"byteLength": len(bin_data)}],
        "bufferViews": views,
        "accessors": accessors,
    }
    js = json.dumps(gltf, separators=(",", ":")).encode("utf-8")
    while len(js) % 4:
        js += b" "
    total = 12 + 8 + len(js) + 8 + len(bin_data)
    with open(path, "wb") as f:
        f.write(struct.pack("<III", 0x46546C67, 2, total))
        f.write(struct.pack("<II", len(js), 0x4E4F534A))
        f.write(js)
        f.write(struct.pack("<II", len(bin_data), 0x004E4942))
        f.write(bin_data)
    print(f"  {os.path.relpath(path)} : {len(positions)} sommets, {len(indices)//3} triangles")


def grid_indices(nx, nz):
    """Indices de deux triangles par cellule d'une grille (nz rangées de nx sommets)."""
    iz, ix = np.meshgrid(np.arange(nz - 1), np.arange(nx - 1), indexing="ij")
    a = iz * nx + ix
    b = a + 1
    c = a + nx
    d = c + 1
    tris = np.stack([a, c, b, b, c, d], axis=-1).reshape(-1)
    return tris.astype(np.uint32)


def grid_normals(positions, nx, nz):
    p = positions.reshape(nz, nx, 3)
    dx = np.zeros_like(p)
    dz = np.zeros_like(p)
    dx[:, 1:-1] = p[:, 2:] - p[:, :-2]
    dx[:, 0] = p[:, 1] - p[:, 0]
    dx[:, -1] = p[:, -1] - p[:, -2]
    dz[1:-1] = p[2:] - p[:-2]
    dz[0] = p[1] - p[0]
    dz[-1] = p[-1] - p[-2]
    n = np.cross(dz, dx)
    ln = np.linalg.norm(n, axis=-1, keepdims=True)
    n = n / np.maximum(ln, 1e-8)
    return n.reshape(-1, 3)


# ----------------------------------------------------------------------------
# 1. Terrain
# ----------------------------------------------------------------------------
def gen_terrain():
    print("Terrain…")
    xs = np.linspace(-WORLD / 2, WORLD / 2, N + 1)
    zs = np.linspace(-WORLD / 2, WORLD / 2, N + 1)
    Z, X = np.meshgrid(zs, xs, indexing="ij")
    H = terrain_height(X, Z)
    pos = np.stack([X, H, Z], axis=-1).reshape(-1, 3)
    nrm = grid_normals(pos, N + 1, N + 1)
    uv = np.stack([(X + WORLD / 2) / WORLD, (Z + WORLD / 2) / WORLD], axis=-1).reshape(-1, 2)
    col = np.ones((len(pos), 3), dtype=np.float32)
    idx = grid_indices(N + 1, N + 1)
    write_glb(os.path.join(OUT, "terrain_vallee.glb"), pos, nrm, uv, col, idx, "terrain_vallee")


# ----------------------------------------------------------------------------
# 2. Albédo cuit
# ----------------------------------------------------------------------------
def gen_albedo():
    print("Albédo…")
    R = TEX
    u = (np.arange(R) + 0.5) / R
    X, Z = np.meshgrid(u * WORLD - WORLD / 2, u * WORLD - WORLD / 2, indexing="xy")
    # X varie le long des colonnes (axe u), Z le long des lignes (axe v) : image[v][u].
    H = terrain_height(X, Z)
    eps = WORLD / R
    dhdx = (terrain_height(X + eps, Z) - terrain_height(X - eps, Z)) / (2 * eps)
    dhdz = (terrain_height(X, Z + eps) - terrain_height(X, Z - eps)) / (2 * eps)
    slope = np.sqrt(dhdx * dhdx + dhdz * dhdz)  # tan(pente)

    xc = river_center(Z)
    half = river_half_width(Z)
    d = np.abs(X - xc) / half
    wl = water_level(Z)
    under_water = smoothstep(-0.05, 0.25, wl - H)  # 1 sous l'eau

    n_big = fbm(X, Z, octaves=5, freq=1 / 3.0, seed=40)
    n_mid = fbm(X, Z, octaves=4, freq=1 / 0.8, seed=41)
    n_fine = fbm(X, Z, octaves=3, freq=1 / 0.18, seed=42)
    n_patch = fbm(X, Z, octaves=3, freq=1 / 9.0, seed=43)

    # Herbe : deux verts mêlés + brins fins.
    g1 = np.array([0.11, 0.22, 0.05])
    g2 = np.array([0.24, 0.36, 0.10])
    g3 = np.array([0.30, 0.34, 0.12])  # herbe sèche
    grass = g1[None, None] * (1 - n_big)[..., None] + g2[None, None] * n_big[..., None]
    grass = grass * (0.85 + 0.3 * n_fine)[..., None]
    dry = smoothstep(0.55, 0.75, n_patch)
    grass = grass * (1 - dry)[..., None] + g3[None, None] * (0.85 + 0.3 * n_fine)[..., None] * dry[..., None]
    # Mousse / sous-bois sombre en taches.
    moss = smoothstep(0.5, 0.7, fbm(X, Z, octaves=4, freq=1 / 5.0, seed=44))
    grass = grass * (1 - 0.45 * moss)[..., None] + np.array([0.06, 0.12, 0.03])[None, None] * (0.45 * moss)[..., None]

    # Roche : gris-brun strié, lichen.
    rock_base = np.array([0.27, 0.25, 0.22])
    strata = vnoise(X * 0.8, Z * 6.0, 45) * 0.5 + vnoise(X * 3.0, Z * 18.0, 46) * 0.5
    rock = rock_base[None, None] * (0.65 + 0.7 * strata)[..., None] * (0.85 + 0.3 * n_fine)[..., None]
    lichen = smoothstep(0.6, 0.8, n_mid)
    rock = rock * (1 - 0.5 * lichen)[..., None] + np.array([0.30, 0.34, 0.18])[None, None] * (0.5 * lichen)[..., None]

    # Galets : taches claires/sombres à haute fréquence.
    pebble = fbm(X, Z, octaves=2, freq=1 / 0.22, seed=47)
    pb = np.array([0.20, 0.18, 0.15])
    light = np.array([0.31, 0.29, 0.26])
    dark = np.array([0.11, 0.10, 0.09])
    peb = pb[None, None] * np.ones_like(X)[..., None]
    peb = np.where((pebble > 0.6)[..., None], light[None, None], peb)
    peb = np.where((pebble < 0.4)[..., None], dark[None, None], peb)
    peb = peb * (0.85 + 0.3 * n_fine)[..., None]
    # Mouillé : plus sombre et plus saturé sous l'eau, algues vertes au fond du bassin.
    peb = peb * (1 - 0.45 * under_water)[..., None]
    algae = under_water * smoothstep(0.45, 0.7, n_mid) * 0.5
    peb = peb * (1 - algae)[..., None] + np.array([0.08, 0.16, 0.08])[None, None] * algae[..., None]

    # Vase / sable des berges.
    mud = np.array([0.16, 0.13, 0.09])[None, None] * (0.8 + 0.4 * n_fine)[..., None]

    # Sol forestier (humus, aiguilles, mousse sombre) loin de la rivière ; la
    # prairie claire ne subsiste qu'en lisière du chenal.
    humus = np.array([0.10, 0.075, 0.04])[None, None] * (0.8 + 0.4 * n_fine)[..., None]
    needles = np.array([0.14, 0.11, 0.05])[None, None] * (0.8 + 0.4 * n_mid)[..., None]
    floor_col = humus * (1 - n_big)[..., None] + needles * n_big[..., None]
    floor_col = floor_col * (1 - 0.5 * moss)[..., None] + np.array([0.05, 0.11, 0.03])[None, None] * (0.5 * moss)[..., None]
    forest = smoothstep(2.4, 4.5, d) * (0.55 + 0.45 * smoothstep(0.35, 0.65, n_patch))
    col = grass * (1 - forest)[..., None] + floor_col * forest[..., None]

    # Composition par masques.
    w_rock = smoothstep(0.45, 1.0, slope + 0.25 * (n_patch - 0.5))
    col = col * (1 - w_rock)[..., None] + rock * w_rock[..., None]
    # Berge : la face abrupte entre la ligne d'eau (d ≈ 0,93) et l'herbe (d = 1)
    # est de la terre sombre et humide (racines, humus), prolongée d'un liseré
    # de vase sous l'herbe — jamais une bande de galets clairs au-dessus de l'eau.
    w_peb = smoothstep(0.92, 0.85, d)
    col = col * (1 - w_peb)[..., None] + peb * w_peb[..., None]
    w_mud = smoothstep(0.82, 0.9, d) * (1 - smoothstep(1.02, 1.18, d))
    col = col * (1 - w_mud)[..., None] + mud * w_mud[..., None]
    # Ombre du surplomb végétal sur la face de berge.
    face_ao = 1.0 - 0.45 * smoothstep(0.8, 0.93, d) * (1 - smoothstep(0.98, 1.08, d))
    col = col * face_ao[..., None]
    # Occlusion douce dans le lit et au pied de la falaise.
    ao = 1.0 - 0.35 * smoothstep(0.0, 1.0, np.clip((channel_depth(Z) * np.clip(1 - d * d, 0, 1)) / 2.5, 0, 1))
    col = col * ao[..., None]

    # Linéaire → sRGB (le moteur échantillonne en Rgba8UnormSrgb).
    srgb = np.where(col <= 0.0031308, col * 12.92, 1.055 * np.power(np.clip(col, 0, 1), 1 / 2.4) - 0.055)
    img = (np.clip(srgb, 0, 1) * 255).astype(np.uint8)
    Image.fromarray(img, "RGB").save(os.path.join(OUT, "terrain_vallee_albedo.png"), optimize=True)
    print(f"  albédo {R}×{R} écrit")


# ----------------------------------------------------------------------------
# 3. Nappes d'eau
# ----------------------------------------------------------------------------
def gen_river_strip(path, name, z0, z1, step, across, margin, foam_fn, level_fn):
    zs = np.arange(z0, z1 + 1e-6, step)
    nz = len(zs)
    nx = across + 1
    vs = np.linspace(-1.0, 1.0, nx)
    Zg, Vg = np.meshgrid(zs, vs, indexing="ij")
    xc = river_center(Zg)
    half = river_half_width(Zg) + margin
    Xg = xc + Vg * half
    Yg = level_fn(Zg) * np.ones_like(Xg)
    pos = np.stack([Xg, Yg, Zg], axis=-1).reshape(-1, 3)
    # u = abscisse curviligne (m), v = position transversale (m)
    dx = np.gradient(river_center(zs))
    ds = np.sqrt(dx * dx + step * step)
    s = np.cumsum(ds) - ds[0]
    Ug = np.repeat(s[:, None], nx, axis=1)
    uv = np.stack([Ug, Vg * half], axis=-1).reshape(-1, 2)
    rel = np.abs(Vg) * half / river_half_width(Zg)  # 0 centre → 1 berge réelle
    foam = foam_fn(Xg, Zg, rel)
    depth = np.clip(1.0 - rel * rel, 0.0, 1.0)
    col = np.stack([foam, depth, np.ones_like(foam)], axis=-1).reshape(-1, 3)
    nrm = np.tile(np.array([0.0, 1.0, 0.0], dtype=np.float32), (len(pos), 1))
    idx = grid_indices(nx, nz)
    write_glb(path, pos, nrm, uv, col, idx, name, double_sided=True)


def gen_water():
    print("Eau…")

    def foam_upper(X, Z, rel):
        banks = smoothstep(0.62, 1.0, rel) * 0.55
        rapids = smoothstep(0.62, 0.8, fbm(X, Z, octaves=3, freq=1 / 6.0, seed=60)) * 0.5
        lip = smoothstep(Z_LIP - 4.0, Z_LIP - 0.4, Z) * 0.7  # accélération avant la chute
        return np.clip(np.maximum(np.maximum(banks, rapids), lip), 0, 1)

    gen_river_strip(
        os.path.join(OUT, "eau_haute.glb"), "eau_haute",
        -WORLD / 2, Z_LIP - 0.25, 0.75, 12, 0.45, foam_upper, lambda z: np.full_like(z, H_UP - 0.35),
    )

    def foam_lower(X, Z, rel):
        banks = smoothstep(0.66, 1.0, rel) * 0.5
        impact = 1.0 - smoothstep(2.6, 7.5, np.sqrt(X * X + (Z - (Z_LIP + 3.2)) ** 2))
        rapids = smoothstep(0.64, 0.8, fbm(X, Z, octaves=3, freq=1 / 7.0, seed=61)) * 0.45
        return np.clip(np.maximum(np.maximum(banks, impact), rapids), 0, 1)

    gen_river_strip(
        os.path.join(OUT, "eau_basse.glb"), "eau_basse",
        Z_LIP + 0.6, WORLD / 2, 0.6, 20, 0.5, foam_lower, lambda z: floor_level(z) - 0.35,
    )

    # Cascade : deux feuillets (arrière plus dense, avant translucide), trajectoire
    # balistique — avance linéaire, chute quadratique.
    nt, nx = 48, 18
    ts = np.linspace(0.0, 1.0, nt)
    xs = np.linspace(-2.9, 2.9, nx)
    y_top = H_UP - 0.35
    y_bot = floor_level(Z_LIP + 3.2) - 0.55
    all_pos, all_nrm, all_uv, all_col, all_idx = [], [], [], [], []
    for layer, (dz, foam_gain) in enumerate([(-0.45, 1.0), (0.0, 0.75)]):
        T, Xg = np.meshgrid(ts, xs, indexing="ij")
        Zg = Z_LIP - 0.2 + 5.2 * T + dz
        Yg = y_top - (y_top - y_bot) * T * T
        # léger évasement vers le bas (la nappe s'élargit en tombant)
        Xg = Xg * (1.0 + 0.35 * T)
        pos = np.stack([Xg, Yg, Zg], axis=-1).reshape(-1, 3)
        nrm = grid_normals(pos, nx, nt)
        # u transversal (m), v = longueur parcourue (m, approx.)
        seg = np.sqrt(np.diff(Zg[:, 0]) ** 2 + np.diff(Yg[:, 0]) ** 2)
        s = np.concatenate([[0.0], np.cumsum(seg)])
        uv = np.stack([Xg, np.repeat(s[:, None], nx, axis=1)], axis=-1).reshape(-1, 2)
        foam = np.clip(0.15 + 0.85 * T, 0, 1) * foam_gain
        edge = 1.0 - smoothstep(0.75, 1.0, np.abs(Xg) / (2.9 * (1.0 + 0.35 * T)))
        col = np.stack([foam, edge, np.ones_like(foam)], axis=-1).reshape(-1, 3)
        base = sum(len(p) for p in all_pos)
        all_pos.append(pos)
        all_nrm.append(nrm)
        all_uv.append(uv)
        all_col.append(col)
        all_idx.append(grid_indices(nx, nt) + base)
    write_glb(
        os.path.join(OUT, "cascade.glb"), np.concatenate(all_pos), np.concatenate(all_nrm),
        np.concatenate(all_uv), np.concatenate(all_col), np.concatenate(all_idx), "cascade", double_sided=True,
    )


if __name__ == "__main__":
    gen_terrain()
    gen_water()
    gen_albedo()
    print("Terminé.")
