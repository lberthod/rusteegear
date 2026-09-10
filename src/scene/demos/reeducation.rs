//! Démo « Rééducation — attrape-bulles » (inspirée du prototype web *Mouvéo*,
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
//! | squelette dessiné sur la vidéo                 | mannequin neutre 3D construit sur les repères (ou bâtons) |
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
local SMOOTH_POSE, SMOOTH_HAND = 35.0, 40.0   -- lissage (1/s) des repères caméra, déjà interpolés par le moteur
local VIS_MIN = 0.5                           -- sous cette visibilité, un repère n'est pas dessiné
local NB = 3                                  -- bulles simultanées au maximum (objets « Bulle 1..3 »)
local CATCH_R = 0.32                          -- distance main -> bulle pour l'attraper (m)

local NAMES = {"nose", "shoulder_l", "shoulder_r", "elbow_l", "elbow_r", "wrist_l", "wrist_r",
               "hip_l", "hip_r", "knee_l", "knee_r", "ankle_l", "ankle_r"}
-- Corps virtuel du mode démo (monde, m) : silhouette debout au centre.
local VBODY = {
  nose = {0.0, 2.55}, shoulder_l = {-0.42, 2.25}, shoulder_r = {0.42, 2.25},
  elbow_l = {-0.72, 1.78}, elbow_r = {0.72, 1.78}, wrist_l = {-0.80, 1.30}, wrist_r = {0.80, 1.30},
  hip_l = {-0.24, 1.35}, hip_r = {0.24, 1.35}, knee_l = {-0.27, 0.72}, knee_r = {0.27, 0.72},
  ankle_l = {-0.29, 0.10}, ankle_r = {0.29, 0.10},
}
-- Rythmes : délai entre deux bulles, durée de vie d'une bulle, bulles simultanées.
local LEVELS = {
  {name = "Doux", spawn = 1.8, life = 6.0, max = 1},
  {name = "Moyen", spawn = 1.2, life = 4.5, max = 2},
  {name = "Vif", spawn = 0.8, life = 3.2, max = 3},
}
local COLORS = {{0.77, 1.00, 0.29}, {0.49, 0.83, 0.99}, {0.77, 0.71, 0.99}, {0.99, 0.83, 0.30}, {0.98, 0.44, 0.52}, {0.18, 0.83, 0.75}}

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
-- Tirage pseudo-aléatoire déterministe (Park-Miller, exact en double sur les
-- deux backends) : mêmes bulles pour une même graine, testable.
local function rnd()
  local s = g("rd_seed", 12345)
  s = (s * 16807) % 2147483647
  save.set("rd_seed", s)
  return s / 2147483647
end

local stage = g("rd_stage", 0)      -- 0 accueil, 1 calibration, 2 partie, 3 bilan
local amp = g("rd_amp", 75)
local level = g("rd_level", 2)
local avatar = g("rd_avatar", 0)    -- 0 mannequin, 1 bâtons
local cam = g("rd_cam", 0) > 0.5    -- mode caméra, verrouillé au départ de la partie

-- ---- Boutons du HUD (widgets Button -> hud:<action>) ----
if on_event("hud:amplitude") then amp = amp + 10; if amp > 95 then amp = 55 end end
if on_event("hud:rythme") then level = level % 3 + 1 end
if on_event("hud:avatar") then avatar = (avatar + 1) % 2 end
if on_event("hud:demarrer") then
  if stage == 0 or stage == 3 then
    save.set("rd_points", 0); save.set("rd_combo", 0); save.set("rd_caught", 0); save.set("rd_missed", 0)
    save.set("rd_elapsed", 0); save.set("rd_last_spawn", -10); save.set("rd_calib_start", 0); save.set("rd_finish_at", 0)
    for i = 1, NB do save.set("rd_b" .. i .. "_alive", 0); save.set("rd_b" .. i .. "_pop", -10) end
    cam = pose.ok
    save.set("rd_cam", cam and 1 or 0)
    if cam then
      stage = 1
    else
      for i = 1, #NAMES do local n = NAMES[i]; save.set("rd_a_" .. n .. "_x", VBODY[n][1]); save.set("rd_a_" .. n .. "_y", VBODY[n][2]) end
      save.set("rd_hand_x", VBODY.wrist_r[1]); save.set("rd_hand_y", VBODY.wrist_r[2])
      stage = 2; save.set("rd_status", 8)
    end
  else
    stage = 0 -- « Arrêter »
  end
end
local lv = LEVELS[level]

-- ---- Points du corps (monde) : pose caméra lissée ou corps virtuel ----
local pts = {}
local live = cam and stage >= 1 and stage <= 2
if stage == 0 then live = pose.ok end
-- Normalisation du corps capté : quelle que soit la distance à la caméra, le
-- patient est dessiné à taille constante (torse hanches->épaules = 0,9 m) et
-- centré, hanches à 1,35 m. Identité en mode démo.
local NK, NCX, NCY = 1.0, 0.0, 1.35
if live then
  for i = 1, #NAMES do
    local n = NAMES[i]; local lm = pose[n]; local x, y = P(lm)
    pts[n] = {smooth("rd_s_" .. n .. "_x", x, SMOOTH_POSE), smooth("rd_s_" .. n .. "_y", y, SMOOTH_POSE), lm.v}
  end
  local hx0, hy0 = (pts.hip_l[1] + pts.hip_r[1]) / 2, (pts.hip_l[2] + pts.hip_r[2]) / 2
  local sx0, sy0 = (pts.shoulder_l[1] + pts.shoulder_r[1]) / 2, (pts.shoulder_l[2] + pts.shoulder_r[2]) / 2
  local torso = dist(hx0, hy0, sx0, sy0)
  local want_k = 1.0
  if torso > 0.05 then want_k = clamp(0.9 / torso, 0.35, 3.0) end
  NK = smooth("rd_norm_k", want_k, 4.0)
  NCX = smooth("rd_norm_cx", hx0, 6.0)
  NCY = smooth("rd_norm_cy", hy0, 6.0)
  for i = 1, #NAMES do
    local p = pts[NAMES[i]]
    p[1] = (p[1] - NCX) * NK
    p[2] = 1.35 + (p[2] - NCY) * NK
  end
else
  for i = 1, #NAMES do local n = NAMES[i]; pts[n] = {VBODY[n][1], VBODY[n][2], 1.0} end
end

-- ---- Mains (doigts) : 21 repères par côté, même projection que le corps ----
local function hand_points(Hn, key)
  local out = {}
  for i = 1, 21 do
    local wx, wy = (0.5 - Hn.x[i]) * W, (1.0 - Hn.y[i]) * H
    wx, wy = (wx - NCX) * NK, 1.35 + (wy - NCY) * NK
    out[i] = {smooth(key .. i .. "_x", wx, SMOOTH_HAND), smooth(key .. i .. "_y", wy, SMOOTH_HAND)}
  end
  return out
end
local HPL, HPR = nil, nil
if cam or stage == 0 then
  if hand.left then HPL = hand_points(hand.left, "rd_hl_") end
  if hand.right then HPR = hand_points(hand.right, "rd_hr_") end
end

-- Corps prêt : épaules visibles et au moins une main.
local body_ok = true
if live then
  body_ok = pose.ok and pts.shoulder_l[3] > VIS_MIN and pts.shoulder_r[3] > VIS_MIN
    and (pts.wrist_l[3] > 0.45 or pts.wrist_r[3] > 0.45)
end

-- ---- Calibration (mode caméra) : 1,8 s de corps visible, puis ancres figées ----
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

-- ---- Partie ----
if stage == 2 then
  local A = {}
  for i = 1, #NAMES do local n = NAMES[i]; A[n] = {g("rd_a_" .. n .. "_x", 0), g("rd_a_" .. n .. "_y", 0)} end
  local cx, cy = (A.shoulder_l[1] + A.shoulder_r[1]) / 2, (A.shoulder_l[2] + A.shoulder_r[2]) / 2
  local limb = dist(A.shoulder_r[1], A.shoulder_r[2], A.elbow_r[1], A.elbow_r[2])
    + dist(A.elbow_r[1], A.elbow_r[2], A.wrist_r[1], A.wrist_r[2])
  limb = math.max(0.5, limb)
  local R = limb * (amp / 100) * 1.1   -- rayon de la zone où naissent les bulles

  -- Points de capture : les deux mains (paume si les doigts sont suivis, sinon poignet).
  local catchers = {}
  if cam then
    if HPL then catchers[#catchers + 1] = {(HPL[1][1] + HPL[10][1]) / 2, (HPL[1][2] + HPL[10][2]) / 2}
    elseif pts.wrist_l[3] > 0.45 then catchers[#catchers + 1] = {pts.wrist_l[1], pts.wrist_l[2]} end
    if HPR then catchers[#catchers + 1] = {(HPR[1][1] + HPR[10][1]) / 2, (HPR[1][2] + HPR[10][2]) / 2}
    elseif pts.wrist_r[3] > 0.45 then catchers[#catchers + 1] = {pts.wrist_r[1], pts.wrist_r[2]} end
  else
    -- Mode démo : la main droite se pilote au joystick (ou flèches via tilt)
    -- autour des épaules, lissée ; le coude suit pour garder l'avatar lisible.
    local jx = clamp(input.jx + tilt.x, -1, 1)
    local jy = clamp(input.jy + tilt.y, -1, 1)
    local hx = smooth("rd_hand_x", cx + jx * R * 1.15, 8.0)
    local hy = smooth("rd_hand_y", cy + jy * R * 1.15, 8.0)
    pts.wrist_r = {hx, hy, 1}
    pts.elbow_r = {(A.shoulder_r[1] + hx) / 2 + 0.08, (A.shoulder_r[2] + hy) / 2 - 0.12, 1}
    catchers[1] = {hx, hy}
  end

  local points, combo = g("rd_points", 0), g("rd_combo", 0)
  local caught, missed = g("rd_caught", 0), g("rd_missed", 0)
  if body_ok then
    save.set("rd_elapsed", g("rd_elapsed", 0) + dt)
    -- Naissance d'une bulle dans la zone atteignable (autour des épaules).
    local alive = 0
    for i = 1, NB do if g("rd_b" .. i .. "_alive", 0) > 0.5 then alive = alive + 1 end end
    if alive < lv.max and time - g("rd_last_spawn", -10) >= lv.spawn then
      for i = 1, NB do
        if g("rd_b" .. i .. "_alive", 0) < 0.5 then
          local bx = clamp(cx + (rnd() * 2 - 1) * R, -1.9, 1.9)
          local by = clamp(cy + (rnd() * 1.5 - 0.5) * R, 0.35, 3.0)
          local c = COLORS[math.floor(rnd() * #COLORS) + 1]
          save.set("rd_b" .. i .. "_alive", 1); save.set("rd_b" .. i .. "_x", bx); save.set("rd_b" .. i .. "_y", by)
          save.set("rd_b" .. i .. "_born", time); save.set("rd_b" .. i .. "_life", lv.life)
          save.set("rd_b" .. i .. "_r", c[1]); save.set("rd_b" .. i .. "_g", c[2]); save.set("rd_b" .. i .. "_b", c[3])
          save.set("rd_last_spawn", time)
          break
        end
      end
    end
    -- Capture et expiration.
    for i = 1, NB do
      if g("rd_b" .. i .. "_alive", 0) > 0.5 then
        local bx, by = g("rd_b" .. i .. "_x", 0), g("rd_b" .. i .. "_y", 0)
        local got = false
        for k = 1, #catchers do
          if dist(catchers[k][1], catchers[k][2], bx, by) < CATCH_R then got = true end
        end
        if got then
          caught = caught + 1; combo = combo + 1
          local gain = 10 + math.floor(combo / 3) * 5
          points = points + gain
          save.set("rd_b" .. i .. "_alive", 0); save.set("rd_b" .. i .. "_pop", time)
          save.set("rd_cel_gain", gain); save.set("rd_cel", 1); save.set("rd_cel_until", time + 0.8)
          if combo == 5 then save.set("rd_cel", 2); save.set("rd_cel_until", time + 1.3)
          elseif combo == 10 then save.set("rd_cel", 3); save.set("rd_cel_until", time + 1.3) end
          save.set("rd_status", 3)
          vibrate(45)
        elseif time - g("rd_b" .. i .. "_born", time) > lv.life then
          missed = missed + 1; combo = 0
          save.set("rd_b" .. i .. "_alive", 0)
          save.set("rd_status", 9)
        end
      end
    end
  end
  save.set("rd_points", points); save.set("rd_combo", combo); save.set("rd_caught", caught); save.set("rd_missed", missed)
  if g("rd_elapsed", 0) >= SESSION_S then
    save.set("rd_games", g("rd_games", 0) + 1)
    if points > g("rd_best", 0) then save.set("rd_best", points) end
    for i = 1, NB do save.set("rd_b" .. i .. "_alive", 0) end
    stage = 3
  end
end

-- ---- Sorties partagées avec les autres objets (repères, os, doigts, bulles) ----
save.set("rd_stage", stage); save.set("rd_amp", amp); save.set("rd_level", level); save.set("rd_avatar", avatar)
local show_body = (stage ~= 3) and (not live or pose.ok)
save.set("rd_body_shown", show_body and 1 or 0)
save.set("rd_mannequin", (avatar == 0) and 1 or 0)
for i = 1, #NAMES do
  local n = NAMES[i]
  -- Un repère mal vu (hors cadre, extrapolé par le modèle) n'est pas dessiné :
  -- sans ça, un os file vers un point fantasque hors de l'écran.
  local ok = show_body and pts[n][3] >= VIS_MIN
  save.set("rd_p_" .. n .. "_ok", ok and 1 or 0)
  save.set("rd_p_" .. n .. "_vis", ok and 1 or 0)
  save.set("rd_p_" .. n .. "_x", pts[n][1]); save.set("rd_p_" .. n .. "_y", pts[n][2])
end
local function write_hand(prefix, pts_h)
  save.set(prefix .. "scale", 0.45)
  save.set(prefix .. "ok", pts_h and 1 or 0)
  for i = 1, 21 do
    if pts_h then
      save.set(prefix .. i .. "_vis", 1); save.set(prefix .. i .. "_x", pts_h[i][1]); save.set(prefix .. i .. "_y", pts_h[i][2])
    else
      save.set(prefix .. i .. "_vis", 0)
    end
  end
end
write_hand("rd_hl_", show_body and HPL or nil)
write_hand("rd_hr_", show_body and HPR or nil)

-- ---- HUD ----
local STATUS = {
  [0] = "Réglez le rythme, puis ▶ Commencer",
  [1] = "Reculez : tête, épaules et mains doivent être visibles",
  [2] = "Ne bougez plus, calibration…",
  [3] = "Attrapez les bulles avec vos mains !",
  [8] = "Mode démo — joystick ou flèches : amenez la main sur les bulles",
  [9] = "Bulle éclatée… la suivante arrive",
}
hud_text("titre", "MOUVÉO · Attrape-bulles")
local reglages = "Rythme " .. lv.name .. " · amplitude " .. num(amp) .. " % · " .. num(SESSION_S) .. " s"
if stage == 0 then
  local src = pose.ok and "📷 Caméra détectée : attrapez avec vos deux mains." or "🎮 Sans caméra : mode démo, la main se pilote au joystick ou aux flèches."
  hud_text("aide", "Des bulles apparaissent autour de vous : touchez-les avant qu'elles n'éclatent.\nChaque bulle vaut 10 points, les séries rapportent plus.\n" .. reglages .. "\n" .. src)
  hud_text("consigne", STATUS[0])
  hud_text("score", ""); hud_text("chrono", ""); hud_text("serie", ""); hud_text("objectif", ""); hud_text("bilan", "")
elseif stage == 1 then
  hud_text("aide", reglages)
  if body_ok then hud_text("consigne", STATUS[2] .. " " .. num(calib * 100) .. " %") else hud_text("consigne", STATUS[1]) end
  hud_text("score", ""); hud_text("chrono", ""); hud_text("serie", ""); hud_text("objectif", ""); hud_text("bilan", "")
elseif stage == 2 then
  local points, combo = g("rd_points", 0), g("rd_combo", 0)
  local left = math.max(0, SESSION_S - g("rd_elapsed", 0))
  local sec = math.floor(left)
  hud_text("score", num(points) .. " pts")
  hud_text("chrono", "Temps 0:" .. ((sec < 10) and "0" or "") .. num(sec))
  hud_text("serie", (combo >= 3) and ("⚡ série ×" .. num(combo)) or "")
  hud_text("objectif", "Bulles attrapées " .. num(g("rd_caught", 0)) .. " · éclatées " .. num(g("rd_missed", 0)))
  hud_text("aide", reglages)
  if not body_ok then hud_text("consigne", "⏸ Repositionnez-vous — partie en pause")
  else hud_text("consigne", STATUS[g("rd_status", 3)] or STATUS[3]) end
  hud_text("bilan", "")
else
  local points, caught, missed = g("rd_points", 0), g("rd_caught", 0), g("rd_missed", 0)
  local total = caught + missed
  local prec = (total > 0) and num(caught / total * 100) or "0"
  local stars = (caught >= 20) and "★★★" or ((caught >= 12) and "★★☆" or ((caught >= 5) and "★☆☆" or "☆☆☆"))
  hud_text("bilan", "🏆 Partie terminée — " .. num(points) .. " points " .. stars .. "\n"
    .. num(caught) .. " bulles attrapées sur " .. num(total) .. " · précision " .. prec .. " %\n"
    .. "Record " .. num(g("rd_best", 0)) .. " pts · " .. num(g("rd_games", 0)) .. " partie" .. ((g("rd_games", 0) == 1) and "" or "s") .. "\n"
    .. "▶ Rejouer")
  hud_text("consigne", "Bougez sans douleur : arrêtez en cas de douleur, vertige ou inconfort inhabituel.")
  hud_text("score", num(points) .. " pts"); hud_text("chrono", ""); hud_text("serie", ""); hud_text("objectif", "")
  hud_text("aide", reglages)
end
local cel = g("rd_cel", 0)
if cel > 0 and time < g("rd_cel_until", 0) then
  local CEL = {"+" .. num(g("rd_cel_gain", 10)), "✨ Série de 5 !", "🏆 Série de 10 !"}
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

/// Script d'une bulle (« Bulle <i> ») : suit `rd_b<i>_*`, flotte doucement,
/// puis gonfle un court instant quand elle est attrapée (`rd_b<i>_pop`).
fn bubble_script(i: usize) -> String {
    format!(
        "local alive = (save.get(\"rd_b{i}_alive\") or 0) > 0.5\n\
         local since = time - (save.get(\"rd_b{i}_pop\") or -10)\n\
         if alive then\n\
           obj.visible = true\n\
           obj.x = save.get(\"rd_b{i}_x\") or 0; obj.y = (save.get(\"rd_b{i}_y\") or 0) + math.sin(time * 2.0 + {i}) * 0.04; obj.z = 0.1\n\
           local s = 0.34 * (1.0 + math.sin(time * 5.0 + {i}) * 0.06)\n\
           obj.sx = s; obj.sy = s; obj.sz = s\n\
           obj.r = save.get(\"rd_b{i}_r\") or 1; obj.g = save.get(\"rd_b{i}_g\") or 1; obj.b = save.get(\"rd_b{i}_b\") or 1\n\
         elseif since >= 0 and since < 0.25 then\n\
           obj.visible = true\n\
           local s = 0.34 * (1.0 + since * 4.0)\n\
           obj.sx = s; obj.sy = s; obj.sz = s\n\
         else\n\
           obj.visible = false\n\
         end\n"
    )
}

/// Halo d'une bulle : rétrécit à mesure que sa durée de vie s'écoule — c'est
/// le compte à rebours visible avant qu'elle n'éclate.
fn bubble_halo_script(i: usize) -> String {
    format!(
        "local alive = (save.get(\"rd_b{i}_alive\") or 0) > 0.5\n\
         obj.visible = alive\n\
         if alive then\n\
           obj.x = save.get(\"rd_b{i}_x\") or 0; obj.y = (save.get(\"rd_b{i}_y\") or 0) + math.sin(time * 2.0 + {i}) * 0.04; obj.z = 0.1\n\
           local life = save.get(\"rd_b{i}_life\") or 4\n\
           local left = 1.0 - (time - (save.get(\"rd_b{i}_born\") or time)) / life\n\
           if left < 0 then left = 0 end\n\
           local s = 0.42 + 0.5 * left\n\
           obj.sx = s; obj.sy = s; obj.sz = s\n\
           obj.r = save.get(\"rd_b{i}_r\") or 1; obj.g = save.get(\"rd_b{i}_g\") or 1; obj.b = save.get(\"rd_b{i}_b\") or 1\n\
         end\n"
    )
}

/// Os de l'avatar (« Os <a>-<b> ») : les 12 segments du squelette de Mouvéo,
/// chacun une capsule entre deux repères.
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

/// Script d'un segment (os du corps ou phalange) : mesh à axe Y et hauteur 1
/// (cylindre ou capsule) posé au milieu des deux repères `ka`/`kb` (préfixes de
/// clés `save`, ex. `rd_p_hip_r` ou `rd_h_9`), étiré à leur distance et tourné
/// autour de Z pour les relier — angle calculé par `acos` + signe plutôt
/// qu'`atan2`, absent en Lua 5.1 sous ce nom. Deux largeurs (échelle x/z) :
/// `width_m` en mode mannequin, `width_s` en mode bâtons (`rd_mannequin`) ;
/// `scale_key` multiplie encore la largeur (mains agrandies).
fn segment_script(
    ka: &str,
    kb: &str,
    width_m: f32,
    width_s: f32,
    z: f32,
    scale_key: Option<&str>,
) -> String {
    let f = scale_key.map_or("1".to_string(), |k| format!("(save.get(\"{k}\") or 1)"));
    format!(
        "local vis = (save.get(\"{ka}_vis\") or 0) > 0.5 and (save.get(\"{kb}_vis\") or 0) > 0.5\n\
         obj.visible = vis\n\
         if vis then\n\
           local w = (((save.get(\"rd_mannequin\") or 1) > 0.5) and {width_m:?} or {width_s:?}) * {f}\n\
           local ax, ay = save.get(\"{ka}_x\") or 0, save.get(\"{ka}_y\") or 0\n\
           local bx, by = save.get(\"{kb}_x\") or 0, save.get(\"{kb}_y\") or 0\n\
           local dx, dy = bx - ax, by - ay\n\
           local len = math.sqrt(dx * dx + dy * dy)\n\
           obj.x = (ax + bx) / 2; obj.y = (ay + by) / 2; obj.z = {z:?}\n\
           obj.sx = w; obj.sz = w; obj.sy = math.max(0.01, len)\n\
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

/// Script d'une pièce ellipsoïdale du mannequin (sphère étirée) posée entre
/// deux repères `ka` → `kb` et orientée le long : `along` = allongement ajouté
/// à la distance (diamètre le long de l'axe), `wide`/`deep` = diamètres
/// transversaux. Visible seulement en mode mannequin et repères fiables.
fn ellipsoid_script(ka: &str, kb: &str, along: f32, wide: f32, deep: f32, z: f32) -> String {
    format!(
        "local vis = (save.get(\"rd_mannequin\") or 0) > 0.5 and (save.get(\"{ka}_ok\") or 0) > 0.5 and (save.get(\"{kb}_ok\") or 0) > 0.5\n\
         obj.visible = vis\n\
         if vis then\n\
           local ax, ay = save.get(\"{ka}_x\") or 0, save.get(\"{ka}_y\") or 0\n\
           local bx, by = save.get(\"{kb}_x\") or 0, save.get(\"{kb}_y\") or 0\n\
           local dx, dy = bx - ax, by - ay\n\
           local len = math.sqrt(dx * dx + dy * dy)\n\
           obj.x = (ax + bx) / 2; obj.y = (ay + by) / 2; obj.z = {z:?}\n\
           obj.sx = {wide:?}; obj.sz = {deep:?}; obj.sy = len + {along:?}\n\
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

/// Points milieu virtuels du mannequin (milieu des hanches, des épaules, base
/// du cou) : posés par ce script sur l'objet « Repère milieu » (invisible) et
/// publiés en `rd_m_<nom>_*` pour les pièces du torse et du cou.
const MIDPOINTS_SCRIPT: &str = r#"
obj.visible = false
local function pt(n) return save.get("rd_p_" .. n .. "_x") or 0, save.get("rd_p_" .. n .. "_y") or 0, (save.get("rd_p_" .. n .. "_ok") or 0) > 0.5 end
local hlx, hly, hlv = pt("hip_l"); local hrx, hry, hrv = pt("hip_r")
local slx, sly, slv = pt("shoulder_l"); local srx, sry, srv = pt("shoulder_r")
local nx, ny, nv = pt("nose")
save.set("rd_m_hips_x", (hlx + hrx) / 2); save.set("rd_m_hips_y", (hly + hry) / 2); save.set("rd_m_hips_ok", (hlv and hrv) and 1 or 0)
local sx, sy = (slx + srx) / 2, (sly + sry) / 2
save.set("rd_m_shoulders_x", sx); save.set("rd_m_shoulders_y", sy); save.set("rd_m_shoulders_ok", (slv and srv) and 1 or 0)
save.set("rd_m_chin_x", sx + (nx - sx) * 0.55); save.set("rd_m_chin_y", sy + (ny - sy) * 0.55); save.set("rd_m_chin_ok", (nv and slv and srv) and 1 or 0)
"#;

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

/// Script d'une sphère d'articulation du corps (« Repère <nom> ») : diamètre
/// `size_m` en mode mannequin (couleur chair), `size_s` en bâtons (bleu pâle) ;
/// `dy` décale verticalement (tête posée au-dessus du nez).
fn body_joint_script(name: &str, size_m: f32, size_s: f32, dy: f32) -> String {
    format!(
        "local vis = (save.get(\"rd_p_{name}_vis\") or 0) > 0.5\n\
         obj.visible = vis\n\
         if vis then\n\
           local m = (save.get(\"rd_mannequin\") or 1) > 0.5\n\
           local f = m and {size_m:?} or {size_s:?}\n\
           obj.sx = f; obj.sy = f; obj.sz = f\n\
           if m then obj.r = 0.92; obj.g = 0.84; obj.b = 0.74 else obj.r = 0.85; obj.g = 0.9; obj.b = 1.0 end\n\
           obj.x = save.get(\"rd_p_{name}_x\") or 0; obj.y = (save.get(\"rd_p_{name}_y\") or 0) + (m and {dy:?} or 0); obj.z = 0\n\
         end\n"
    )
}

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

        for i in 1..=3 {
            let mut halo = demo_obj(
                &format!("Halo {i}"),
                MeshKind::Sphere,
                Vec3::new(0.0, 1.8, 0.1),
            );
            halo.script = bubble_halo_script(i);
            halo.opacity = 0.22;
            halo.emissive = 0.6;
            halo.visible = false;
            objects.push(halo);
            let mut bulle = demo_obj(
                &format!("Bulle {i}"),
                MeshKind::Sphere,
                Vec3::new(0.0, 1.8, 0.1),
            );
            bulle.script = bubble_script(i);
            bulle.emissive = 1.4;
            bulle.visible = false;
            objects.push(bulle);
        }

        // Os du corps : capsules épaisses (mannequin) ou fines (bâtons). Largeur
        // = 4 × rayon voulu (la capsule unité a un rayon de 0,25).
        for (a, b) in BONES {
            let (wm, ws) = match (a, b) {
                ("shoulder_l", "elbow_l") | ("shoulder_r", "elbow_r") => (0.32, 0.06),
                ("elbow_l", "wrist_l") | ("elbow_r", "wrist_r") => (0.26, 0.06),
                ("hip_l", "knee_l") | ("hip_r", "knee_r") => (0.42, 0.06),
                ("knee_l", "ankle_l") | ("knee_r", "ankle_r") => (0.32, 0.06),
                // Lignes des épaules, des hanches et des flancs : cachées dans le
                // torse et le bassin du mannequin.
                _ => (0.12, 0.06),
            };
            let mut os = demo_obj(&format!("Os {a}-{b}"), MeshKind::Capsule, Vec3::ZERO);
            os.transform = os.transform.with_scale(Vec3::new(0.06, 0.5, 0.06));
            os.color = [0.88, 0.82, 0.76];
            os.roughness = 0.75;
            os.emissive = 0.05;
            os.script = segment_script(
                &format!("rd_p_{a}"),
                &format!("rd_p_{b}"),
                wm,
                ws,
                0.0,
                None,
            );
            os.visible = false;
            objects.push(os);
        }

        for name in JOINTS {
            let (sm, ss, dy) = match name {
                "nose" => (0.30, 0.2, 0.04),
                "shoulder_l" | "shoulder_r" => (0.26, 0.07, 0.0),
                "elbow_l" | "elbow_r" => (0.16, 0.07, 0.0),
                "wrist_l" | "wrist_r" => (0.13, 0.07, 0.0),
                "hip_l" | "hip_r" => (0.20, 0.07, 0.0),
                "knee_l" | "knee_r" => (0.20, 0.07, 0.0),
                _ => (0.14, 0.07, 0.0),
            };
            let mut j = demo_obj(&format!("Repère {name}"), MeshKind::Sphere, Vec3::ZERO);
            j.transform = j.transform.with_scale(Vec3::splat(ss));
            j.color = [0.88, 0.82, 0.76];
            j.roughness = 0.9;
            j.emissive = 0.05;
            j.script = body_joint_script(name, sm, ss, dy);
            j.visible = false;
            objects.push(j);
        }

        // Pièces du mannequin : points milieu, torse, bassin, cou, pieds.
        let mut milieu = demo_obj("Repère milieu", MeshKind::Sphere, Vec3::new(0.0, -6.0, 0.0));
        milieu.transform = milieu.transform.with_scale(Vec3::splat(0.01));
        milieu.script = MIDPOINTS_SCRIPT.into();
        milieu.visible = false;
        objects.push(milieu);
        for (name, ka, kb, along, wide, deep, z) in [
            (
                "Torse",
                "rd_m_hips",
                "rd_m_shoulders",
                0.30,
                0.62,
                0.32,
                -0.04,
            ),
            (
                "Bassin",
                "rd_p_hip_l",
                "rd_p_hip_r",
                0.24,
                0.34,
                0.30,
                -0.03,
            ),
            ("Cou", "rd_m_shoulders", "rd_m_chin", 0.02, 0.13, 0.13, 0.0),
            (
                "Pied gauche",
                "rd_p_ankle_l",
                "rd_p_ankle_l",
                0.12,
                0.13,
                0.30,
                0.08,
            ),
            (
                "Pied droit",
                "rd_p_ankle_r",
                "rd_p_ankle_r",
                0.12,
                0.13,
                0.30,
                0.08,
            ),
        ] {
            let mut piece = demo_obj(name, MeshKind::Sphere, Vec3::ZERO);
            piece.color = [0.88, 0.82, 0.76];
            piece.roughness = 0.75;
            piece.emissive = 0.05;
            piece.script = ellipsoid_script(ka, kb, along, wide, deep, z);
            piece.visible = false;
            objects.push(piece);
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
            text_widget("consigne", HudAnchor::BottomLeft, [16.0, -40.0], 18.0),
            text_widget("mention", HudAnchor::BottomLeft, [16.0, -14.0], 0.0),
            button_widget("b_demarrer", "▶ Commencer / Arrêter", "demarrer", -16.0),
            button_widget("b_rythme", "Rythme doux / moyen / vif", "rythme", -56.0),
            button_widget("b_amplitude", "Amplitude 55-95 %", "amplitude", -96.0),
            button_widget("b_avatar", "Avatar : mannequin / bâtons", "avatar", -136.0),
        ];

        Scene {
            objects,
            camera_follow: false,
            game_camera: Some(GameCamera {
                // Un peu de recul : le corps normalisé (≈ 2,6 m avec les bras
                // levés) tient dans l'image avec de la marge pour le HUD.
                target: [0.0, 1.55, 0.0],
                yaw: 0.0,
                pitch: 0.03,
                distance: 6.4,
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
