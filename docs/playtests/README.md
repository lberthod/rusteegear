# Playtests — résultats réels

Roadmap post-audit UX 2026-09-04, vague 3 : tout ce que les audits disent de
l'ergonomie reste un avis d'expert tant que personne d'autre que l'auteur n'a
tenu l'éditeur. Ce dossier reçoit **un fichier par session de test**, nommé
`AAAA-MM-JJ-<prenom-ou-role>.md`, rempli à partir de
[TEST_FEEDBACK_FORM.md](../TEST_FEEDBACK_FORM.md) pendant que le testeur
suit [TEST_SCENARIO.md](../TEST_SCENARIO.md).

## Protocole minimal (3 à 5 testeurs, dont un non-développeur)

1. Construire le `.dmg` (`./packaging/build_dmg.sh`) ou, pour la variante
   source, vérifier `./scripts/doctor.sh` sur la machine du testeur.
2. Ne pas aider : noter l'heure de chaque étape, ce que la personne cherche,
   ce qu'elle dit à voix haute. Règle du scénario : bloqué plus de 10 min =
   noter et passer.
3. Après la session, remplir le formulaire avec la personne (9 questions).
4. Copier le fichier ici, puis reprioriser `docs/roadmapauditUX4septembre.md`
   (vague 5) avec ce qui est ressorti — un point vu par deux testeurs passe
   devant tout ce qu'un audit a deviné.

## Ce que chaque fichier doit contenir

- Profil (métier, habitude des éditeurs 3D, plateforme, variante A/B).
- Tableau par étape : durée, réussi / aidé / abandonné, verbatim.
- Les trois moments où la personne a hésité le plus longtemps.
- Réponses aux 9 questions du formulaire.
- Version testée (commit ou tag).

Aucun résultat à ce jour : ce dossier est vide tant que la vague 3 n'a pas
eu lieu.
