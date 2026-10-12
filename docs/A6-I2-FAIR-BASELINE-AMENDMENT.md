# A6-I2 — Amendement transparent après le premier run : comparateur bayésien uniforme actualisé

**Ajout du 12 octobre 2026, APRÈS le premier run scientifique réussi, AVANT tout changement de code pour ce diagnostic.** Le préenregistrement initial `A6-I2-FOUR-HYPOTHESIS-PASSIVE-PREREGISTRATION.md` reste intact. Premier run réussi : GitHub Actions `passive-committee` job `114811920996`, SHA `2781d0b56c6df92fa148f91744fe3d9f8f610ca7`, 504 ajustements et tests +42 négatifs. Les premiers résultats sont conservés.

## Observation motivant l'ajout

Le premier banc publie `brier_uniform` comme **mélange uniforme STATIQUE**, qui ne s'actualise pas avec les étiquettes TRAIN. Ce choix est une référence valide à ignorance constante, mais ne représente pas un **second classificateur bayésien avec prior uniforme mis à jour**. Ainsi, comparer le prior biaisé **actualisé** au mélange uniforme **non actualisé** en régime PARTIAL/FULL attribuerait abusivement l'amélioration au prior plutôt qu'à l'apprentissage. La comparaison est équitable dans BLIND, où les quatre candidats restent observablement identiques, mais nécessite un contrôle supplémentaire ailleurs.

## Modification strictement instrumentale fixée ici

Sur les **mêmes comités, les mêmes 504 observations TRAIN, les mêmes 2048 TEST, les mêmes candidats, mêmes graines et mêmes interventions (aucune)**, ajouter une métrique de **posterior uniforme *actualisé***. Si `p_i^{bias}` est la probabilité finale du comité avec prior `[4,2,2,1]/9`, alors le posterior sous prior uniforme est calculé exactement par changement de mesure :

`p_i^{uniform,updated} = (p_i^{bias}/w_i) / Σ_j(p_j^{bias}/w_j)`, avec `w=[4,2,2,1]`.

La vraisemblance TRAIN est **identique**, la repondération analytique ne consulte jamais de label TEST ou vérité du professeur. Évaluer `p_true`, puis `(p_true−y)^2` sur les mêmes sorties TEST. Conserver séparément `brier_uniform_static` (le champ original `brier_uniform`) et `brier_uniform_updated`. N'ajouter **aucune règle de sélection, calibration ou abstention** basée sur ce diagnostic ; ne modifier ni les 504 politiques gelées, ni les seuils H1-H5, ni l'alarme de modèle absent.

Contrôle natif additionnel : sous BLIND, le posterior uniforme actualisé doit rester **[1/4,1/4,1/4,1/4]** ; sous FULL, il doit converger vers le professeur dans les mêmes strates que le posterior biaisé. Cela n'ajoute aucune nouvelle hypothèse de succès : un résultat non favorable au prior biaisé doit être publié.

**Règle de documentation :** rapporter explicitement les deux baselines, conserver les résultats du premier run comme baseline historique, publier toute variation; aucune optimisation scientifique post-TEST n'est autorisée.
