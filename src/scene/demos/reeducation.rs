//! Démo « Rééducation — mobilité guidée » (portage du prototype web *Mouvéo*,
//! <https://github.com/antoinequarroz/mouveo-reeducation>, 10 septembre 2026) :
//! un jeu de rééducation motrice où le patient atteint des cibles lumineuses
//! avec son poignet, son genou, sa cheville ou ses hanches, revient en position
//! neutre, et marque des points (série, régularité) — cf. `docs/REEDUCATION.md`.
//!
//! **Ce que fait Mouvéo et comment c'est rendu ici**
//!
//! | Mouvéo (React + canvas 2D)                     | RusteeGear                                              |
//! | ---------------------------------------------- | ------------------------------------------------------- |
//! | MediaPipe Pose dans le navigateur              | idem, dans `packaging/web/reeduc.html` -> export wasm    |
//! |                                                | `set_pose_landmarks` -> table Lua `pose` (`app::pose`)   |
//! | squelette dessiné sur la vidéo                 | personnage skinné (rig 43 os, doigts) piloté par `bone()`, ou bâtons |
//! | (pas de doigts dans Mouvéo)                    | deux mains sur le squelette (21 repères MediaPipe chacune), 3 exercices de doigts avec la main en grand |
//! | cibles 2D relatives à l'épaule/la hanche       | sphères émissives dans le plan `z = 0`, mêmes motifs    |
//! | machine à états React (welcome/calibrate/…)    | script Lua « directeur », état dans `save.*`            |
//! | HUD React (score, série, chrono, consigne)     | widgets HUD déclaratifs + `hud_text`                    |
//! | boutons (exercice, côté, amplitude, bilan)     | widgets `Button` -> `on_event("hud:…")`                  |
//! | « Démo tactile » (clic = cible atteinte)       | mode démo : le point suivi se pilote au joystick/flèches |
//! | historique `localStorage`, jardin de mobilité  | `save.*` (session) : séances, points cumulés, jardin    |
//!
//! Tout le gameplay est **en donnée de scène** (objets + scripts Lua + widgets) :
//! la scène s'exporte, s'édite dans l'inspecteur et tourne sur les cinq cibles ;
//! seule la caméra (MediaPipe) est propre au web. Sans caméra, `pose.ok` est faux
//! et la séance passe en mode démo — jouable partout, et c'est ce mode que les
//! tests pilotent (`app::simulation_tests`).
//!
//! Avertissement repris de Mouvéo : prototype de coaching, aucune mesure
//! clinique ni diagnostic.

use super::*;

/// Largeur (m) du cadre caméra projeté dans le monde : `x` normalisé `[0, 1]`
/// (MediaPipe) -> `[+W/2, −W/2]` (miroir : la droite du patient apparaît à
/// droite de l'écran, comme dans une glace). Même constante dans le script
/// (`W`) et dans les tests qui fabriquent des poses (`pose_to_world`).
pub const POSE_WORLD_WIDTH: f32 = 4.0;
/// Hauteur (m) du cadre caméra projeté : `y` normalisé (vers le bas) -> `[H, 0]`.
pub const POSE_WORLD_HEIGHT: f32 = 3.2;
/// Durée d'une séance (s de temps *actif* : le chrono s'arrête quand le corps
/// sort du cadre, comme dans Mouvéo).
pub const SESSION_SECONDS: f32 = 60.0;
/// Durée de calibration (s de corps visible sans interruption) avant la séance.
pub const CALIBRATION_SECONDS: f32 = 1.8;

/// Repères nommés (mêmes noms que `app::pose::NAMED`) dans l'ordre où le script
/// les parcourt ; chacun a une sphère « Repère <nom> » dans la scène.
const JOINTS: [&str; 13] = [
    "nose",
    "shoulder_l",
    "shoulder_r",
    "elbow_l",
    "elbow_r",
    "wrist_l",
    "wrist_r",
    "hip_l",
    "hip_r",
    "knee_l",
    "knee_r",
    "ankle_l",
    "ankle_r",
];

/// Projette un repère MediaPipe (`x`, `y` normalisés, `y` vers le bas) dans le
/// plan de jeu — la même formule que `P()` dans le script directeur. Exposée pour
/// les tests, qui doivent fabriquer des poses **inverses** (monde -> repère) pour
/// amener le poignet sur une cible lue dans la scène.
#[cfg(test)]
pub(crate) fn pose_to_world(x: f32, y: f32) -> (f32, f32) {
    ((0.5 - x) * POSE_WORLD_WIDTH, (1.0 - y) * POSE_WORLD_HEIGHT)
}

/// Inverse de `pose_to_world`.
#[cfg(test)]
pub(crate) fn world_to_pose(wx: f32, wy: f32) -> (f32, f32) {
    (0.5 - wx / POSE_WORLD_WIDTH, 1.0 - wy / POSE_WORLD_HEIGHT)
}

/// Script « directeur » de la séance (objet « Séance ») : machine à états de
/// Mouvéo (`welcome -> calibrate -> playing -> finished`), détection des cibles,
/// score, HUD. Lua 5.1 **et** 5.4 (cf. `docs/LUA_PORTABLE.md` : pas de `//`,
/// pas de `goto`, pas de `string.format`, pas de `pairs`) — vérifié par
/// `scripting_web::tests::reeducation_scripts_run_on_the_web_backend`.
///
/// Tout l'état vit dans `save.*` (nombres seulement, préfixe `rd_`), les autres
/// objets (cible, halo, point suivi, repères, os) le relisent le même tick — la
/// « Séance » est le premier objet de la scène pour ça.
pub const DIRECTOR_SCRIPT: &str = r#"
local SMOOTH_POSE, SMOOTH_HAND = 22.0, 30.0   -- lissage (1/s) des repères caméra
local HK = 5.0                                -- zoom de la main dessinée (m par unité image)
local VIS_MIN = 0.5                           -- sous cette visibilité, un repère n'est pas dessiné

local NAMES = {"nose", "shoulder_l", "shoulder_r", "elbow_l", "elbow_r", "wrist_l", "wrist_r",
               "hip_l", "hip_r", "knee_l", "knee_r", "ankle_l", "ankle_r"}
-- Corps virtuel du mode démo (monde, m) : silhouette debout au centre.
local VBODY = {
  nose = {0.0, 2.55}, shoulder_l = {-0.42, 2.25}, shoulder_r = {0.42, 2.25},
  elbow_l = {-0.72, 1.78}, elbow_r = {0.72, 1.78}, wrist_l = {-0.80, 1.30}, wrist_r = {0.80, 1.30},
  hip_l = {-0.24, 1.35}, hip_r = {0.24, 1.35}, knee_l = {-0.27, 0.72}, knee_r = {0.27, 0.72},
  ankle_l = {-0.29, 0.10}, ankle_r = {0.29, 0.10},
}
-- Les 8 exercices de Mouvéo + 3 exercices de doigts : motifs = décalages (x
-- latéral, y vers le BAS) en fraction de la longueur du membre, depuis
-- l'origine (épaule / hanche). Pour les mains (`mode = "hand"`), la caméra
-- mesure un degré de fermeture/ouverture `m` (0 = neutre, 1 = geste complet)
-- et l'amplitude fixe le `m` à atteindre ; le motif ne sert qu'au mode démo.
local EX = {
  {name="Bulles latérales", short="Élévation latérale", mode="arm", hold=0, k=1.6, r=0.77,g=1.00,b=0.29,
   pat={{0.72,-0.52},{0.86,-0.68},{0.68,-0.82}}, tip="Écartez le bras sur le côté, puis revenez doucement.", fam="haut du corps"},
  {name="Lumières devant", short="Élévation frontale", mode="arm", hold=0, k=1.6, r=0.49,g=0.83,b=0.99,
   pat={{0.28,-0.62},{0.35,-0.82},{0.20,-0.95}}, tip="Levez le bras devant vous sans hausser l'épaule.", fam="haut du corps"},
  {name="Chemin lumineux", short="Trajectoire contrôlée", mode="arm", hold=0, k=1.6, r=0.77,g=0.71,b=0.99,
   pat={{0.42,-0.35},{0.68,-0.62},{0.82,-0.82}}, tip="Suivez les cibles successives avec un mouvement fluide.", fam="haut du corps"},
  {name="Étoile stable", short="Maintien du bras", mode="arm", hold=1.0, k=1.6, r=0.99,g=0.83,b=0.30,
   pat={{0.72,-0.65},{0.72,-0.65},{0.72,-0.65}}, tip="Atteignez la cible et maintenez la position une seconde.", fam="équilibre"},
  {name="Fusée genou", short="Lever de genou", mode="knee", hold=0, k=1.0, r=0.98,g=0.44,b=0.52,
   pat={{0.04,0.16},{0.12,0.12},{0.02,0.08}}, tip="Montez le genou vers la cible puis reposez le pied calmement.", fam="jambes"},
  {name="Feux de pas", short="Pas latéraux", mode="ankle", hold=0, k=1.0, r=0.18,g=0.83,b=0.75,
   pat={{0.38,0.50},{0.52,0.50},{0.65,0.50}}, tip="Touchez la lumière avec le pied puis revenez au centre.", fam="jambes"},
  {name="Ascenseur", short="Flexion guidée", mode="hips", hold=0, k=0.6, r=0.98,g=0.57,b=0.24,
   pat={{0.0,0.36},{0.0,0.46},{0.0,0.40}}, tip="Descendez les hanches vers la cible puis redressez-vous doucement.", fam="jambes"},
  {name="Île équilibre", short="Équilibre sur une jambe", mode="ankle", hold=2.0, k=1.0, r=0.65,g=0.55,b=0.98,
   pat={{0.25,0.58},{0.32,0.52},{0.22,0.48}}, tip="Levez légèrement le pied et maintenez-le dans l'île lumineuse.", fam="équilibre"},
  {name="Pince lumineuse", short="Pince pouce-index", mode="hand", gesture="pinch", hold=0, k=1.2, r=1.00,g=0.90,b=0.70,
   pat={{0.0,-0.8},{0.0,-0.8},{0.0,-0.8}}, tip="Rapprochez le bout de l'index du pouce, puis rouvrez la main.", fam="main"},
  {name="Éventail", short="Ouverture des doigts", mode="hand", gesture="spread", hold=0, k=1.2, r=0.30,g=0.90,b=0.40,
   pat={{0.0,-0.8},{0.0,-0.8},{0.0,-0.8}}, tip="Ouvrez grand la main, puis refermez le poing doucement.", fam="main"},
  {name="Piano", short="Opposition pouce-doigts", mode="hand", gesture="piano", hold=0, k=1.2, r=0.30,g=0.50,b=1.00,
   pat={{0.0,-0.8},{0.0,-0.8},{0.0,-0.8}}, tip="Touchez le pouce avec chaque doigt, l'un après l'autre.", fam="main"},
}
local GARDEN = {"Graine", "Pousse", "Jeune plante", "En fleurs", "Jardin lumineux"}
local FINGER_TIPS = {9, 13, 17, 21}   -- index, majeur, annulaire, auriculaire (1-based)

local function g(k, d) local v = save.get(k); if v == nil then return d end; return v end
local function num(x) return tostring(math.floor(x + 0.5)) end
local function P(lm) return (0.5 - lm.x) * W, (1.0 - lm.y) * H end
local function dist(ax, ay, bx, by) local dx, dy = ax - bx, ay - by; return math.sqrt(dx * dx + dy * dy) end
local function clamp(v, lo, hi) if v < lo then return lo end; if v > hi then return hi end; return v end
-- Lissage exponentiel d'une valeur mémorisée dans `save` (framerate-indépendant).
local function smooth(key, target, rate)
  local cur = save.get(key)
  if cur == nil then cur = target end
  cur = cur + (target - cur) * (1.0 - math.exp(-rate * dt))
  save.set(key, cur)
  return cur
end

local stage = g("rd_stage", 0)      -- 0 accueil, 1 calibration, 2 séance, 3 bilan
local ex_i = g("rd_ex", 1)
local side = g("rd_side", 1)        -- 1 droit, -1 gauche
local amp = g("rd_amp", 75)
local goal = g("rd_goal", 8)
local cam = g("rd_cam", 0) > 0.5    -- mode caméra, verrouillé au départ de la séance
local pain, fatigue = g("rd_pain", 0), g("rd_fatigue", 2)
local sfx = (side > 0) and "_r" or "_l"

-- ---- Boutons du HUD (widgets Button -> hud:<action>) ----
if on_event("hud:ex_prev") then ex_i = ex_i - 1; if ex_i < 1 then ex_i = #EX end end
if on_event("hud:ex_next") then ex_i = ex_i + 1; if ex_i > #EX then ex_i = 1 end end
if on_event("hud:cote") then side = -side; sfx = (side > 0) and "_r" or "_l" end
if on_event("hud:amplitude") then amp = amp + 10; if amp > 95 then amp = 55 end end
if on_event("hud:objectif") then goal = goal + 2; if goal > 12 then goal = 4 end end
-- Avatar : 0 squelette de bâtons, 1 héros (proportions humaines, doigts en
-- bâtons sur ses mains), 2 ninja (rig cartoon avec doigts animés).
local avatar = g("rd_avatar", 1)
if on_event("hud:avatar") then avatar = (avatar + 1) % 3 end
if stage == 3 then
  if on_event("hud:douleur") then pain = (pain + 1) % 11 end
  if on_event("hud:fatigue") then fatigue = (fatigue + 1) % 11 end
  if on_event("hud:enregistrer") and g("rd_saved", 0) < 0.5 then
    save.set("rd_saved", 1)
    save.set("rd_sessions", g("rd_sessions", 0) + 1)
    save.set("rd_lifetime", g("rd_lifetime", 0) + g("rd_points", 0))
    if g("rd_points", 0) > g("rd_best", 0) then save.set("rd_best", g("rd_points", 0)) end
    save.set("rd_last_reg", g("rd_regularity", 0)); save.set("rd_last_pain", pain); save.set("rd_last_fatigue", fatigue)
    save.set("rd_cel", 4); save.set("rd_cel_until", time + 1.3)
  end
end
local ex = EX[ex_i]
local mode = ex.mode
local hand_ex = (mode == "hand")
if on_event("hud:demarrer") then
  if stage == 0 or stage == 3 then
    -- Nouvelle séance : compteurs à zéro, mode caméra si une pose (ou une main) est fraîche.
    save.set("rd_hits", 0); save.set("rd_points", 0); save.set("rd_combo", 0); save.set("rd_phase", 0)
    save.set("rd_tidx", 0); save.set("rd_last_hit", -10); save.set("rd_dwell", 0); save.set("rd_elapsed", 0)
    save.set("rd_saved", 0); save.set("rd_finish_at", 0); save.set("rd_calib_start", 0); save.set("rd_regularity", 0)
    pain, fatigue = 0, 2
    for i = 1, 12 do save.set("rd_hit_" .. i, 0) end
    cam = pose.ok or (hand_ex and hand.ok)
    save.set("rd_cam", cam and 1 or 0)
    if cam then
      stage = 1
    else
      for i = 1, #NAMES do local n = NAMES[i]; save.set("rd_a_" .. n .. "_x", VBODY[n][1]); save.set("rd_a_" .. n .. "_y", VBODY[n][2]) end
      save.set("rd_hand_x", VBODY["wrist" .. sfx][1]); save.set("rd_hand_y", VBODY["wrist" .. sfx][2])
      stage = 2; save.set("rd_status", 8)
    end
  else
    stage = 0 -- « Arrêter la séance »
  end
end

-- ---- Points du corps (monde) : pose caméra lissée ou corps virtuel ----
local pts = {}
local live = cam and stage >= 1 and stage <= 2
if stage == 0 then live = pose.ok end
if live then
  for i = 1, #NAMES do
    local n = NAMES[i]; local lm = pose[n]; local x, y = P(lm)
    pts[n] = {smooth("rd_s_" .. n .. "_x", x, SMOOTH_POSE), smooth("rd_s_" .. n .. "_y", y, SMOOTH_POSE), lm.v}
  end
else
  for i = 1, #NAMES do local n = NAMES[i]; pts[n] = {VBODY[n][1], VBODY[n][2], 1.0} end
end

-- ---- Mains (doigts) : 21 repères par côté, lissés ----
-- Exercice de doigts : la main travaillée est dessinée en grand, centrée ;
-- sinon les deux mains sont projetées comme le corps (même image, même
-- échelle) et viennent se poser sur les poignets du squelette.
local function hand_points(Hn, key, zoom)
  local out = {}
  if zoom then
    local cx, cy = 0, 0
    for i = 1, 21 do cx = cx + Hn.x[i]; cy = cy + Hn.y[i] end
    cx, cy = smooth("rd_hc_x", cx / 21, 12.0), smooth("rd_hc_y", cy / 21, 12.0)
    for i = 1, 21 do
      local wx, wy = (cx - Hn.x[i]) * HK, 1.6 + (cy - Hn.y[i]) * HK
      out[i] = {smooth(key .. i .. "_x", wx, SMOOTH_HAND), smooth(key .. i .. "_y", wy, SMOOTH_HAND)}
    end
  else
    for i = 1, 21 do
      local wx, wy = (0.5 - Hn.x[i]) * W, (1.0 - Hn.y[i]) * H
      out[i] = {smooth(key .. i .. "_x", wx, SMOOTH_HAND), smooth(key .. i .. "_y", wy, SMOOTH_HAND)}
    end
  end
  return out
end
local HP = nil           -- main travaillée, en grand (exercice de doigts), nil sinon
local hs = 0             -- sa taille (poignet -> base du majeur, m)
local HPL, HPR = nil, nil -- mains à l'échelle du corps, sur le squelette
local hands_live = cam or stage == 0
if hands_live and hand_ex then
  local Hn = (side > 0) and hand.right or hand.left
  if Hn then
    HP = hand_points(Hn, "rd_hz_", true)
    hs = math.max(0.2, dist(HP[1][1], HP[1][2], HP[10][1], HP[10][2]))
  end
elseif hands_live then
  if hand.left then HPL = hand_points(hand.left, "rd_hl_", false) end
  if hand.right then HPR = hand_points(hand.right, "rd_hr_", false) end
end

-- Visibilité « corps prêt » selon l'exercice.
local body_ok = true
if hand_ex then
  if cam or stage == 0 then body_ok = HP ~= nil end
  if not cam and stage >= 1 then body_ok = true end
elseif live then
  if not pose.ok then body_ok = false
  elseif mode == "arm" then
    body_ok = pts["shoulder" .. sfx][3] > 0.55 and pts["wrist" .. sfx][3] > 0.45 and pts["hip" .. sfx][3] > 0.45
  elseif mode == "hips" then
    body_ok = pts.shoulder_l[3] > 0.48 and pts.shoulder_r[3] > 0.48 and pts.hip_l[3] > 0.48 and pts.hip_r[3] > 0.48
      and pts.knee_l[3] > 0.48 and pts.knee_r[3] > 0.48
  else
    body_ok = pts["hip" .. sfx][3] > 0.55 and pts["knee" .. sfx][3] > 0.5 and pts["ankle" .. sfx][3] > 0.45
  end
end

-- ---- Calibration (mode caméra) : 1,8 s de corps (ou main) visible, puis ancres figées ----
local calib = 0
if stage == 1 then
  if body_ok then
    local start = g("rd_calib_start", 0)
    if start <= 0 then start = time; save.set("rd_calib_start", start) end
    calib = clamp((time - start) / CALIB_S, 0, 1)
    if calib >= 1 then
      for i = 1, #NAMES do local n = NAMES[i]; save.set("rd_a_" .. n .. "_x", pts[n][1]); save.set("rd_a_" .. n .. "_y", pts[n][2]) end
      stage = 2; save.set("rd_status", 3)
    end
  else
    save.set("rd_calib_start", 0)
  end
end

-- ---- Séance ----
local target_vis, tx, ty, tr, tg, tb = 0, 0, 0, ex.r, ex.g, ex.b
local hand_vis, hx, hy = 0, 0, 0
local dwell_frac = 0
if stage == 2 then
  local phase = g("rd_phase", 0)
  local tidx = g("rd_tidx", 0)
  local strength = amp / 100
  local reached, returned = false, false
  local tracked, target

  if hand_ex and cam then
    -- Geste de main mesuré sur la main dessinée : degré `m` (0 neutre, 1 complet).
    if HP then
      local m
      local thumb = HP[5]
      if ex.gesture == "spread" then
        local wrist, mcp, tip = HP[1], HP[10], HP[13]
        local dx, dy = mcp[1] - wrist[1], mcp[2] - wrist[2]
        local dl = math.max(0.001, math.sqrt(dx * dx + dy * dy))
        m = clamp((dist(wrist[1], wrist[2], tip[1], tip[2]) / hs - 1.3) / 1.0, 0, 1)
        target = {wrist[1] + dx / dl * 2.3 * hs, wrist[2] + dy / dl * 2.3 * hs}
        tracked = tip
      else
        local tip_i = 9
        if ex.gesture == "piano" then tip_i = FINGER_TIPS[(tidx % 4) + 1] end
        local tip = HP[tip_i]
        m = 1.0 - clamp((dist(thumb[1], thumb[2], tip[1], tip[2]) / hs - 0.15) / 0.9, 0, 1)
        target = thumb
        tracked = tip
      end
      reached = m >= strength
      returned = m <= 0.25
      dwell_frac = 0
    end
  else
    -- Géométrie corps (Mouvéo) : origine / neutre / membre calibrés.
    local A = {}
    for i = 1, #NAMES do local n = NAMES[i]; A[n] = {g("rd_a_" .. n .. "_x", 0), g("rd_a_" .. n .. "_y", 0)} end
    local origin, neutral, limb, tname
    if hand_ex then
      -- Mode démo des exercices de doigts : jauge verticale à droite de l'avatar.
      origin, neutral, limb, tname = {1.5, 1.1}, {1.5, 1.1}, 0.8, nil
    elseif mode == "arm" then
      origin, neutral, tname = A["shoulder" .. sfx], A["hip" .. sfx], "wrist" .. sfx
      limb = dist(origin[1], origin[2], A["elbow" .. sfx][1], A["elbow" .. sfx][2])
        + dist(A["elbow" .. sfx][1], A["elbow" .. sfx][2], A["wrist" .. sfx][1], A["wrist" .. sfx][2])
    elseif mode == "hips" then
      origin = {(A.hip_l[1] + A.hip_r[1]) / 2, (A.hip_l[2] + A.hip_r[2]) / 2}
      neutral, tname = origin, "hips"
      limb = math.max(0.5, dist(origin[1], origin[2], (A.shoulder_l[1] + A.shoulder_r[1]) / 2, (A.shoulder_l[2] + A.shoulder_r[2]) / 2))
    else
      origin, tname = A["hip" .. sfx], mode .. sfx
      neutral = A[tname]
      limb = dist(origin[1], origin[2], A["knee" .. sfx][1], A["knee" .. sfx][2])
        + dist(A["knee" .. sfx][1], A["knee" .. sfx][2], A["ankle" .. sfx][1], A["ankle" .. sfx][2])
    end
    if cam then
      if mode == "hips" then tracked = {(pts.hip_l[1] + pts.hip_r[1]) / 2, (pts.hip_l[2] + pts.hip_r[2]) / 2}
      else tracked = {pts[tname][1], pts[tname][2]} end
    else
      -- Mode démo : le point suivi part de la position neutre et se pilote au
      -- joystick (ou flèches via tilt), lissé pour un geste « doux ».
      local jx = clamp(input.jx + tilt.x, -1, 1)
      local jy = clamp(input.jy + tilt.y, -1, 1)
      local want_x, want_y = neutral[1] + jx * limb * ex.k, neutral[2] + jy * limb * ex.k
      local cx = smooth("rd_hand_x", want_x, 8.0)
      local cy = smooth("rd_hand_y", want_y, 8.0)
      tracked = {cx, cy}
      -- Le reste du membre suit, pour que l'avatar reste lisible : coude à
      -- mi-chemin épaule -> main, cheville sous le genou levé, genou à mi-chemin
      -- hanche -> cheville, tout le tronc pour les hanches.
      if mode == "hips" then
        local ox, oy = cx - origin[1], cy - origin[2]
        local names = {"hip_l", "hip_r", "shoulder_l", "shoulder_r", "elbow_l", "elbow_r", "wrist_l", "wrist_r", "nose"}
        for i = 1, #names do local n = names[i]; pts[n] = {A[n][1] + ox, A[n][2] + oy, 1} end
        pts.knee_l = {A.knee_l[1], A.knee_l[2] + oy * 0.5, 1}; pts.knee_r = {A.knee_r[1], A.knee_r[2] + oy * 0.5, 1}
      elseif tname then
        pts[tname] = {cx, cy, 1}
        if mode == "arm" then
          local s = A["shoulder" .. sfx]
          pts["elbow" .. sfx] = {(s[1] + cx) / 2 + side * 0.08, (s[2] + cy) / 2 - 0.12, 1}
        elseif mode == "knee" then
          pts["ankle" .. sfx] = {cx + side * 0.05, cy - 0.6, 1}
        else
          local h = A["hip" .. sfx]
          pts["knee" .. sfx] = {(h[1] + cx) / 2 + side * 0.06, (h[2] + cy) / 2 + 0.05, 1}
        end
      end
    end
    local off = ex.pat[(tidx % 3) + 1]
    local apply_side = (mode == "hips" or hand_ex) and 0 or side
    local reach = {origin[1] + apply_side * limb * off[1] * strength, origin[2] - limb * off[2] * strength}
    target = (phase == 0) and reach or neutral
    local thr = math.max(0.2, limb * 0.16)
    local near = dist(tracked[1], tracked[2], target[1], target[2]) < thr
    reached, returned = near, near
  end

  if phase == 1 then tr, tg, tb = 0.49, 0.83, 0.99 end
  if target then target_vis, tx, ty = 1, target[1], target[2] end
  if tracked then hand_vis, hx, hy = (body_ok and 1 or 0), tracked[1], tracked[2] end

  local hits, combo, points = g("rd_hits", 0), g("rd_combo", 0), g("rd_points", 0)
  local function record_hit()
    local last = g("rd_last_hit", -10)
    if time - last < 0.65 then return end
    save.set("rd_last_hit", time)
    hits = hits + 1; combo = combo + 1
    if hits <= 12 then save.set("rd_hit_" .. hits, time) end
    points = points + 10 + math.floor(combo / 3) * 5
    save.set("rd_phase", 1)
    if hand_ex then save.set("rd_status", 12)
    elseif mode == "arm" then save.set("rd_status", 4) elseif mode == "knee" then save.set("rd_status", 5)
    elseif mode == "ankle" then save.set("rd_status", 6) else save.set("rd_status", 7) end
    local m1, m2 = math.ceil(goal * 0.4), math.ceil(goal * 0.7)
    if combo == m1 then save.set("rd_cel", 1); save.set("rd_cel_until", time + 1.3)
    elseif combo == m2 then save.set("rd_cel", 2); save.set("rd_cel_until", time + 1.3)
    elseif combo == goal then save.set("rd_cel", 3); save.set("rd_cel_until", time + 1.3) end
    vibrate(45)
    if hits >= goal then save.set("rd_finish_at", time + 0.5) end
  end

  if body_ok and tracked then
    save.set("rd_elapsed", g("rd_elapsed", 0) + dt)
    local trigger = (phase == 0) and reached or (phase == 1 and returned)
    if trigger then
      if phase == 1 then
        save.set("rd_phase", 0); save.set("rd_tidx", tidx + 1); save.set("rd_dwell", 0); save.set("rd_status", 9)
      elseif ex.hold > 0 then
        local ds = g("rd_dwell", 0)
        if ds <= 0 then ds = time; save.set("rd_dwell", ds) end
        dwell_frac = clamp((time - ds) / ex.hold, 0, 1)
        save.set("rd_status", 10)
        if dwell_frac >= 1 then save.set("rd_dwell", 0); dwell_frac = 0; record_hit() end
      else
        record_hit()
      end
    elseif phase == 0 then
      save.set("rd_dwell", 0)
      if g("rd_status", 3) == 10 then save.set("rd_status", 3) end
    end
  end
  save.set("rd_hits", hits); save.set("rd_combo", combo); save.set("rd_points", points)

  local finish_at = g("rd_finish_at", 0)
  if (finish_at > 0 and time >= finish_at) or g("rd_elapsed", 0) >= SESSION_S then
    -- Régularité : écart-type relatif des intervalles entre cibles (Mouvéo).
    local reg = 0
    if hits >= 3 then
      local n = math.min(hits, 12) - 1
      local mean = 0
      for i = 1, n do mean = mean + (g("rd_hit_" .. (i + 1), 0) - g("rd_hit_" .. i, 0)) end
      mean = mean / n
      local var = 0
      for i = 1, n do local it = g("rd_hit_" .. (i + 1), 0) - g("rd_hit_" .. i, 0); var = var + (it - mean) * (it - mean) end
      var = var / n
      reg = clamp(math.floor(100 - (math.sqrt(var) / math.max(mean, 0.001)) * 70 + 0.5), 50, 99)
    elseif hits > 0 then reg = 75 end
    save.set("rd_regularity", reg)
    stage = 3
  end
end

-- ---- Sorties partagées avec les autres objets (cible, halo, point, repères, os, doigts) ----
save.set("rd_stage", stage); save.set("rd_ex", ex_i); save.set("rd_side", side); save.set("rd_amp", amp); save.set("rd_goal", goal)
save.set("rd_pain", pain); save.set("rd_fatigue", fatigue); save.set("rd_avatar", avatar)
save.set("rd_target_vis", target_vis); save.set("rd_target_x", tx); save.set("rd_target_y", ty)
save.set("rd_target_r", tr); save.set("rd_target_g", tg); save.set("rd_target_b", tb)
save.set("rd_hand_vis", hand_vis); save.set("rd_hand_px", hx); save.set("rd_hand_py", hy)
save.set("rd_dwell_frac", dwell_frac)
save.set("rd_target_scale", HP and math.max(0.5, hs * 1.2) or 1.0)
local show_hand = HP ~= nil and stage ~= 3
local show_body = (stage ~= 3) and (not live or pose.ok) and not show_hand
-- Le personnage skinné (objet « Avatar ») lit `rd_p_*_ok` (repère fiable) et
-- `rd_body_shown` ; les bâtons lisent `rd_p_*_vis`, à zéro en mode personnage.
save.set("rd_body_shown", show_body and 1 or 0)
for i = 1, #NAMES do
  local n = NAMES[i]
  -- Un repère mal vu (hors cadre, extrapolé par le modèle) n'est pas dessiné :
  -- sans ça, un os file vers un point fantasque hors de l'écran.
  local ok = show_body and pts[n][3] >= VIS_MIN
  save.set("rd_p_" .. n .. "_ok", ok and 1 or 0)
  save.set("rd_p_" .. n .. "_vis", (ok and avatar == 0) and 1 or 0)
  save.set("rd_p_" .. n .. "_x", pts[n][1]); save.set("rd_p_" .. n .. "_y", pts[n][2])
end
-- Deux jeux d'objets de main (gauche/droite) : en exercice de doigts, celui du
-- côté travaillé porte la main agrandie (échelle 1), l'autre est caché ; sinon
-- chacun suit sa main sur le squelette, à l'échelle du corps (0,45).
local function write_hand(prefix, pts_h, scale, sticks)
  save.set(prefix .. "scale", scale)
  save.set(prefix .. "ok", pts_h and 1 or 0)
  for i = 1, 21 do
    if pts_h then
      save.set(prefix .. i .. "_vis", sticks and 1 or 0); save.set(prefix .. i .. "_x", pts_h[i][1]); save.set(prefix .. i .. "_y", pts_h[i][2])
    else
      save.set(prefix .. i .. "_vis", 0)
    end
  end
end
if show_hand then
  write_hand((side > 0) and "rd_hr_" or "rd_hl_", HP, 1.0, true)
  write_hand((side > 0) and "rd_hl_" or "rd_hr_", nil, 1.0, true)
else
  write_hand("rd_hl_", show_body and HPL or nil, 0.45, avatar ~= 2)
  write_hand("rd_hr_", show_body and HPR or nil, 0.45, avatar ~= 2)
end

-- ---- HUD ----
local STATUS = {
  [0] = "Choisissez votre mission, puis ▶ Commencer",
  [1] = "Reculez : tête, bras et hanches doivent être visibles",
  [2] = "Ne bougez plus, calibration…",
  [3] = "Atteignez la cible sans forcer",
  [4] = "Cible atteinte — revenez près de la hanche",
  [5] = "Genou levé — reposez le pied doucement",
  [6] = "Cible atteinte — revenez au centre",
  [7] = "Descente validée — redressez-vous doucement",
  [8] = "Mode démo — joystick ou flèches : amenez le point blanc sur la cible",
  [9] = "Nouvelle cible — mouvement lent et confortable",
  [10] = "Maintenez la position…",
  [11] = "Montrez votre main entière à la caméra",
  [12] = "Bien — rouvrez la main doucement",
}
local cote = (side > 0) and "droit" or "gauche"
local cote_m = (side > 0) and "droite" or "gauche"
hud_text("titre", "MOUVÉO · " .. ex.name)
local reglages = (hand_ex and ("Main " .. cote_m) or ("Côté " .. cote)) .. " · amplitude " .. num(amp) .. " % · objectif " .. num(goal) .. " répétitions"
if stage == 0 then
  local src
  if hand_ex then
    src = HP and "📷 Main détectée : la séance suivra vos doigts." or (hand.ok and ("📷 Montrez votre main " .. cote_m .. " à la caméra.") or "🎮 Sans caméra : mode démo, la jauge se pilote au joystick ou aux flèches.")
  else
    src = pose.ok and "📷 Caméra détectée : la séance suivra vos mouvements." or "🎮 Sans caméra : mode démo, le point blanc se pilote au joystick ou aux flèches."
  end
  hud_text("aide", ex.short .. " (" .. ex.fam .. ")\n" .. ex.tip .. "\n" .. reglages .. "\n" .. src)
  hud_text("consigne", STATUS[0])
  hud_text("score", ""); hud_text("chrono", ""); hud_text("serie", ""); hud_text("objectif", ""); hud_text("bilan", "")
elseif stage == 1 then
  hud_text("aide", reglages)
  if body_ok then hud_text("consigne", STATUS[2] .. " " .. num(calib * 100) .. " %")
  else hud_text("consigne", hand_ex and STATUS[11] or STATUS[1]) end
  hud_text("score", ""); hud_text("chrono", ""); hud_text("serie", ""); hud_text("objectif", ""); hud_text("bilan", "")
elseif stage == 2 then
  local hits, combo, points = g("rd_hits", 0), g("rd_combo", 0), g("rd_points", 0)
  local left = math.max(0, SESSION_S - g("rd_elapsed", 0))
  local sec = math.floor(left)
  hud_text("score", num(points) .. " pts")
  hud_text("chrono", "Temps actif 0:" .. ((sec < 10) and "0" or "") .. num(sec))
  hud_text("serie", (combo >= 3) and ("⚡ série ×" .. num(combo)) or "")
  local bar = ""
  for i = 1, goal do bar = bar .. ((i <= hits) and "★" or "☆") end
  hud_text("objectif", "Objectif " .. num(math.min(hits, goal)) .. "/" .. num(goal) .. "  " .. bar)
  hud_text("aide", reglages)
  if not body_ok then hud_text("consigne", hand_ex and ("⏸ " .. STATUS[11] .. " — séance en pause") or "⏸ Repositionnez-vous — séance en pause")
  else
    local s = g("rd_status", 3)
    if s == 10 then hud_text("consigne", STATUS[10] .. " " .. num(dwell_frac * 100) .. " %")
    elseif hand_ex and s == 3 and ex.gesture == "piano" then
      local fingers = {"l'index", "le majeur", "l'annulaire", "l'auriculaire"}
      hud_text("consigne", "Touchez le pouce avec " .. fingers[(g("rd_tidx", 0) % 4) + 1])
    else hud_text("consigne", STATUS[s] or STATUS[3]) end
  end
  hud_text("bilan", "")
else
  local hits, points, reg = g("rd_hits", 0), g("rd_points", 0), g("rd_regularity", 0)
  local stars = 0
  if hits >= goal then stars = 3 elseif hits >= goal * 0.65 then stars = 2 elseif hits >= goal * 0.35 then stars = 1 end
  local star_s = ""
  for i = 1, 3 do star_s = star_s .. ((i <= stars) and "★" or "☆") end
  local ppm = (hits > 0) and num(points / hits) or "0"
  hud_text("bilan", "🏆 Séance terminée — " .. num(points) .. " points " .. star_s .. "\n"
    .. num(hits) .. " répétitions · régularité " .. num(reg) .. " % · " .. ppm .. " pts / mouvement\n"
    .. "Douleur ressentie " .. num(pain) .. "/10 · Fatigue " .. num(fatigue) .. "/10\n"
    .. ((g("rd_saved", 0) > 0.5) and "✔ Séance enregistrée" or "💾 Enregistrer le bilan, ou ▶ Nouvelle séance"))
  hud_text("consigne", "Bougez sans douleur : arrêtez en cas de douleur, vertige ou inconfort inhabituel.")
  hud_text("score", num(points) .. " pts"); hud_text("chrono", ""); hud_text("serie", ""); hud_text("objectif", "")
  hud_text("aide", reglages)
end
local sessions, lifetime = g("rd_sessions", 0), g("rd_lifetime", 0)
local level = math.min(4, math.floor(lifetime / 150))
hud_text("jardin", "🌱 Jardin de mobilité : " .. GARDEN[level + 1] .. " · " .. num(sessions) .. " séance" .. ((sessions == 1) and "" or "s")
  .. " · " .. num(lifetime) .. " pts" .. ((g("rd_best", 0) > 0) and (" · record " .. num(g("rd_best", 0))) or ""))
local cel = g("rd_cel", 0)
if cel > 0 and time < g("rd_cel_until", 0) then
  local CEL = {"✨ Série lancée !", "✨ Très régulier !", "🏆 Mission accomplie !", "✔ Séance enregistrée !"}
  hud_text("celebration", CEL[cel] or "")
else
  hud_text("celebration", "")
  if cel > 0 then save.set("rd_cel", 0) end
end
hud_text("mention", "Prototype de coaching, sans diagnostic ni mesure clinique — suivez les consignes de votre professionnel de santé.")
"#;

/// Script directeur complet : les constantes de ce module (projection, durées)
/// injectées en tête de `DIRECTOR_SCRIPT`, pour qu'elles n'existent qu'à un
/// seul endroit (les tests fabriquent des poses avec les mêmes).
pub fn director_script() -> String {
    format!(
        "-- Projection repère caméra -> plan de jeu (cf. reeducation.rs : POSE_WORLD_*).\n\
         local W, H = {POSE_WORLD_WIDTH:?}, {POSE_WORLD_HEIGHT:?}\n\
         local SESSION_S, CALIB_S = {SESSION_SECONDS:?}, {CALIBRATION_SECONDS:?}\n\
         {DIRECTOR_SCRIPT}"
    )
}

/// Rig d'un avatar skinné : noms des os et mesures de repos utilisés par
/// `avatar_script`. Les rigs Blender/glTF posent chaque os le long de +Y local,
/// c'est cet axe que `bone()` réoriente (cf. `import::compute_joint_matrices_into_with`).
struct AvatarRig {
    /// Valeur de `rd_avatar` qui affiche cet avatar (1 héros, 2 ninja).
    mode: u32,
    /// Rotation Y (°) pour faire face à la caméra (+Z monde).
    face_yaw: f32,
    /// Hauteur des hanches dans le rig (unités modèle, pose de repos).
    hips_y: f32,
    /// Distance hanches → épaules dans le rig (unités modèle).
    torso: f32,
    /// Chaîne du tronc (tous orientés hanches → épaules), puis le cou/tête.
    spine: &'static [&'static str],
    neck: &'static str,
    /// Bras, avant-bras, cuisse, jambe : `(côté .L du rig, côté .R)` — le
    /// personnage fait face au patient, son `.L` suit le côté **droit** capté
    /// (miroir), sauf si `mirror` est faux.
    upper_arm: (&'static str, &'static str),
    lower_arm: (&'static str, &'static str),
    upper_leg: (&'static str, &'static str),
    lower_leg: (&'static str, &'static str),
    /// Os de la main (poignet → milieu de la paume), `""` si le rig n'en a pas.
    hand: (&'static str, &'static str),
    /// Le rig a des chaînes de doigts nommées `Thumb1`, `Index1`… + suffixe.
    fingers: bool,
    mirror: bool,
}

const HERO_RIG: AvatarRig = AvatarRig {
    mode: 1,
    face_yaw: 0.0,
    hips_y: 0.95,
    torso: 0.57,
    spine: &["Hips", "Spine", "Chest"],
    neck: "Head",
    upper_arm: ("UpperArm.L", "UpperArm.R"),
    lower_arm: ("Forearm.L", "Forearm.R"),
    upper_leg: ("Thigh.L", "Thigh.R"),
    lower_leg: ("Shin.L", "Shin.R"),
    hand: ("Hand.L", "Hand.R"),
    fingers: false,
    // Ce rig nomme ses côtés du point de vue du spectateur : `Shoulder.L` est à
    // −X, donc à gauche de l'écran, côté gauche du patient dans le miroir.
    mirror: false,
};

const NINJA_RIG: AvatarRig = AvatarRig {
    mode: 2,
    face_yaw: 0.0,
    hips_y: 0.785,
    torso: 0.868,
    spine: &["Hips", "Abdomen", "Torso"],
    neck: "Neck",
    upper_arm: ("UpperArm.L", "UpperArm.R"),
    lower_arm: ("LowerArm.L", "LowerArm.R"),
    upper_leg: ("UpperLeg.L", "UpperLeg.R"),
    lower_leg: ("LowerLeg.L", "LowerLeg.R"),
    hand: ("", ""),
    fingers: true,
    // `Shoulder.L` à +X (à droite de l'écran) : suit le côté droit du patient.
    mirror: true,
};

/// Script d'un avatar skinné : retargeting des repères captés par **directions
/// d'os** (`bone()`), pas par positions — les longueurs du rig restent les
/// siennes, seule l'orientation de chaque segment suit le patient. Échelle =
/// distance hanches→épaules captée / celle du rig ; origine posée pour que les
/// hanches du rig tombent sur les hanches captées. Un repère non fiable laisse
/// l'os sur sa pose d'animation (`Idle`). Doigts (ninja) : chaînes
/// pouce/index/majeur/auriculaire ← repères de la main (pas d'annulaire dans ce rig).
fn avatar_script(rig: &AvatarRig, idle_clip: &str) -> String {
    let (side_l, side_r) = if rig.mirror { ("r", "l") } else { ("l", "r") };
    let (hand_l, hand_r) = if rig.mirror {
        ("rd_hr_", "rd_hl_")
    } else {
        ("rd_hl_", "rd_hr_")
    };
    let spine: Vec<String> = rig
        .spine
        .iter()
        .map(|n| format!("bone(\"{n}\", dx, dy, 0)"))
        .collect();
    let spine = spine.join("; ");
    let hand_seg = if rig.hand.0.is_empty() {
        String::new()
    } else {
        format!(
            "  hand_dir(\"{hl}\", \"{hand_l}\"); hand_dir(\"{hr}\", \"{hand_r}\")\n",
            hl = rig.hand.0,
            hr = rig.hand.1
        )
    };
    let fingers = if rig.fingers {
        format!("  fingers(\"{hand_l}\", \".L\"); fingers(\"{hand_r}\", \".R\")\n")
    } else {
        String::new()
    };
    format!(
        r#"
local show = math.floor((save.get("rd_avatar") or 1) + 0.5) == {mode} and (save.get("rd_body_shown") or 0) > 0.5
obj.visible = show
obj.anim = "{idle_clip}"
if show then
  local function pt(n) return save.get("rd_p_" .. n .. "_x") or 0, save.get("rd_p_" .. n .. "_y") or 0, (save.get("rd_p_" .. n .. "_ok") or 0) > 0.5 end
  local function dir(ax, ay, bx, by)
    local dx, dy = bx - ax, by - ay
    local l = math.sqrt(dx * dx + dy * dy)
    if l < 1e-4 then return nil end
    return dx / l, dy / l
  end
  local function seg(name, a, b)
    local ax, ay, av = pt(a); local bx, by, bv = pt(b)
    if av and bv then local dx, dy = dir(ax, ay, bx, by); if dx then bone(name, dx, dy, 0) end end
  end
  local hlx, hly, hlv = pt("hip_l"); local hrx, hry, hrv = pt("hip_r")
  local slx, sly, slv = pt("shoulder_l"); local srx, sry, srv = pt("shoulder_r")
  local hx, hy = (hlx + hrx) / 2, (hly + hry) / 2
  local sx, sy = (slx + srx) / 2, (sly + sry) / 2
  local torso = math.sqrt((sx - hx) * (sx - hx) + (sy - hy) * (sy - hy))
  local want = torso / {torso}
  if want < 0.4 then want = 0.4 elseif want > 2.2 then want = 2.2 end
  local scale = save.get("rd_av{mode}_scale") or want
  scale = scale + (want - scale) * (1.0 - math.exp(-6.0 * dt))
  save.set("rd_av{mode}_scale", scale)
  obj.sx = scale; obj.sy = scale; obj.sz = scale
  obj.x = hx; obj.y = hy - {hips_y} * scale; obj.z = 0
  obj.rx = 0; obj.ry = {face_yaw}; obj.rz = 0
  if hlv and hrv and slv and srv then
    local dx, dy = dir(hx, hy, sx, sy)
    if dx then {spine} end
    local nx, ny, nv = pt("nose")
    if nv then local dx2, dy2 = dir(sx, sy, nx, ny); if dx2 then bone("{neck}", dx2, dy2, 0) end end
  end
  seg("{ua_l}", "shoulder_{sl}", "elbow_{sl}"); seg("{la_l}", "elbow_{sl}", "wrist_{sl}")
  seg("{ua_r}", "shoulder_{sr}", "elbow_{sr}"); seg("{la_r}", "elbow_{sr}", "wrist_{sr}")
  seg("{ul_l}", "hip_{sl}", "knee_{sl}"); seg("{ll_l}", "knee_{sl}", "ankle_{sl}")
  seg("{ul_r}", "hip_{sr}", "knee_{sr}"); seg("{ll_r}", "knee_{sr}", "ankle_{sr}")
  local function hp(prefix, i) return save.get(prefix .. i .. "_x") or 0, save.get(prefix .. i .. "_y") or 0 end
  local function hand_dir(name, prefix)
    if (save.get(prefix .. "ok") or 0) < 0.5 then return end
    local ax, ay = hp(prefix, 1); local bx, by = hp(prefix, 10)
    local dx, dy = dir(ax, ay, bx, by)
    if dx then bone(name, dx, dy, 0) end
  end
  local function fingers(prefix, suffix)
    if (save.get(prefix .. "ok") or 0) < 0.5 then return end
    local function fb(name, i, j)
      local ax, ay = hp(prefix, i); local bx, by = hp(prefix, j)
      local dx, dy = dir(ax, ay, bx, by)
      if dx then bone(name .. suffix, dx, dy, 0) end
    end
    fb("Thumb1", 2, 3); fb("Thumb2", 3, 5)
    fb("Index1", 6, 7); fb("Index2", 7, 8); fb("Index3", 8, 9)
    fb("Middle1", 10, 11); fb("Middle2", 11, 12); fb("Middle3", 12, 13)
    fb("Pinky1", 18, 19); fb("Pinky2", 19, 20); fb("Pinky3", 20, 21)
  end
{hand_seg}{fingers}end
"#,
        mode = rig.mode,
        idle_clip = idle_clip,
        torso = rig.torso,
        hips_y = rig.hips_y,
        face_yaw = rig.face_yaw,
        spine = spine,
        neck = rig.neck,
        ua_l = rig.upper_arm.0,
        ua_r = rig.upper_arm.1,
        la_l = rig.lower_arm.0,
        la_r = rig.lower_arm.1,
        ul_l = rig.upper_leg.0,
        ul_r = rig.upper_leg.1,
        ll_l = rig.lower_leg.0,
        ll_r = rig.lower_leg.1,
        sl = side_l,
        sr = side_r,
        hand_seg = hand_seg,
        fingers = fingers,
    )
}

/// Modèle embarqué dans le binaire (`assets::EMBEDDED_MODELS`, schéma
/// `embedded://`) : disponible sur toutes les cibles, web compris.
fn import_embedded_model(imported: &mut Vec<ImportedMesh>, file: &str) -> MeshKind {
    let path = format!("{}{file}", crate::assets::EMBEDDED_SCHEME);
    match crate::scene::import::load_gltf(&path) {
        Ok((data, aabb_min, aabb_max)) => {
            let mut mesh = ImportedMesh {
                name: file.into(),
                path,
                data,
                aabb_min,
                aabb_max,
                ..Default::default()
            };
            mesh.load_skinning();
            let index = imported.len() as u32;
            imported.push(mesh);
            MeshKind::Imported(index)
        }
        Err(e) => {
            log::error!("import_embedded_model({file}) : {e}");
            MeshKind::Capsule
        }
    }
}

/// Script de la cible lumineuse (« Cible ») : suit `rd_target_*`, pulse, prend
/// la couleur de l'exercice (ou le bleu « retour »).
const TARGET_SCRIPT: &str = r#"
local vis = (save.get("rd_target_vis") or 0) > 0.5
obj.visible = vis
if vis then
  obj.x = save.get("rd_target_x") or 0; obj.y = save.get("rd_target_y") or 0; obj.z = 0
  local s = 0.30 * (save.get("rd_target_scale") or 1) * (1.0 + math.sin(time * 5.5) * 0.08)
  obj.sx = s; obj.sy = s; obj.sz = s
  obj.r = save.get("rd_target_r") or 1; obj.g = save.get("rd_target_g") or 1; obj.b = save.get("rd_target_b") or 1
end
"#;

/// Halo translucide autour de la cible : grossit avec le maintien (`rd_dwell_frac`,
/// exercices « Étoile stable » / « Île équilibre »), équivalent de l'anneau de
/// progression de Mouvéo.
const HALO_SCRIPT: &str = r#"
local vis = (save.get("rd_target_vis") or 0) > 0.5
obj.visible = vis
if vis then
  obj.x = save.get("rd_target_x") or 0; obj.y = save.get("rd_target_y") or 0; obj.z = 0
  local s = (0.5 * (1.0 + math.sin(time * 5.5) * 0.08) + 0.4 * (save.get("rd_dwell_frac") or 0)) * (save.get("rd_target_scale") or 1)
  obj.sx = s; obj.sy = s; obj.sz = s
  obj.r = save.get("rd_target_r") or 1; obj.g = save.get("rd_target_g") or 1; obj.b = save.get("rd_target_b") or 1
end
"#;

/// Point suivi (poignet / genou / cheville / hanches) : sphère blanche.
const HAND_SCRIPT: &str = r#"
local vis = (save.get("rd_hand_vis") or 0) > 0.5
obj.visible = vis
if vis then obj.x = save.get("rd_hand_px") or 0; obj.y = save.get("rd_hand_py") or 0; obj.z = 0.05 end
"#;

/// Os de l'avatar (« Os <a>-<b> ») : les 12 segments du squelette de Mouvéo,
/// chacun un cylindre entre deux repères. Des objets plutôt que des
/// `debug.line` : les segments de debug ne vivent qu'un pas de simulation et
/// clignotent dès que l'affichage tourne plus vite que la simulation, alors
/// qu'un objet est interpolé entre deux pas comme tout le reste.
const BONES: [(&str, &str); 12] = [
    ("shoulder_l", "shoulder_r"),
    ("shoulder_l", "elbow_l"),
    ("elbow_l", "wrist_l"),
    ("shoulder_r", "elbow_r"),
    ("elbow_r", "wrist_r"),
    ("shoulder_l", "hip_l"),
    ("shoulder_r", "hip_r"),
    ("hip_l", "hip_r"),
    ("hip_l", "knee_l"),
    ("knee_l", "ankle_l"),
    ("hip_r", "knee_r"),
    ("knee_r", "ankle_r"),
];

/// Script d'un segment (os du corps ou phalange) : cylindre (axe Y, hauteur 1)
/// posé au milieu des deux repères `ka`/`kb` (préfixes de clés `save`, ex.
/// `rd_p_hip_r` ou `rd_h_9`), étiré à leur distance et tourné autour de Z pour
/// les relier — angle calculé par `acos` + signe plutôt qu'`atan2`, absent en
/// Lua 5.1 sous ce nom.
fn segment_script(ka: &str, kb: &str, thickness: f32, z: f32, scale_key: Option<&str>) -> String {
    let f = scale_key.map_or("1".to_string(), |k| format!("(save.get(\"{k}\") or 1)"));
    format!(
        "local vis = (save.get(\"{ka}_vis\") or 0) > 0.5 and (save.get(\"{kb}_vis\") or 0) > 0.5\n\
         obj.visible = vis\n\
         if vis then\n\
           local ax, ay = save.get(\"{ka}_x\") or 0, save.get(\"{ka}_y\") or 0\n\
           local bx, by = save.get(\"{kb}_x\") or 0, save.get(\"{kb}_y\") or 0\n\
           local dx, dy = bx - ax, by - ay\n\
           local len = math.sqrt(dx * dx + dy * dy)\n\
           obj.x = (ax + bx) / 2; obj.y = (ay + by) / 2; obj.z = {z:?}\n\
           obj.sx = {thickness:?} * {f}; obj.sz = {thickness:?} * {f}; obj.sy = math.max(0.01, len)\n\
           if len > 0.0001 then\n\
             local c = dy / len\n\
             if c > 1 then c = 1 elseif c < -1 then c = -1 end\n\
             local ang = math.deg(math.acos(c))\n\
             if dx > 0 then ang = -ang end\n\
             obj.rx = 0; obj.ry = 0; obj.rz = ang\n\
           end\n\
         end\n"
    )
}

/// Repères de la main (MediaPipe Hand Landmarker, 1-based) : couleur par doigt
/// comme dans la visualisation MediaPipe (pouce crème, index violet, majeur
/// jaune, annulaire vert, auriculaire bleu, paume gris-bleu).
fn finger_color(i: usize) -> [f32; 3] {
    match i {
        2..=5 => [1.0, 0.92, 0.72],
        6..=9 => [0.77, 0.71, 0.99],
        10..=13 => [0.99, 0.83, 0.30],
        14..=17 => [0.35, 0.9, 0.45],
        18..=21 => [0.35, 0.55, 1.0],
        _ => [0.85, 0.9, 1.0],
    }
}

/// Les 21 liaisons de la main (paires 1-based) : chaînes des cinq doigts + paume.
const HAND_BONES: [(usize, usize); 21] = [
    (1, 2),
    (2, 3),
    (3, 4),
    (4, 5),
    (1, 6),
    (6, 7),
    (7, 8),
    (8, 9),
    (6, 10),
    (10, 11),
    (11, 12),
    (12, 13),
    (10, 14),
    (14, 15),
    (15, 16),
    (16, 17),
    (14, 18),
    (18, 19),
    (19, 20),
    (20, 21),
    (1, 18),
];

fn hand_joint_script(prefix: &str, i: usize, radius: f32) -> String {
    format!(
        "local vis = (save.get(\"{prefix}{i}_vis\") or 0) > 0.5\n\
         obj.visible = vis\n\
         if vis then\n\
           local f = {radius:?} * (save.get(\"{prefix}scale\") or 1)\n\
           obj.sx = f; obj.sy = f; obj.sz = f\n\
           obj.x = save.get(\"{prefix}{i}_x\") or 0; obj.y = save.get(\"{prefix}{i}_y\") or 0; obj.z = 0.02\n\
         end\n"
    )
}

/// Script d'une sphère d'articulation (« Repère <nom> »).
fn joint_script(name: &str) -> String {
    format!(
        "local vis = (save.get(\"rd_p_{name}_vis\") or 0) > 0.5\n\
         obj.visible = vis\n\
         if vis then obj.x = save.get(\"rd_p_{name}_x\") or 0; obj.y = save.get(\"rd_p_{name}_y\") or 0; obj.z = 0 end\n"
    )
}

fn text_widget(id: &str, anchor: HudAnchor, offset: [f32; 2], font: f32) -> HudWidget {
    HudWidget {
        id: id.into(),
        anchor,
        offset,
        size: [0.0, font],
        kind: HudWidgetKind::Text {
            content: String::new(),
            binding: HudBinding::None,
        },
    }
}

fn button_widget(id: &str, label: &str, action: &str, offset_y: f32) -> HudWidget {
    HudWidget {
        id: id.into(),
        anchor: HudAnchor::BottomRight,
        offset: [-16.0, offset_y],
        size: [170.0, 32.0],
        kind: HudWidgetKind::Button {
            label: label.into(),
            action: action.into(),
        },
    }
}

impl Scene {
    /// Démo « Rééducation — mobilité guidée » (cf. la doc du module).
    pub fn reeducation_demo() -> Self {
        let mut objects = Vec::with_capacity(120);
        let mut imported: Vec<ImportedMesh> = Vec::new();

        // Directeur en tête de liste : les autres objets relisent son état le
        // même tick (les scripts s'exécutent dans l'ordre de `objects`).
        let mut seance = demo_obj("Séance", MeshKind::Sphere, Vec3::new(0.0, -6.0, 0.0));
        seance.transform = seance.transform.with_scale(Vec3::splat(0.05));
        seance.script = director_script();
        objects.push(seance);

        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::ZERO);
        sol.transform = sol.transform.with_scale(Vec3::new(14.0, 1.0, 14.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.06, 0.12, 0.2];
        sol.roughness = 0.9;
        objects.push(sol);

        let mut fond = demo_obj("Fond", MeshKind::Cube, Vec3::new(0.0, 3.0, -1.6));
        fond.transform = fond.transform.with_scale(Vec3::new(12.0, 6.0, 0.1));
        fond.color = [0.05, 0.1, 0.17];
        fond.roughness = 0.95;
        objects.push(fond);

        let mut halo = demo_obj("Halo", MeshKind::Sphere, Vec3::new(0.0, 1.8, 0.0));
        halo.script = HALO_SCRIPT.into();
        halo.opacity = 0.22;
        halo.emissive = 0.6;
        halo.visible = false;
        objects.push(halo);

        let mut cible = demo_obj("Cible", MeshKind::Sphere, Vec3::new(0.0, 1.8, 0.0));
        cible.script = TARGET_SCRIPT.into();
        cible.emissive = 1.4;
        cible.visible = false;
        objects.push(cible);

        for (a, b) in BONES {
            let mut os = demo_obj(&format!("Os {a}-{b}"), MeshKind::Cylinder, Vec3::ZERO);
            os.transform = os.transform.with_scale(Vec3::new(0.07, 0.5, 0.07));
            os.color = [0.85, 0.9, 1.0];
            os.emissive = 0.25;
            os.script = segment_script(&format!("rd_p_{a}"), &format!("rd_p_{b}"), 0.07, 0.0, None);
            os.visible = false;
            objects.push(os);
        }

        for name in JOINTS {
            let mut j = demo_obj(&format!("Repère {name}"), MeshKind::Sphere, Vec3::ZERO);
            let r = if name == "nose" { 0.2 } else { 0.07 };
            j.transform = j.transform.with_scale(Vec3::splat(r));
            j.color = [0.85, 0.9, 1.0];
            j.emissive = 0.35;
            j.script = joint_script(name);
            j.visible = false;
            objects.push(j);
        }

        // Deux mains (21 repères + 21 phalanges chacune, couleur par doigt) :
        // posées sur les poignets du squelette à l'échelle du corps, ou la main
        // travaillée en grand pendant un exercice de doigts (`rd_h<l|r>_scale`).
        for (s, label) in [("l", "gauche"), ("r", "droite")] {
            let prefix = format!("rd_h{s}_");
            for (a, b) in HAND_BONES {
                let mut ph = demo_obj(
                    &format!("Phalange {label} {a}-{b}"),
                    MeshKind::Cylinder,
                    Vec3::ZERO,
                );
                ph.transform = ph.transform.with_scale(Vec3::new(0.05, 0.3, 0.05));
                ph.color = if matches!((a, b), (1, 6) | (6, 10) | (10, 14) | (14, 18) | (1, 18)) {
                    finger_color(1)
                } else {
                    finger_color(b)
                };
                ph.emissive = 0.3;
                ph.script = segment_script(
                    &format!("{prefix}{a}"),
                    &format!("{prefix}{b}"),
                    0.05,
                    0.01,
                    Some(&format!("{prefix}scale")),
                );
                ph.visible = false;
                objects.push(ph);
            }
            for i in 1..=21 {
                let mut d = demo_obj(&format!("Doigt {label} {i}"), MeshKind::Sphere, Vec3::ZERO);
                let r = if i == 1 { 0.09 } else { 0.06 };
                d.transform = d.transform.with_scale(Vec3::splat(r));
                d.color = finger_color(i);
                d.emissive = 0.5;
                d.script = hand_joint_script(&prefix, i, r);
                d.visible = false;
                objects.push(d);
            }
        }

        // Avatars skinnés, pilotés par directions d'os (cf. `avatar_script`) :
        // héros aux proportions humaines (défaut), ninja cartoon avec doigts animés.
        for (name, file, rig, idle) in [
            ("Avatar héros", "fairy_hero.glb", &HERO_RIG, "Idle"),
            (
                "Avatar ninja",
                "monster_ninja_b.glb",
                &NINJA_RIG,
                "CharacterArmature|Idle",
            ),
        ] {
            let mesh = import_embedded_model(&mut imported, file);
            let mut avatar = demo_obj(name, mesh, Vec3::ZERO);
            avatar.animation = Some(AnimationState {
                clip: idle.into(),
                ..Default::default()
            });
            avatar.script = avatar_script(rig, idle);
            avatar.visible = false;
            objects.push(avatar);
        }

        let mut main = demo_obj("Point suivi", MeshKind::Sphere, Vec3::new(0.8, 1.3, 0.05));
        main.transform = main.transform.with_scale(Vec3::splat(0.12));
        main.color = [1.0, 1.0, 1.0];
        main.emissive = 1.0;
        main.script = HAND_SCRIPT.into();
        main.visible = false;
        objects.push(main);

        let hud_widgets = vec![
            text_widget("titre", HudAnchor::TopLeft, [16.0, 12.0], 24.0),
            text_widget("objectif", HudAnchor::TopLeft, [16.0, 48.0], 16.0),
            text_widget("aide", HudAnchor::TopLeft, [16.0, 76.0], 0.0),
            // Sous la rangée ⏸ 🔊 Carte ? du HUD joueur (`editor::hud::mobile_top_buttons`,
            // ~52 px de haut en haut à droite) — pas par-dessus.
            text_widget("score", HudAnchor::TopRight, [-16.0, 64.0], 30.0),
            text_widget("chrono", HudAnchor::TopRight, [-16.0, 106.0], 16.0),
            text_widget("serie", HudAnchor::TopRight, [-16.0, 132.0], 16.0),
            text_widget("celebration", HudAnchor::Center, [0.0, -150.0], 32.0),
            text_widget("bilan", HudAnchor::Center, [0.0, 40.0], 18.0),
            text_widget("consigne", HudAnchor::BottomLeft, [16.0, -64.0], 18.0),
            text_widget("jardin", HudAnchor::BottomLeft, [16.0, -38.0], 0.0),
            text_widget("mention", HudAnchor::BottomLeft, [16.0, -14.0], 0.0),
            button_widget("b_demarrer", "▶ Commencer / Arrêter", "demarrer", -16.0),
            button_widget("b_ex_prev", "◀ Exercice précédent", "ex_prev", -56.0),
            button_widget("b_ex_next", "Exercice suivant ▶", "ex_next", -96.0),
            button_widget("b_cote", "Côté gauche / droit", "cote", -136.0),
            button_widget("b_amplitude", "Amplitude 55-95 %", "amplitude", -176.0),
            button_widget("b_objectif", "Objectif 4-12 rép.", "objectif", -216.0),
            button_widget("b_douleur", "Douleur +1 (bilan)", "douleur", -256.0),
            button_widget("b_fatigue", "Fatigue +1 (bilan)", "fatigue", -296.0),
            button_widget(
                "b_enregistrer",
                "💾 Enregistrer le bilan",
                "enregistrer",
                -336.0,
            ),
            button_widget(
                "b_avatar",
                "Avatar : héros / ninja / bâtons",
                "avatar",
                -376.0,
            ),
        ];

        Scene {
            objects,
            imported,
            camera_follow: false,
            game_camera: Some(GameCamera {
                target: [0.0, 1.5, 0.0],
                yaw: 0.0,
                pitch: 0.03,
                distance: 5.2,
                ortho_height: 0.0,
            }),
            light: Light {
                dir: [0.2, 1.0, 0.6],
                color: [0.95, 0.97, 1.0],
                ambient: 0.35,
            },
            point_lights: vec![PointLight {
                position: [0.0, 3.5, 2.5],
                color: [0.77, 1.0, 0.29],
                intensity: 0.6,
                range: 12.0,
                ..PointLight::default()
            }],
            sky: Sky {
                horizon_color: [0.05, 0.1, 0.17],
                zenith_color: [0.03, 0.07, 0.12],
                fog_color: [0.05, 0.1, 0.17],
                ..Sky::default()
            },
            mobile: MobileControls {
                joystick: true,
                ..Default::default()
            },
            hud_widgets,
            arcade_hud: true,
            ..Default::default()
        }
    }
}
