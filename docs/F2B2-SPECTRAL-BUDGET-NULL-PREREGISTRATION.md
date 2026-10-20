# F2b-2 — Grilles 32/508, témoin nul et refus explicite Walsh-3 hors budget

**Préenregistrement 2026-10-20, avant implémentation et mesures F2b-2.** Suite de F2b-1 (SML #634, RemoteOps #623/#624). Les résultats F2b-1 demeurent figés et sont reproductibles par l'ancien drapeau `--f2b`; aucun changement de seuil post hoc.

## Hypothèse et limites

À 508 sources candidates, Walsh3 exhaustive exige plus de 21 millions de combinaisons structurelles, très au-dessus de la grille 128 du banc précédent. Il faut **s'abstenir d'exécuter Walsh3** sous plafond fixe, plutôt que masquer le coût et déclarer une sélection non mesurée. Les labels IID indépendants sont un contrôle négatif de *fausses promotions*. Le but n'est pas de prouver une accélération générale ou une découverte universelle de parités bruitées.

## Protocole de données, figé

Créer un mode opt-in `--f2b2 xor|mixed|null 0|40 11|23 32|508` dans le banc historique C12h, sans changer la commande sans arguments ni `--f2b` de F2b-1. `xor` et `mixed` ont bruit 0 ou 40 %, le témoin `null` n'accepte que 0 % de bruit en argument et remplace les cibles du générateur par un flux pseudo-aléatoire indépendant des entrées. Deux graines 11/23 dont les nouveaux sels de génération sont distincts des sels C12h/F2b-1.

20 essais **par hôte** : 2 fonctions ×2 bruits ×2 graines ×2 largeurs (16), plus 1 contrôle nul ×2 graines ×2 largeurs (4). N=32 et N=508 sont les grilles **d'indices de sources candidats**, non la taille de la FWHT. Pas de vérité de l'enseignant transmise au découvreur.

Conserver TRAIN_fit avec halo de 128 pas et 128×64 ou 512×64 bits utiles selon bruit, interne TRAIN validation 640×64, deux TRAIN_cal séparés de 640×64, puis DEV de 4096×64 construit **après freeze**. Pour null, la cible indépendante remplace tous les labels de **chaque étape** (fit, validation, cal1, cal2, DEV). Le bruit nul n'implique pas une tâche facile.

## Budget et routage préenregistrés

Plafond **1 000 000 triplets admissibles** pour l'étape W3 exhaustive. Si `n508` dépasse ce plafond, W3 est marqué `NOT_EVALUATED_BUDGET` et **n'est jamais exécuté** ; son réseau n'existe pas, son erreur de calibration pénalisée au maximum (pas promue), temps de recherche brut déclaré 0 avec champ de skip explicite. Le Hybrid W2+Sketch/MITM est exécuté normalement et admissible seulement sous le *même gate F2a* déjà figé en F2b-1. Ne pas remplacer le W3 ignoré par un score oracle, ni appeler son erreur `0`.

Pour `n32`, exécuter les deux moteurs réels et comparer sur les cibles identiques. Les mêmes limites CPU préexistantes, marge de 10 000 ppm et les deux partitions sont appliquées. Route 0=Hybrid, 1=W3, 2=Abstain. Si aucun réseau n'est qualifié : abstention avec zéro prédiction routée. Les comparateurs shadow DEV sont autorisés **seulement après freeze** et exclus de la durée du chemin choisi.

TSV `F2B_ROUTE_V2` : 29 champs = exactement 27 champs du V1, suivis de `n_sources` et `w3_not_evaluated_budget` (0/1). Cas `null` identifié séparément, 20 lignes uniques par hôte. Pas de seuil de réussite scientifique imposé dans la CI, seulement des invariants de ressources, exactitude des étiquettes, provenance et absence de rétroaction DEV.

## Critère de qualification et limites

PASS d'ingénierie si chaque hôte Dell/Thor exécute les 20 cas et produit la preuve que W3 à n508 n'a jamais été appelée, que null n'utilise aucun label de l'enseignant et qu'aucune route ne fonctionne sans modèle figé ; CI Rustfmt + Clippy + tests sans erreurs. Le choix effectif, les faux positifs et les abstentions doivent rester publiés, y compris négatifs.

STOP scientifique si le témoin nul est artificiellement qualifié et promu sur TEST sans signal, si le routeur choisit une méthode hors coût, si un n508 « gagnant W3 » est inventé, ou si les temps de calibration sont dissimulés.

Ce protocole élargit seulement deux dimensions. Les niveaux de bruit 20/30 %, les grammaires hors ordre3, source temporelle cachée, corpus linguistique externe admis, VRAM/RSS intégral et modèle 25B sont à traiter ultérieurement. SEC **G0 NOT_ADMITTED**, TEST 2024 **UNOPENED**. Rust natif uniquement, sans SDK NVIDIA.
