# A6-E2 — Lecture probabiliste ANF entraînée sur corpus réel : résultat mesuré **STOP** (2026-10-12)

**Décision scientifique préenregistrée : STOP.** La nouvelle probabilité KT/Dirichlet associée aux supports ANF réels réduit très nettement l'ancienne surconfiance du comité ANF, mais **ne bat toujours pas le ngram4 compétent** sur la tâche linguistique testée. Ce résultat négatif qualifié n'est pas une erreur CI. Il ne démontre aucune émergence d'intuition ni compétence linguistique SML-2/25B.

## Source, gel et reproductibilité

Protocole enregistré **avant le premier code** au commit `9f6a1f92d818536bc850b6775ffd64dfd6bfb705` : [A6-E2 preregistration](A6-E2-KT-CALIBRATED-ANF-PREREGISTRATION.md). Même corpus que A6-E1 : Jules Verne, *Le Tour du monde en quatre-vingts jours*, corps du texte du Projet Gutenberg #800, format UTF-8 normalisé LF, **433036 octets**, SHA256 **`b51f36fb2baf0847fd3632fbd6ff939b8858abc1d728b5f86c94d99e64766a74`**, déjà épinglé par `scripts/fetch-char-corpus.sh`. Découpage chronologique 80/10/10 `CharSplits`, tâche **prédire la présence de l'octet espace suivant**, avec 346240 observations TRAIN, 43136 VAL et **43136 TEST** ; ces scores sont en **bits/événement binaire**, pas les bits/octet full-token de A5.

Preuves natives [GitHub Actions run 38297230454](https://github.com/Memorithm/SML-GENIUS/actions/runs/38297230454) au code exact SHA `31c798f66735ca4f1a17de009acec89e72a509f5` :
- Validation Rust `fmt --check`, `cargo check --locked`, Clippy `-D warnings` et tests binaires : job **114971134965**, **SUCCESS**.
- Deux jobs de mesure GitHub-hosted isolés, **SUCCESS**, aux mêmes données et hyperparamètres : job **114971344827** (réplica 1) et **114971344801** (réplica 2). Archives intégrales `a6-e2-full-readout-replica-1` [artifact 11767669705](https://github.com/Memorithm/SML-GENIUS/actions/runs/38297230454/artifacts/11767669705), ZIP SHA256 `eeef5677dbb4d9e5fc4e938867fa0cfa7b4f862ef686ac40b4a15a7de0ff7d49`; `a6-e2-full-readout-replica-2` [artifact 11768414437](https://github.com/Memorithm/SML-GENIUS/actions/runs/38297230454/artifacts/11768414437), ZIP SHA256 `35905f7d105c9c63ffb8cb6fe5bd70ac305da859ff11943fdaff379a714ce3bd`. Les deux archives ont été téléchargées, ouvertes, recomptées : chacune contient **4 LEARNED, 16 VAL_PURE, 96 VAL_MIX, un ALL_FROZEN, un TEST_BASELINES, un TEST_SELECTED, un COMPLETE**. Les **122 enregistrements scientifiques par réplica sont identiques**, hors 3 temps `train/val/test_seconds`. Ce sont deux répétitions de calcul, **pas deux jeux TEST indépendants**.
- `cargo test` a réussi avec 3 tests binaires sur les tables Möbius native, les postérieurs strictement entre 0 et 1, le mélange `λ=0` reproduisant la probabilité ngram4 et les effectifs. Les critères de décision, grilles et le TEST n'ont pas été modifiés après l'exécution.

## Apprentissage effectué — cas défavorables conservés

Les **quatre supports `WideAnfGate`** sont appris depuis TRAIN **uniquement** par sélection séquentielle de coordonnées dans les 16 bits des deux octets précédents. L'ordre canonique, le nombre de termes GF(2) natifs et les comptes bruts des tables sont :

| Dimensions | Support physique appris (bit 0..15) | Monômes ANF natifs actifs | Cellules de comptage | Log-loss KT leave-one-out TRAIN (bits/event) |
|---:|---|---:|---:|---:|
| 2 | [2,14] | **0** | 4 | 0.53823642 |
| 3 | [2,4,14] | **0** | 8 | 0.51577729 |
| 4 | [0,2,4,14] | **0** | 16 | 0.49494218 |
| 5 | [0,2,4,6,14] | **16** | 32 | 0.45776776 |

Chaque table majoritaire est convertie par Möbius en une vraie ANF GF(2) et vérifiée sur toutes ses affectations par `WideAnfGate::evaluate_scalar`. Les probabilités prédites sont `(count_positive+α)/(count_total+2α)` **sur ces mêmes supports** ; ceci n'est **pas** un nouveau mécanisme d'activation du réseau ANF dans le runtime SML-25B. Les trois portes constantes zéro sont conservées et constituent une limite de l'apprentissage ; leurs comptes conditionnels contiennent malgré tout une information probabiliste marginale.

Les **16 candidats purs** croisent k=2/3/4/5 et `α∈{0.5,1,4,16}`. Les **96 politiques mixtes** croisent ces 16 candidats avec `λ∈{0,0.05,0.10,0.25,0.50,1}` dans `λ P_KT+(1−λ)P_ngram4`. Le meilleur candidat pur est `index 12` (k=5, α=.5) avec **0.45178386 bits/event sur VAL**. Le meilleur mélange **index 0, λ=0**, atteint **0.21999604 bits/event sur VAL** et est par définition le **ngram4 pur** : sur VAL, aucun apport positif de notre expert ANF n'a été retenu. Ces décisions sont affichées dans `A6_E2_ALL_FROZEN` **avant l'accès aux étiquettes TEST**.

## Mesures finales TEST strictement communes (43136 événements)

| Modèle TEST | Bits/événement binaire ↓ | Erreurs de classification |
|---|---:|---:|
| TRAIN majority | 0.58551026 | 6060 |
| Witten–Bell ngram2 | 0.30636244 | 4590 |
| **Witten–Bell ngram4 (contrôle compétent)** | **0.21751656** | **3236** |
| **KT/ANF pur choisi par VAL (k5, α=.5)** | **0.45528156** | **5474** |
| **Mélange choisi par VAL (λ=0)** | **0.21751656** | **3236** |
| Comité ANF **non calibré A6-E1** (historique, mêmes splits) | 2.52933653 | 5474 |

Le KT/ANF pur améliore l'ancien comité non calibré de **2.07405497 bits/event** (`2.52933653−0.45528156`), sans améliorer ici la précision en classification. Toutefois il est encore **0.23776500 bits/event PLUS MAUVAIS que ngram4** (`0.45528156−0.21751656`). Le meilleur mélange retenu avec `λ=0` n'améliore le ngramme d'**aucun** bit. Le critère *préalablement gelé* exigeait que **les deux modèles**, pur et mixte, gagnent chacun au moins 0.02 bits/event sur TEST. Les deux conditions échouent ; **STOP** sans réinterprétation du contrôle `λ=0` comme une victoire ANF.

## Interprétation et limites exactes

La **calibration probabiliste a corrigé en partie la surconfiance** antérieure, mais le niveau de signal des tables 2..5 bits et des deux seuls octets précédents est insuffisant dans cette expérience pour concurrencer un ngram4 prenant davantage de contexte. Cette explication est **hypothétique** : elle n'établit ni la cause complète de l'écart ni une infériorité intrinsèque des fonctions booléennes. Il faudrait comparer à représentation/longueur de contexte équivalente, à coût de calcul/mémoire contrôlé, et répéter sur d'autres corpus et tâches pour isoler ces facteurs.

Leurs ressources **logiques TRAIN** sont 4+8+16+32=60 cellules binaires, soit `60×2×4=480` octets de compteurs `u32` hors allocation, ANF, texte et n-grammes. Les références exposent **11254 paramètres ngram2**, **108031 paramètres ngram4** (comptages de structure, **pas** octets de RAM). Le job a rapporté `train_seconds=0.540`, `val_seconds=0.127/0.128` et `test_seconds=0.057/0.058` sur runners hébergés ; ce ne sont pas des mesures matérielles Thor ni des comparaisons à allocation/qualité équivalentes.

**Conséquence pour la roadmap :** conserver A6-E2 comme expérience négative qualifiée ; préenregistrer séparément une hypothèse plus discriminante, par exemple une représentation booléenne **à contexte temporel plus riche** avec comptage probabiliste à budget contrôlé, ablations `longueur de contexte` et `qualité du signal`, et comparaison sur tâches et splits figés contre ngram4. Ne jamais adapter cette étude A6-E2 aux scores du TEST déjà observés ; aucune promotion de modèle 2B/25B, GPU speedup ou intuition humaine démontrée.
