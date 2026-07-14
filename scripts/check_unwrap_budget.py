#!/usr/bin/env python3
"""Garde CI (Sprint 113b) : empêche la régression silencieuse constatée entre le
Sprint 46 (« 0 unwrap/expect en code de prod », scope gfx/renderer.rs + lib.rs
seulement) et l'audit complet du Sprint 113b (crate entier).

Compte les `.unwrap()`/`.expect(`/`panic!(` de `src/**/*.rs` en excluant tout ce qui
est à l'intérieur d'un module `#[cfg(test)] mod tests { ... }` (détection par
correspondance d'accolades, pas une regex ligne à ligne — un module de test peut
s'étendre sur des centaines de lignes). Échoue si le total dépasse la liste blanche
ci-dessous ; chaque entrée whitelistée doit avoir un commentaire au site d'appel
expliquant pourquoi l'invariant est garanti (cf. ROADMAP_SPRINTS.md, Sprint 113b).
"""

import re
import sys
import glob

# Sites audités au Sprint 113b, chacun avec un commentaire au site d'appel
# justifiant l'invariant garanti. Toute nouvelle occurrence hors de cette liste
# doit soit être supprimée (durcie en Result/let-else), soit ajoutée ici avec la
# même justification en commentaire dans le code.
WHITELIST = {
    ("src/gfx/renderer.rs", "unwrap"): 1,
    ("src/net/interpolation.rs", "expect"): 1,
    ("src/app/combat.rs", "expect"): 1,
    ("src/app/simulation.rs", "expect"): 2,
    ("src/app/network_client.rs", "expect"): 1,
}


def find_test_ranges(lines):
    """Plages de lignes (inclusives, 0-indexées) couvertes par un module de test
    (`#[cfg(...test...)] mod tests { ... }`), attribut potentiellement multi-lignes."""
    ranges = []
    i = 0
    n = len(lines)
    while i < n:
        if re.match(r"^\s*(pub(\(\w+\))?\s+)?mod\s+tests?\s*\{", lines[i]):
            start = i
            j = i - 1
            while j >= 0 and (lines[j].strip() == "" or lines[j].strip().startswith("//")):
                j -= 1
            k = j
            attr_start = i
            while k >= 0:
                s = lines[k].strip()
                if s == "":
                    k -= 1
                    continue
                if s.startswith("#[") or (")]" in s and k != j):
                    attr_start = k
                    if s.startswith("#["):
                        break
                    k -= 1
                    continue
                break
            depth = 0
            k2 = start
            started = False
            while k2 < n:
                depth += lines[k2].count("{") - lines[k2].count("}")
                if "{" in lines[k2]:
                    started = True
                if started and depth == 0:
                    ranges.append((attr_start, k2))
                    break
                k2 += 1
            i = k2 + 1
            continue
        i += 1
    return ranges


def scan(path):
    with open(path, encoding="utf-8") as f:
        lines = f.read().split("\n")
    test_ranges = find_test_ranges(lines)

    def in_test(idx):
        return any(a <= idx <= b for a, b in test_ranges)

    hits = {"unwrap": [], "expect": [], "panic": []}
    for idx, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("//"):
            continue
        if in_test(idx):
            continue
        if re.search(r"\.unwrap\(\)", line):
            hits["unwrap"].append(idx + 1)
        if re.search(r"\.expect\(", line):
            hits["expect"].append(idx + 1)
        if re.search(r"\bpanic!\(", line):
            hits["panic"].append(idx + 1)
    return hits


def main():
    total_over_budget = []
    grand_total = 0
    for path in sorted(glob.glob("src/**/*.rs", recursive=True)):
        hits = scan(path)
        for kind, lines_hit in hits.items():
            count = len(lines_hit)
            if count == 0:
                continue
            grand_total += count
            budget = WHITELIST.get((path, kind), 0)
            if count > budget:
                total_over_budget.append((path, kind, count, budget, lines_hit))

    if total_over_budget:
        print("Budget unwrap/expect/panic dépassé (Sprint 113b) :\n")
        for path, kind, count, budget, lines_hit in total_over_budget:
            print(f"  {path}: {count} `{kind}` hors tests (budget {budget}) — lignes {lines_hit}")
        print(
            "\nDurcir en Result/let-else, ou si l'invariant est réellement garanti, "
            "documenter au site d'appel (commentaire expliquant pourquoi) et ajouter "
            "l'entrée à WHITELIST dans scripts/check_unwrap_budget.py."
        )
        sys.exit(1)

    print(f"OK : {grand_total} unwrap/expect/panic en code de production, tous whitelistés.")


if __name__ == "__main__":
    main()
