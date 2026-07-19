-- rotating_object.lua — le script du « Cube tournant » de scene.json.
--
-- Dans RusteeGear, un script vit DANS l'objet (champ « Script » de
-- l'inspecteur), pas dans un fichier : ce fichier est la copie lisible du
-- script inline, pour lecture et copier-coller.
--
-- Exécuté à chaque frame en mode Play :
--   obj  = l'objet porteur (position obj.x/y/z, rotation obj.rx/ry/rz en
--          degrés, échelle obj.sx/sy/sz, couleur obj.r/g/b)
--   dt   = durée de la frame en secondes

obj.ry = obj.ry + 45 * dt
