# HYB-ALT — voie indépendante : tête résiduelle ternaire à lecture seule sur SBG

**Gel : 23 octobre 2026, avant les expériences H-ALT.**
**Branche indépendante** de l'option B « bus de bits » implémentée en parallèle par Claude. Ne pas modifier ses modules `ternary_bitplane`, `Pvp6Bank`, `SbgAnfExpert`, `MOR`, `RTLR-V6`, ni ses contrats de routage.

## Hypothèse concurrente

**H-ALT** : traiter le calcul neuronal comme une **tête résiduelle facultative en lecture seule**, sur un instantané `SemanticState<S>` figé à la frontière entre traitement SBG et scoring. La tête n'a **aucune écriture dans SBG**, aucun cache récurrent, aucune attribution de page MOR ; elle peut être activée/désactivée à coût et entrées rigoureusement identiques. Ce contrôle évite de confondre amélioration du readout neuronal et réécriture de la géométrie/mémoire.

La voie Claude B teste le bus d'experts partagés au sein de MOR. La voie alternative teste l'intérêt **intrinsèque** de la correction neuronale avant toute admission par page. Les deux chemins devront ensuite être comparés sur les mêmes entrées, labels, budgets et SHA.

## H-ALT-0 : noyau exact, sans qualité revendiquée

- Dimensions : `S` logique validé par `SemanticState<S>`, `F ∈ [1,4096]` caractéristiques SBG choisies explicitement parmi `{Value,Meaning,Ambiguity,Routing}×[0,S)`, sans duplicata, `O∈[1,256]` sorties.
- Poids **ternaires** `w∈{-1,0,1}`, deux plans `positive,negative` disjoints par sortie, avec padding strictement nul. `bias_i32` avec bornes de construction garantissant l'absence de dépassement `i32` lors de l'accumulation maximale. La lecture de bits suit exactement l'ordre des coordonnées déclaré.
- Noyau : `score = popcount(inputs & positive) - popcount(inputs & negative) + bias`, avec référence scalaire indépendante par itération sur `w_j×bit_j`. **Même sortie `i32` sur x86_64 et aarch64**, sous mêmes poids, entrées et contract de code.
- Interface : `TernaryResidualReadout<S>::score(&SemanticState<S>) -> Result<Vec<i32>,..>`, sans mutation des bits ; `add_to_baseline(baseline_i32,scale)` avec overflow contrôlé et aucune saturation silencieuse.
- Baseline nulle : `scale=0` restitue **exactement** les logits entiers de référence. On ne modifie aucune route MOR ni aucun état.
- Oracle de test : toutes petites tables 1..16 bits ; largeurs de champs non alignées sur 64 (65/127/129) ; mots extrêmes, overlap/padding rejetés, duplication de caractéristiques, changement de champ, `i32` overflow, invariance snapshot, 256 classes, entrée vide et dimensions incorrectes.
- Ne pas confondre bitplanes de poids sur **une projection de champs** avec bitplanes de requêtes évaluées en GB-6. Les gains GB-9 pour une banque ne transfèrent pas à ce readout sans mesure distincte.

## H-ALT-1 : preuve de valeur en prédiction, séparée de H-ALT-0

- Conserver deux bras : **baseline booléenne gelée** et **baseline + tête résiduelle**, mêmes logits de base et mêmes positions temporelles, aucun label DEV/TEST utilisé pour entraîner ou router.
- Entraîner/exporter les poids maîtres f32 **uniquement sur TRAIN** ; exporter une révision immuable de masques ternaires avec seuil, version, provenance SHA. Ne pas déduire que l'inférence entière rend l'entraînement bit-exact.
- Utiliser `eng1c_dev::DevCorpus` et son manifeste/hashes, ou un corpus équivalent vérifié. Sans corpus, logits de base, checkpoints/poids entraînés et alignement token, ne produire **aucune** valeur de BPC d'un modèle.
- Scorer le BPC DEV en f64 **hors noyau entier**, avec softmax log-sum-exp stable, vocabulaire byte 256 et exactement les mêmes positions, sans annoncer identité inter-machines du BPC flottant. Publier plusieurs seeds, positions, variance, latence et RSS, ablations `scale=0` et permutation des features.
- Admission go/no-go : improvement DEV robuste aux seeds et supérieur au bruit, coût CPU ≤1,5× du moteur booléen (sur définition complète incl. projection, mémoire et chargement), puis seulement validation TEST fermée. Un échec ne bloque pas l'option Claude, mais écarte cette tête résiduelle particulière.

## Mesures et modèle

H-ALT-0 : seulement `fmt/check/clippy/tests` + futur RemoteOps Dell/Thor SHA final, checksum entier exact. H-ALT-1 : apprentissage TRAIN séparé et latence native sur les deux processeurs. Aucun modèle SML-25B, Transformer, GPU ou énergie n'est qualifié par H-ALT-0.

## Réconciliation des deux solutions

À la comparaison : figer identités du corpus, tokenizer, train/dev/test, seeds, compteurs, poids logiques budget et sorties oracle ; rapporter A (sidecar), B (experts MOR), 0 (booléen seul). Les échecs doivent être conservés. Si la tête résiduelle n'apporte rien, ne pas lui attribuer une « innovation » sans preuve. Tous les artefacts portent le préfixe `hyb_alt_` et sont entièrement optionnels.
