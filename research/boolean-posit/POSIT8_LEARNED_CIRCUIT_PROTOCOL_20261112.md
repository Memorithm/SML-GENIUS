# BPB-P8-SYNTH0 — Apprentissage de microcircuits posit8 et compilation native

**Statut : recherche expérimentale, sans activation dans le modèle.** Programme autonome Memorithm : découvrir, qualifier et compiler des circuits booléens arithmétiques natifs. L'implémentation Rust, les oracles, les données synthétiques et les tests sont élaborés au sein du projet, sans importation de code, poids, modèles, tables ou jeux de données provenant d'autres projets.

## Périmètre scientifique gelé avant les résultats

- Format de référence : **posit8 conforme à la représentation Posit Standard 2022** (8 bits, exposant de 2 bits, régime de base 16), zero `0x00`, NaR `0x80`, négatif encodé en complément à deux. Valeurs finies en unités exactes de `2^-24`; `minpos=2^-24`, `maxpos=2^24`. La conformité complète à la norme nécessiterait une validation croisée indépendante, non couverte ici.
- L'addition et la multiplication de référence s'effectuent sur les valeurs entières à échelle fixe `i128`, sans multiplication flottante : les **65 536 paires** d'octets couvrent exactement l'espace de deux entrées posit8. Arrondi vers le plus proche, ex aequo au code pair, saturation non nulle aux extrêmes et propagation NaR.
- Les deux oracles arithmétiques sont **écrits manuellement**, pas appris, et leurs tests exhaustifs vérifient notamment la commutativité, la symétrie des signes et les valeurs aux limites. Il ne s'agit **pas d'une preuve indépendante de conformité aux règles Posit 2022** : une seconde bibliothèque ou un second décodeur indépendant reste nécessaire.
- Premier circuit découvert : **négation de posit8**, une opération arithmétique unaire exacte sur l'encodage, y compris zero et NaR. Pour borner l'espace de recherche, l'enseignant propose des signaux de préfixe OR des bits moins significatifs ; l'algorithme **apprend la composition exacte de chaque sous-fonction** dans la bibliothèque AND/OR/XOR par recherche énumérative de portes. C'est une synthèse supervisée structurée, **pas encore une découverte libre d'additionneur ou multiplicateur**, et ce n'est pas un réseau neuronal différentiable.
- Partition immuable `TRAIN = {x | x % 8 != 5 et != 6}` : **192** octets ; **64** octets restants réservés à validation indépendante du choix des portes. Un apprentissage qui échoue sur les 64 échantillons TEST est refusé, sans retoucher les choix. L'épreuve finale compare exhaustivement **256** entrées à l'oracle arithmétique.
- Recherche déterministe, au plus **32 portes** après huit entrées, priorité à la faible profondeur, aucun accès à des programmes, chemins, shell ou modèles externes. La sortie est un Rust sans boucle dans le noyau logique, compilé par `rustc -O` vers de vraies **instructions machine** (artefact assembleur conservé). LLVM est autorisé à remplacer un réseau de portes par `neg` s'il reconnaît l'identité ; nous ne prétendons pas qu'il produira nécessairement des `xor/and/or` distincts.

## Mesures d'acceptation

1. Chaque codage posit8 se décode correctement selon l'algorithme de référence, les extrêmes et l'inversion complément à deux sont contrôlés.
2. Tous les couples posit8 ADD et MUL sont examinés (**65 536 chacun**), y compris NaR et sous/surdépassement, sous les invariants déclarés.
3. Le microcircuit obtenu a zéro erreur sur les 192 TRAIN, zéro erreur sur les 64 TEST, zéro erreur sur les 256 entrées exhaustives. Rapporter le nombre exact de portes et la profondeur constatée.
4. Le code Rust **généré** est compilé en assembleur natif, conserve le symbole `learned_posit8_neg`, puis un binaire compilé distinct revalide les **256** entrées ; archiver Rust généré, assembleur, logs et SHA du commit.
5. Aucun gain de vitesse, de surface silicium, de consommation d'énergie, de précision de réseau, de performance de modèle SML, ou d'originalité n'est déduit de la découverte de ce microcircuit.

## Étape suivante (distincte, non réalisée par SYNTH0)

**SYNTH1** : apprentissage libre d'un réseau combinatoire pour `posit8_add(a,b)` et `posit8_mul(a,b)`, avec le minimum de supervision architecturale, budget de calcul/de profondeur/périmètre, validation de toutes les 65 536 paires. Les réseaux peuvent contenir decode-régime, alignement, arrondi et saturation mais leur recherche est interdite d'utiliser l'ensemble TEST pour la sélection. Confronter les circuits à des additionneurs/concepteurs manuels et des implémentations standard, puis profiler sur CPU hébergé et enfin sur machines admises **exclusivement via Memorithm/RemoteOps**.

## Indépendance du développement et provenance

Les circuits, les encodages, les algorithmes de recherche, les oracles, les jeux d'essais et les procédures de compilation sont conçus et implémentés au sein de Memorithm. Aucun extrait de code, poids entraîné, table de correspondance, jeu de données ou test propriétaire externe ne doit être copié. Toute réutilisation future d'un composant tiers exige une revue de provenance et de licence distincte, ainsi qu'une qualification sur les oracles internes.

Documentation normative : [Posit Standard, Posit Working Group (2022)](https://posithub.org/docs/posit_standard-2.pdf). Les programmes restent séparés de l'optimisation propriétaire BPB-3-A8 (ce dernier est un codec à échelle partagée, **pas** un posit scalaire 8 bits).
