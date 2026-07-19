-- zone_signal.lua — le script de la « Zone d'éveil » de scene.json.
--
-- Copie lisible du script inline (cf. rotating_object.lua pour le principe).
--
-- L'objet a « Trigger » coché : en mode Play, le moteur expose
--   obj.triggered = vrai tant que le joueur est dans la zone
--   obj.exited    = vrai à la frame où il en sort
--
-- Ici la zone change simplement de couleur quand on marche dessus —
-- c'est le squelette de n'importe quel déclencheur (porte, piège, checkpoint).

if obj.triggered then
  obj.r = 0.25
  obj.g = 0.8
  obj.b = 0.35
end
if obj.exited then
  obj.r = 0.6
  obj.g = 0.6
  obj.b = 0.25
end
