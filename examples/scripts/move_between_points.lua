-- move_between_points.lua — script officiel n°2 : va-et-vient entre deux points.
-- Portable natif/web : couvert par les tests différentiels de
-- src/app/scripting_web.rs.
--
-- API utilisée : obj.x, time (secondes de jeu), math.sin.
-- L'objet oscille entre x = -3 et x = +3 autour de son axe.

obj.x = math.sin(time * 0.8) * 3
