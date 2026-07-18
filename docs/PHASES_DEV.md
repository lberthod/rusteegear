# Niches de marché & phases de développement avant le testing

> Document de cadrage produit, niveau macro — au-dessus des sprints techniques détaillés dans
> [`ROADMAP_SPRINTS.md`](ROADMAP_SPRINTS.md) et du game design dans [`GDD_MMORPG.md`](../GDD_MMORPG.md).
> Objectif : trancher la niche prioritaire et situer où en est réellement le projet avant d'aborder
> le testing (beta fermée, QA, beta ouverte — volontairement hors scope de ce document).

## 1. Comparatif des deux niches envisagées

| Critère | 🎮 Petits jeux coopératifs 3D | 📚 Serious games / EdTech |
|---|---|---|
| **Réutilisation de l'existant** | ✅ Le serveur autoritaire (`src/net/server_loop.rs`), le protocole réseau (`src/net/protocol.rs`) et un premier jeu jouable, *Le Hameau des Braises* (`GDD_MMORPG.md`), existent déjà dans le dépôt. | 🟡 Le moteur (rendu, réseau, portage mobile) est réutilisable, mais aucun pipeline de contenu pédagogique/médical n'existe encore. |
| **Time-to-market** | ✅ Prochaine étape = polir un MVP déjà entamé, pas repartir de zéro. | 🔴 Nécessite du contenu et des cas d'usage entièrement nouveaux. |
| **Risque technique** | ✅ Repose sur des phases déjà largement terminées ou avancées (A→I, M-net→Q-net — voir §2). | 🔴 La brique différenciante (portabilité web/XR) repose sur les Phases **Q (Web)** et **R (WebXR)**, pas encore atteintes (sprints 114→120). |
| **Différenciation marché** | 🟡 Marché du petit coop 3D est concurrentiel, mais RusteeGear a déjà un produit jouable pour s'y positionner. | ✅ Portabilité web/mobile, légèreté et contrôle des données sont des avantages réels face à Unreal/Unity dans ce secteur, sans avoir à rivaliser sur le photoréalisme. |
| **Alignement profil perso** | 🟡 Pas de lien particulier avec le profil de l'utilisateur. | ✅ Cohérent avec le profil personnel. |

**Conclusion** : ce n'est pas un choix exclusif — c'est un **séquencement**. La niche coop 3D est prête à être poussée vers un testing à court terme parce qu'elle capitalise sur ce qui est déjà construit. La niche EdTech reste stratégiquement pertinente mais dépend de briques (Web, WebXR) situées plus loin dans la roadmap ; elle doit être préparée en parallèle (cadrage, contacts marché) sans bloquer la sortie du produit coop.

## 2. Grille des phases de développement avant le testing

Six phases macro, chacune reliée à sa correspondance réelle dans `ROADMAP_SPRINTS.md` (table "Vue d'ensemble des phases") pour éviter tout renvoi approximatif.

| # | Phase macro | Objectif | Critère de sortie (definition of done) | Correspond à |
|---|---|---|---|---|
| 0 | **Cadrage & choix de niche** | Trancher la niche prioritaire et la proposition de valeur | Niche prioritaire actée, proposition de valeur écrite en une phrase | Décision produit — pas un sprint |
| 1 | **Prototype technique** | Boucle réseau autoritaire + rendu de base qui tournent | Un client se connecte à un serveur autoritaire et voit une scène rendue | Phases **A, B** (7→13) et **M-net→Q-net** (50→79+80/82) — largement déjà faites |
| 2 | **MVP jouable** | Boucle de jeu complète de bout en bout, contenu minimal (1 carte, 1-2 classes, ennemis de base) | Une partie se joue du début à la fin sans intervention développeur | État actuel du *Hameau des Braises* à évaluer contre ce seuil |
| 3 | **Vertical slice / contenu** | Étoffer cartes, classes, créatures, objectifs jusqu'à une tranche représentative | Le contenu couvre au moins un cycle complet (vagues/survie/escorte/boss du GDD) | Phases **K, L, M, N** (80→99) — golden tests, animation squelettale, image, chaîne gameplay |
| 4 | **Alpha interne** | Feature-complete pour le MVP, jouable en continu sans intervention développeur | Sessions répétées sans crash ni contournement manuel | Phases **O, P, P2** (100→113f) — physique/feel, audio/HUD/confort, dette/sécurité/accessibilité |
| 5 | **Polish / UX / onboarding** | Ergonomie, tutoriel, stabilisation avant testeurs externes | Un joueur non initié termine une partie sans aide extérieure | Fin de P2, avant l'ouverture de la Phase **Q (Web, 114→117)** si diffusion navigateur |

**Cas particulier EdTech** : une **Phase 3bis** (portabilité web/XR, contenu pédagogique) viendrait se greffer après la Phase 3, mais dépend des Phases **Q (Web)** et **R (WebXR, 115→120)** — non encore atteintes dans la roadmap actuelle. Elle est donc mécaniquement plus tardive que le chemin coop 3D.

## 3. Recommandation de séquencement

1. **Maintenant** : amener *Le Hameau des Braises* au seuil MVP (Phase 2) puis vertical slice (Phase 3) — c'est le chemin qui atteint un testing réel le plus vite, en s'appuyant sur ce qui est déjà livré (serveur autoritaire, protocole réseau, premier jeu jouable).
2. **En parallèle, sans bloquer** : cadrer la niche EdTech — cas d'usage cible, contraintes réglementaires/données, premiers contacts marché — pour être prêt à l'activer dès que les Phases Q et R seront atteintes.
3. **À ne pas démarrer trop tôt** : le travail WebXR (Phase R) — il est explicitement traité en dernier dans la roadmap par choix de priorité, et le lancer prématurément détournerait l'effort du chemin le plus court vers un produit testable.
