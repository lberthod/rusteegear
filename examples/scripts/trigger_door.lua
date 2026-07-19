-- trigger_door.lua — script officiel n°3 : porte déclenchée par une zone.
-- À poser sur un objet dont « Trigger » est coché. Portable natif/web :
-- couvert par les tests différentiels de src/app/scripting_web.rs.
--
-- API utilisée : obj.triggered (joueur dans la zone), obj.y, dt,
-- math.min / math.max.
-- La porte monte quand le joueur est dans la zone, redescend sinon.

if obj.triggered then
  obj.y = math.min(obj.y + 2 * dt, 2.5)
else
  obj.y = math.max(obj.y - 2 * dt, 0.5)
end
