# WideSparseANF — addendum post-premier run (avant optimisation)

**Ajout daté du 12 octobre 2026 après la première exécution et avant toute optimisation.** Ce document n'efface ni ne modifie le [préenregistrement initial](ANF-WIDE-SPARSE-1024-PREREGISTRATION.md).

## Première mesure obligatoire conservée

Commit du premier banc complet `1cdf45d4d444428ea6922013c9befe7de595aa68`, [job GitHub-hosted 114704820279](https://github.com/Memorithm/SML-GENIUS/actions/runs/38217268362/job/114704820279). Les 22 familles/largeurs passent l'oracle littéral, les 2 tests récurrents et l'adaptateur k31 passent. Mesures médianes natives du premier run, cinq répétitions de 8 192 exécutions :

| Largeur | linear32 sparse | mixed16 sparse | full_and sparse |
|---:|---:|---:|---:|
| 8 | 19,126 ns | 44,825 ns | 17,151 ns |
| 16 | 34,536 ns | 54,749 ns | 36,117 ns |
| 32 | 63,297 ns | 67,415 ns | 64,240 ns |
| 64 | 79,161 ns | 83,730 ns | 93,722 ns |
| 128 | 112,811 ns | 104,359 ns | 177,821 ns |
| 256 | 138,762 ns | 145,105 ns | 342,042 ns |
| 1 024 | **378,175 ns** | **384,757 ns** | **1 341,112 ns** |

Contrôle dense `k8,dense128` : **424,280 ns**. Ces temps incluent la vérification en boucle chaude des index physiques de **toutes les k entrées**, même lorsque seuls quelques monômes sont actifs ; ils ne sont donc pas une mesure pure du coût des monômes. Le premier checksum XOR a produit zéro par cancellation des séquences périodiques sur les 5×8 192 tours, malgré `black_box` des sorties : ce n'est pas un échec d'oracle mais un instrument de diagnostic peu discriminant.

## Correction préenregistrée ici avant second run

1. **Pré-calculer une fois** le `required_source_len=max(global_input_index)+1` à la construction de chaque `WideAnfGate`, et vérifier `source.len()>=required_source_len` en O(1) avant de toucher le scratch. Cette condition est strictement équivalente au scan `inputs.iter().any(index>=source.len())` sur une porte immuable validée ; elle ne change ni monômes, ni seuil dense/sparse, ni erreur émise, ni valeur calculée. Le plafond de budget reste au constructeur.
2. Remplacer la réduction XOR symétrique des checksums par une réduction `wrapping_add` indexée afin d'empêcher la cancellation périodique systématique. Garder **les mêmes patterns, mêmes cinq répétitions, même compteur d'évaluations et même ordre**. Le checksum n'entre ni dans l'évaluation ni dans le choix du backend ; c'est seulement une sortie de diagnostic.
3. Refaire **strictement le même panel de 22 cas**, tests invariants, 2 ponts récurrents, 1 pont k31 et contrôles CI. Publier les durées **avant et après** et des conditions de comparaison, sans expliquer toute différence par la seule modification si le runner partage le CPU avec d'autres charges. Aucun gain GPU/SIMD matériel ni apprentissage de monômes n'est démontré.

**Le résultat initial et les modifications prévues restent auditables. Ne pas cacher les temps initiaux.**
