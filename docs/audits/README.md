# `docs/audits/`

Historique de développement (Sprint 103a-3, `ROADMAP_SPRINTS.md`) : ce dossier
reçoit la mémoire « comment on en est arrivé là » que les commentaires de code
portaient auparavant — bugs réels trouvés en testant, essais qui n'ont pas
marché, mesures ponctuelles, attribution par sprint.

**Règle de partage entre code et ici** :
- Dans le code (`///`/`//`) : uniquement ce qui aide à comprendre ou modifier le
  comportement *actuel* sans casser un invariant non évident — une contrainte
  du moteur/de la lib, un choix de design qui a une vraie alternative
  tentante-mais-fausse, un piège à ne pas réintroduire. Pas de numéro de
  sprint, pas de date, pas de « trouvé en testant » narratif.
- Ici : le récit — quel sprint a ajouté quoi, quel bug réel a motivé un
  correctif, ce qui a été essayé et écarté. Utile pour comprendre l'évolution
  du projet, inutile pour le lire/modifier au quotidien.

Un fichier par module source (`docs/audits/<nom_du_module>.md`), rempli au fil
de l'eau — pas une reconstitution rétroactive de tout l'historique en une
fois (voir `ROADMAP_SPRINTS.md` pour le journal sprint par sprint, qui reste
la source de vérité chronologique).
