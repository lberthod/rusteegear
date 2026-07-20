#!/usr/bin/env python3
"""Garde-fou du bundle embarqué (audit 2026-07-20, risque A2).

`assets/bundle/` est figé dans le binaire par `include_dir!` (src/assets.rs) :
chaque fichier non référencé par `assets/player_scene.json` est du poids mort
embarqué chez tous les joueurs — et les renumérotations `mNN` successives de la
resynchro en accumulaient (~394 orphelins constatés le 20 juillet 2026, bundle
de 715 fichiers pour 321 clés utilisées).

Mode check (défaut, utilisé par la CI) : échoue si un orphelin ou une clé
référencée mais absente existe. Mode `--fix` : supprime les orphelins.

Fichiers toujours conservés : `.gitkeep`, `default_settings.json`
(cf. `assets::DEFAULT_SETTINGS_FILE`, écrit par l'export, pas par la scène).
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = ROOT / "assets" / "bundle"
SCENE = ROOT / "assets" / "player_scene.json"
KEEP_ALWAYS = {".gitkeep", "default_settings.json"}


def main() -> int:
    fix = "--fix" in sys.argv[1:]
    referenced = set(re.findall(r"bundle://([^\"]+)", SCENE.read_text(encoding="utf-8")))
    if not referenced:
        print("ERREUR : aucune clé bundle:// dans player_scene.json — scène suspecte.")
        return 1

    on_disk = {p.name for p in BUNDLE.iterdir() if p.is_file()}
    missing = sorted(referenced - on_disk)
    orphans = sorted(on_disk - referenced - KEEP_ALWAYS)

    if missing:
        print(f"ERREUR : {len(missing)} clé(s) référencée(s) par player_scene.json "
              f"absente(s) d'assets/bundle/ :")
        for k in missing:
            print(f"  - {k}")
        return 1

    if orphans:
        if fix:
            for k in orphans:
                (BUNDLE / k).unlink()
            print(f"OK : {len(orphans)} orphelin(s) supprimé(s), "
                  f"{len(referenced)} clé(s) référencée(s) conservée(s).")
            return 0
        print(f"ERREUR : {len(orphans)} fichier(s) d'assets/bundle/ non référencé(s) "
              f"par player_scene.json (poids mort embarqué dans le binaire).")
        print("Relancer : python3 scripts/check_bundle_orphans.py --fix")
        for k in orphans[:10]:
            print(f"  - {k}")
        if len(orphans) > 10:
            print(f"  … et {len(orphans) - 10} autres")
        return 1

    print(f"OK : bundle aligné sur la scène ({len(referenced)} clés, aucun orphelin).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
