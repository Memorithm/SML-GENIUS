# GB-11 P1 — banques à adresse booléenne apprise sur le corpus S5h (préenregistrement)

Gelé le 24 octobre 2026, avant tout run sur S5h. Le propriétaire a approuvé P1 le 24 octobre 2026, en même temps que H-1 sur les mêmes traces. Ce protocole suit P0 et P0b (`docs/GB11-RELAXED-SPARSE-ANF-LEARNING-PREREGISTRATION.md`, `docs/RESULTS-GB11-RELAXED-SPARSE-ANF-LEARNING-P0.md`). Code : `crates/sml-core/src/gb11_p1_stream.rs` et le binaire `sml-gb11-p1-s5h`.

## Question

P0b a montré, sur données synthétiques, que le câblage discret (quels bits forment l'adresse d'une banque) s'apprend par recherche exacte avec réajustement des experts en forme close, et non par relaxation. P1 pose la même question sur du texte réel. Une recherche gloutonne exacte du câblage, avec des experts en forme close (des compteurs), donne-t-elle un prédicteur d'octet suivant compétitif avec le témoin à gradient `--stream-50m-msb`, à mémoire égale, sur le même DEV ?

## Témoin (déjà mesuré, non relancé)

Il s'agit du run RemoteOps `sml50m-s5h-pes2o-mix-train-dell.yml` au commit `c542d63d74693c0968031dc8bdec53df565d75cb` (job 119723848038, 22 octobre 2026). Ses poids sont sur le Dell, dans `~/sml-artifacts/sml50m-s5h-pes2o-mix-c542d63…`.

| Grandeur | Valeur |
|---|---:|
| manifeste SHA-256 | `966eb65f37c58488f8b449eabd7b7594fc5a056ca10db98df009adc789b8a09e` |
| TRAIN | 48 981 fichiers, 1 206 546 716 octets, une passe |
| DEV | 9 512 fichiers, 60 256 970 octets, 60 237 946 événements |
| paramètres appris (`learned_unique`) | 15 068 983 sur 50 000 896 emplacements |
| BPC DEV f32 (`master.f32`, 200 003 584 octets) | 3,655162232 |
| BPC DEV q4 (`weights.q4`, 25 000 448 octets) | 3,785251745 |

Le témoin prédit l'octet `t` à partir des deux octets précédents seulement, avec 17 caractéristiques linéaires hachées par nœud de l'arbre MSB. Il compte un événement par position `2..len` de chaque fichier empaqueté.

## Données

- Le staging S5h est rejoué à l'identique (S1b, S4e, tranche peS2o de 1 GiB revue par l'opérateur, audit S4d `PASS_NARROW`). Le manifeste produit doit avoir le SHA-256 `966eb65f…`, sinon le run s'arrête avant tout entraînement. Le binaire reçoit ce SHA attendu en argument et le vérifie aussi.
- Le manifeste n'admet que des lignes TRAIN suivies de lignes DEV. **TEST n'est jamais lu.** Chaque document est revérifié (taille, SHA-256, UTF-8) à chaque lecture.
- Les événements sont ceux du témoin : positions `2..len` de chaque fichier, avec un contexte remis à zéro au début de chaque fichier.
- Le texte, les poids et les traces restent sur le Dell. Seules les métriques paraissent dans le journal du job.

## Modèle

- **Bits d'entrée** (80 candidats) : les 64 bits des 8 octets précédents, c1..c8, et un registre de mot `W` de 16 bits. Pour une lettre ASCII (casse repliée) ou un octet ≥ 0x80, on fait `W ← rotl3(W) ⊕ octet` ; tout autre octet remet `W` à zéro. Cette mise à jour est GF(2)-linéaire, à prédicat de lettre près : c'est un réseau de XOR.
- **Banque** : une adresse est un sous-ensemble de ces bits, choisi par masque (une ANF de degré 1 par bit d'adresse, c'est-à-dire un câblage). Chaque banque a sa propre table hachée, composée de blocs de 32 octets. Un bloc contient une étiquette de 8 bits et les 15 nœuds d'un quartet de l'arbre MSB. Le premier quartet est adressé par le contexte seul ; le second par le contexte et le quartet haut de la cible.
- **Expert** : chaque nœud porte deux compteurs de 8 bits (n0, n1). C'est le réajustement en forme close de P0b, étendu à des comptes. Sa décision dure, la majorité, est la porte booléenne de P0b. En apprentissage, une étiquette différente réinitialise le bloc ; en évaluation gelée, elle se lit comme « jamais vu ».
- **Probabilités entières** :
  - `st = round(256 · ln((n1 + 0,4)/(n0 + 0,4)))`, saturé à ±2047 ;
  - poids de confiance `w = round(64 · n/(n + 1,5))` ;
  - `P(1) = clamp(round(65536/(1 + e^(−st/256))), 1, 65535)`, lu dans une table.
  Les tables sont construites une fois ; la prédiction n'utilise ensuite que des additions, des multiplications et des divisions entières, et des lectures de table.
- **Mélange fixe, sans poids appris**, choisi parmi trois règles :
  - `average` : Σ w·st / Σ w ;
  - `half_sum` : Σ w·st / 128 ;
  - `top` : la banque d'indice le plus élevé ayant au moins deux observations (repli sur la banque 0).
- **Mise à jour**, choisie parmi deux règles :
  - `stationary` : incrément, et division par deux des deux comptes à saturation ;
  - `adaptive` : en plus, le compte opposé supérieur à 2 devient n/2 + 1.

## Apprentissage (toute sélection sur TRAIN, jamais sur DEV)

1. **Préfixe de sélection.** Les premiers fichiers TRAIN, dans l'ordre du manifeste, jusqu'à au moins 16 MiB, forment le *fit*. Les suivants, jusqu'à au moins 2 MiB, forment la *holdout*. Ces fichiers sont aussi vus ensuite par la passe complète, comme tout TRAIN.
2. **Recherche du câblage** (bras `search`), sur 6 banques :
   - la banque 0 a l'adresse vide (ordre 0) ;
   - pour chaque banque suivante, on ajoute un à un le bit candidat qui réduit le plus la longueur de code sur la holdout du mélange `average`/`stationary` des banques déjà retenues plus la candidate ;
   - on s'arrête quand le gain tombe sous 0,001 bit par octet ou que l'adresse atteint 40 bits ;
   - à égalité, on prend le plus petit indice de bit ;
   - une banque à masque vide met fin à la recherche.
   La recherche est exacte : chaque candidat est réentraîné sur le fit. Ses comptes sont indépendants des autres banques, si bien que la contribution évaluée est exactement celle de la banque dans le modèle complet.
3. **Configuration.** Pour chaque bras à 25 Mo, les 6 couples (mélange, mise à jour) sont entraînés sur le fit et évalués sur la holdout. On garde le meilleur ; à égalité, l'ordre de `CONFIGS` décide. Le bras `search_f32` reprend les masques et la configuration de `search_q4`.
4. **Passe complète.** Une seule passe sur tout TRAIN, dans l'ordre du manifeste, en mode préquentiel : chaque décision est prédite avant la mise à jour. Le budget de tokens est donc égal à celui du témoin.
5. **DEV gelé.** DEV est évalué une fois, sans aucune mise à jour. Le SHA-256 des tables doit être identique avant et après DEV.

## Bras

| Bras | Masques | Table | Rôle |
|---|---|---:|---|
| `search_q4` | recherche gloutonne, 6 banques au plus | 25 000 448 octets | **primaire** ; seul à écrire les traces |
| `bytes_q4` | n-grammes d'octets d'ordres 0, 1, 2, 3, 4 et 6 | 25 000 448 octets | câblage fait à la main |
| `order2_q4` | ordres 0, 1 et 2 (le contexte du témoin) | 25 000 448 octets | contrôle du contexte |
| `search_f32` | masques de `search_q4` | 200 003 584 octets | échelle, à mémoire égale au f32 du témoin |

« Paramètres appris » désigne les nœuds dont au moins un compte est non nul ; ce nombre est rapporté pour chaque bras. La comparaison principale se fait à octets de table égaux. Le nombre de paramètres n'y sert pas de critère, car un compteur de 16 bits ne vaut pas un poids f32.

## Critères (fixés avant le run)

- **C1 — intégrité** (tous les bras), sinon le run est nul :
  - manifeste `966eb65f…` ;
  - 60 237 946 événements DEV ;
  - tables inchangées par DEV ;
  - taille de table sous le budget ;
  - BPC DEV recalculé depuis `trace-dev.bin` égal à la métrique, à 10⁻⁶ près.
- **C2 — primaire.** `dev_bpc(search_q4) < 3,785251745`, le BPC du témoin q4 à octets égaux. Forme forte : `< 3,655162232`, le témoin f32, qui occupe 8 fois plus d'octets.
- **C3 — câblage appris.** Si `dev_bpc(search_q4) ≤ dev_bpc(bytes_q4) − 0,02`, le câblage appris bat le câblage fait à la main. Un écart de ±0,02 vaut équivalence. Au-delà de +0,02, la recherche est moins bonne que les n-grammes.
- **C4 — décomposition.** On compare `order2_q4` au témoin q4, à contexte égal (2 octets). Cela isole l'apport des experts en forme close de celui du contexte plus long.
- **C5 — échelle**, descriptif : `search_f32` contre `search_q4`.

Il n'y a ni graine ni intervalle de confiance : tout est déterministe et DEV compte 60 M d'événements. Les documents DEV ne sont pas indépendants, donc les écarts inférieurs à 0,02 BPC ne sont pas interprétés.

## Traces pour H-1 (format accepté par le fil H-1)

Le bras `search_q4` écrit deux fichiers d'enregistrements de 40 octets, little-endian :

| Champ | Type |
|---|---|
| doc_index | u32, rang 0-basé dans le manifeste |
| position | u32, octet prédit dans le fichier |
| split | u8 : 0 = TRAIN préquentiel, 1 = DEV gelé |
| target | u8 |
| W | u16, avant la cible |
| contexte c1..c8 | u64, c1 dans l'octet bas, 0 avant le début du fichier |
| logits | 8 × i16 : `st` de chaque décision du chemin MSB vrai, d = 0 étant le bit 7 |
| confiance | u32 : 4 bits par banque, min(15, n0 + n1) à la décision 0 |

- `trace-dev.bin` couvre toutes les positions DEV, avec le modèle gelé.
- `trace-train.bin` couvre les fichiers TRAIN avec `doc_index % 20 == 0`. Leurs logits sont préquentiels, calculés avant la mise à jour : H-1 n'apprend donc jamais sur des logits in-sample.
- `trace.json` donne le layout, l'échelle (ln-odds × 256), la saturation (±2047 avant la sigmoïde), la sigmoïde exacte en Q16 et le coût, les SHA-256 des traces, des poids (`weights-search_q4.bin`) et du manifeste, ainsi que le commit source.
- `gb11_p1_stream::read_trace` relit les traces et recalcule le BPC.
- Chaque fichier fait environ 2,4 Go. Le workflow exige au moins 16 Go libres avant le run.

## Exécution

Le workflow RemoteOps `sml-gb11-p1-s5h-dell.yml` tourne sur le Dell T430 :
- même garde d'acteur et même staging que S5h ;
- `ulimit -v` de 3 GiB, `nice`, délai maximal de 10 h ;
- sortie dans `~/sml-artifacts/sml-gb11-p1-s5h-<sha>`, sans écrasement.

Le journal publie `search.tsv`, `configs.tsv` et `metrics.tsv`. La source SML est épinglée par SHA.

## Ce que P1 ne montre pas

- **Le gain probable vient surtout du contexte.** Le témoin ne voit que 2 octets ; les banques P1 en voient jusqu'à 8. C4 et C3 servent à séparer « contexte plus long », « experts en forme close » et « câblage appris ». Une victoire en C2 seule n'est pas une victoire de la méthode.
- **La sélection sur 16 MiB favorise les adresses courtes**, car les contextes longs y sont rares mais utiles sur 1,2 Go. C3 mesure ce biais ; il n'est pas corrigé.
- **Les règles `adaptive` sont désavantagées par l'évaluation gelée.** Un instantané récent ne vaut pas une moyenne. La sélection sur la holdout tranche, sans retouche.
- **Aucun temps natif, aucune énergie, aucune promotion.** Le bénéfice matériel ici est structurel : adresses câblées, registre en XOR, lectures de table et arithmétique entière, sans flottant à l'inférence. La mesure native reste l'objet de P2 (protocole GB-9).
