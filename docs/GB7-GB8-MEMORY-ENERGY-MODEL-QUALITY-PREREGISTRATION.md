# GB-7 et GB-8 — portes de validation des résultats encore non démontrés

**Préenregistrement : 23 octobre 2026, avant toute campagne physique GB-7 ou score modèle GB-8.** Ces deux questions sont méthodologiquement indépendantes ; une réussite de GB-6 sur le CPU ne les qualifie pas.

## Évidence déjà qualifiée : GB-6

Source scientifique GB-6 `8f367ff163db98a47695a9025e35073f1276af8a`, Rust 1.89.0 ; RemoteOps [workflow #39977860338](https://github.com/Memorithm/RemoteOps/actions/runs/39977860338), jobs Dell x86_64 et Thor aarch64 réussis. Chaque hôte a produit 432 cellules de parité et 3888 mesures. Le bras complet « une transposition, R experts » gagne sur 56/144 cellules Dell et 54/144 Thor. Pour la famille π, le gain commence à R=4 pour 8 secteurs et R=8 pour 16 ; à 64 secteurs R≤32, **aucun gain** sur les deux hôtes. Ce sont des résultats de circuits synthétiques à données réutilisées, pas des gains modèle ou énergétiques.

## GB-7A : coût mémoire réel

Mesurer sur les mêmes hôtes et la même source GB-6, mais dans des processus **séparés par bras** : lecture LUT, bitplanes prétransposés, bitplanes avec transposition. Les sorties doivent toujours correspondre exactement. Compiler avec les mêmes flags. Publier RSS max du **processus enfant** (Linux `getrusage(RUSAGE_CHILDREN).ru_maxrss` en KiB ou `/usr/bin/time -v` documenté), en plus des allocations Rust si un instrument fiable est ajouté. Enregistrer la taille des banques d'experts, l'empreinte des coefficients/masques et des plans d'entrée ; différencier **allocation de données**, **RSS de tout le processus** et **mémoire du modèle**. Une seule exécution de GB-6 où les trois bras résident dans un même processus ne permet **pas** d'attribuer la mémoire à un bras.

Répéter 9 fois par bras (processus froids séparés), avec les mêmes tailles `M=8,16,32,64`, `R=1,4,8,16,32`, `lanes=64`. Publier min/median/max et taille binaire. Ne pas conclure sur les 25 milliards de paramètres à partir d'une différence de quelques KiB de circuit.

## GB-7B : disponibilité et validité de la mesure énergétique

D'abord effectuer une **découverte passive, en lecture seule** des domaines CPU/SoC accessibles (RAPL `energy_uj`, éventuels instruments exposant énergie joules, capteurs `power*_input` et leurs unités, méthode d'échantillonnage). Aucun outil NVIDIA/CUDA n'est requis. L'absence d'un compteur de joules accessible doit produire `UNAVAILABLE`, jamais une estimation à partir du TDP ou une attribution à partir de puissance instantanée.

Si un compteur exact est disponible : relever début, fin, `max_energy_range_uj` et granularité pour corriger les débordements ; vérifier qu'il expose **énergie**, pas puissance ; mesurer charge inactive et témoin LUT à durée comparable ; documenter le domaine (package vs process), température/fréquence, autres workloads, fenêtre de mesure. L'énergie de package inclut les autres processus : ne pas annoncer des « J/requête modèle » sans isolation/admission et témoin de fond appropriés. Sinon GB-7B reste **non mesurée**.

## GB-8A : représentations matérielles SIMD/NEON et GPU portable

Les mots `u64` ne démontrent **pas** une exécution NEON, AVX2, AVX-512 ou GPU. Comparer la version Rust scalaire portable à un prototype vectorisé explicitement contrôlé avec le code machine/disassembly et les jeux de tests exacts ; exiger les mêmes entrées et quotas mémoire. Avant un gain annoncé, publier CPU flags, instructions effectivement utilisées et leurs coûts de conversion, ainsi que l'absence éventuelle d'optimisation automatique. Pas de dépendance CUDA/TensorRT/stack propriétaire dans la voie scientifique par défaut ; toute voie WGPU doit être isolée et opt-in.

## GB-8B : qualité prédictive SML et alternative à l'attention

L'essai requis ne se réduit pas à produire des réponses correctes par un oracle symbolique. Il faut **exécuter un candidat réellement entraîné** SML, une ablation sans GB-6, le routeur/PVP6-SBG-MOR existant et un témoin Transformer/attention à capacité/compute/token et données comparables. Pour chaque bras : temps et budget d'entraînement, nombre de paramètres, tokens, énergie/mémoire si observables, configuration de l'optimiseur, checkpoints, seed, protocole de sélection et SHA source.

Corpus de démarrage : six familles `sml_core::tasks` (delayed/overwrite/keyed/copy/conjunction/distractor). Geler les groupes d'identifiants d'instance **avant entraînement** : `train case_id=0..999`, `dev=10000..10199`, `test=20000..20199`, `ood=30000..30199`, pour chacune des six familles. Régime IID : délai/distractors `case_id%16`, copie `1+case_id%8`. Régime OOD : 64 à 127 distracteurs/délai (selon index modulo 64), copie de longueur 32 à 95 ; les valeurs cibles restent construites par la fixture déterministe d'origine. Chaque tâche émet séquence d'événements et réponse gold avec un schéma stable et un hash. **Tous ces événements sont synthétiques**, pas une représentation de langue naturelle.

Les cas ne doivent pas se répéter entre splits (contrôle par identifiant source, famille et condensat du contenu). Éviter une sélection de checkpoint sur `test`. Préciser que les générateurs déterministes connus et la faible diversité des fixtures limitent fortement les revendications de généralisation ; ajouter ensuite des splits par templates réellement nouveaux et des tâches de langage indépendantes.

Métriques : exact match par famille et par split, écart dev/test et OOD, calibration et taux d'échec de routage, latence CPU/ARM/GPU selon runtime et coût mémoire ; intervalles par bootstrap par instance/split, seeds multiples. Le résultat doit inclure témoins négatifs ; un gain sur un domaine limité ne qualifie pas un modèle 25B opérationnel.

## GB-8C : remplacer l'attention

Aucun gain GB-1..GB-7 ne démontre que l'ANF peut remplacer l'attention dans un modèle de langage. Il faut une étude matched compute, contexte long, mémoire associative, rappel, copie, extrapolation de longueur, perplexité et débit de génération sur corpus autorisés, avec contrôles RNN/attention et ablations de mécanismes de mémoire. Pas de revendication `O(N)` ou VRAM constante à moins de mesurer l'end-to-end (encodeur, état récurrent, batch, cache, transfert).

## Gates et décision

1. **Correction** : parité native sur chaque entrée/secteur/banque, CI verte SHA exact et absence de seuil réajusté après lecture de `test`.
2. **Mesure physique** : source et configuration gelées, capteur qualifié ou `UNAVAILABLE`, preuves brutes et contrôle de fond, hosts non agrégés.
3. **Qualité du modèle** : candidats exécutés, témoin entraîné à budget équivalent, résultats OOD/test non utilisés pour choisir la politique.
4. **Promotion** : une étude favorable de microbench GB-6 n'active aucune optimisation productive ; une hypothèse de remplacement de l'attention demeure ouverte.
