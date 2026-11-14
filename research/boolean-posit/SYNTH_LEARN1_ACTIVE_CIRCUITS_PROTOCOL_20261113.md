# SYNTH-LEARN1 — Apprentissage actif de circuits posit8 ADD/MUL

**Statut : prototype expérimental.** Ce jalon ajoute une découverte de circuit réellement pilotée par des **requêtes partielles à un professeur arithmétique** et distincte des synthétiseurs ROBDD exhaustifs. Il n'est pas une preuve d'apprentissage exact, de gain de vitesse ou de conformité normative à Posit 2022.

## Contrat préenregistré (avant mesure)

L'univers comporte `2^16 = 65 536` couples encodés par entrelacement des huit bits de chaque opérande. Pour chaque opération, la cible est notre oracle Rust d'arithmétique entière posit8 (et non une implémentation normative extérieure).

L'apprentissage est limité à **320 requêtes de professeur** par opération : 64 exemples initiaux, puis 8 itérations de 32 exemples. Chaque itération entraîne deux graphes de décision sur **les mêmes seuls exemples étiquetés**, avec politique opposée de départage des séparations binaires. L'algorithme examine uniquement leurs prédictions, **sans consulter le professeur**, pour sélectionner jusqu'à 24 points de désaccord et compléter par exploration déterministe. Le professeur étiquette alors ces 32 points ; les erreurs du premier modèle comptent comme contre-exemples. Le modèle est réentraîné à l'itération suivante.

Le mécanisme de découverte est un arbre de décision **à branchements sur les 16 variables binaires**, partageant les constantes et les sous-circuits identiques au moyen d'un DAG canonique local. La séparation de chaque branche maximise une concentration Gini entière déterministe des étiquettes observées. Chaque branche correspond logiquement à un MUX booléen ; aucune table arithmétique de 65 536 sorties n'est construite pour apprendre. Il s'agit d'un **premier espace d'hypothèses borné**, pas encore d'une recherche ouverte sur les portes AND/XOR ni sur des topologies arithmétiques nouvelles.

La partition de validation est gelée par `is_holdout(u16)`, mélange déterministe non affine, avant l'entraînement. Aucune de ses entrées n'est interrogée pendant l'apprentissage. Un audit final du professeur sur les 65 536 couples compte les erreurs holdout et totales, **après le gel du dernier modèle** ; ses étiquettes ne sont jamais réinjectées. Le nombre de requêtes d'apprentissage reste toujours 320, indépendamment du nombre d'évaluations d'audit. Il ne faut pas présenter les erreurs d'audit comme des requêtes utilisées pour apprendre.

## Génération native

Le même DAG appris est abaissé directement en assembleur GNU x86-64 System V (arguments octets `dil`, `sil`, `test`, `jnz`, `jmp`, retour `eax`). C'est un circuit à sélection conditionnelle exécuté sous forme de branchements machine, **pas encore** un noyau sans branchement XOR/AND ni un calcul vectoriel. Le workflow assemble les deux sources GAS, produit les objets ELF, lie un test Rust FFI, puis compare **sur les 65 536 couples** chaque résultat machine au résultat du modèle appris. Les 320 exemples étiquetés doivent également être respectés. Cette comparaison native ne prouve **pas** que les opérations ADD/MUL soient exactes sur tous les couples : l'audit professeur en publie séparément le taux d'erreur.

## Règles de décision et étapes suivantes

- Un modèle ne peut être déclaré exact que si `full_errors=0` **après** audit exhaustif ; l'exactitude sur 320 exemples ne suffit pas.
- Les résultats obligatoires comprennent : budget et requêtes consommées, désaccords, contre-exemples, erreurs d'entraînement, erreurs réservées, erreurs totales, nombre de nœuds, portes MUX, profondeur, SHA et preuves d'exécution native.
- Des échecs de généralisation sont des résultats négatifs utiles. Ils devront orienter l'extension vers la découverte de sous-circuits XOR/AND/NOT partagés, l'induction de règles de propagation de retenue et le guidage par contre-exemples, **sans** autoriser de revendication de qualité industrielle.
- Un second oracle arithmétique du dépôt existe pour les tests de cohérence, mais les deux implémentations ne constituent pas une certification normative extérieure.
- Pas d'exécution Dell/Thor/Mac, pas de service ni SDK GPU ; toute validation physique future doit emprunter exclusivement Memorithm/RemoteOps. Aucune intégration à l'inférence de SML-GENIUS n'est autorisée par ce prototype.

Implémentation : `research/boolean-posit/posit8_active_learn.rs`. Validation machine : `research/boolean-posit/posit8_active_native_test.rs`.
