-- simple_enemy.lua — script officiel n°4 : poursuite du joueur.
-- Portable natif/web : couvert par les tests différentiels de
-- src/app/scripting_web.rs.
--
-- API utilisée : find_tag(tag) → liste de positions {x, y, z}, obj.x/z, dt.
-- Donnez le tag « joueur » à l'objet à poursuivre.

local cibles = find_tag('joueur')
if #cibles > 0 then
  local c = cibles[1]
  local dx = c.x - obj.x
  local dz = c.z - obj.z
  local d = math.sqrt(dx * dx + dz * dz)
  if d > 0.1 then
    obj.x = obj.x + dx / d * 2 * dt
    obj.z = obj.z + dz / d * 2 * dt
  end
end
