-- rotate.lua — script officiel n°1 : rotation continue.
-- Portable natif (mlua) / web (rilua) : couvert par les tests différentiels
-- de src/app/scripting_web.rs (official_scripts_match_between_backends).
--
-- API utilisée : obj.ry (degrés), dt (secondes).

obj.ry = obj.ry + 45 * dt
