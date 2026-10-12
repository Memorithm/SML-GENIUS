# A6-E3 — Banque native de 128 fonctions booléennes, prédiction réelle du prochain octet : STOP qualifié

**Campagne du 12 octobre 2026. Verdict préenregistré : `STOP`.** L'expérience a trouvé un faible gain du mélange probabiliste *bits bruts + ngramme 4*, mais **aucune fonction AND, XOR à deux octets ni XOR à trois octets n'a été retenue par l'apprentissage**. Les modèles RAW32 et ANF128 aboutissent aux **mêmes huit bits sélectionnés, mêmes pertes et mêmes précisions**. On ne peut attribuer le gain au calcul booléen composé.

## Chaîne de preuve et intégrité

- **Protocole figé AVANT code/TEST :** [A6-E3 préenregistrement](A6-E3-FULL-BYTE-BOOLEAN-BANK-PREREGISTRATION.md), premier commit `911029608694e1194c0989e6d04cf09d56edc05e`. 128 fonctions candidates publiquement déterminées avant les données : 32 bits bruts, 32 XOR de couples, 32 AND de couples, 32 XOR de triplets. Quatre octets d'entrée (32 bits physiques), sélection TRAIN seulement de 8 portes par bras, préfixes k4/k6/k8, β=128, α=16/128, λ=0/0,1/0,25/0,5/1.
- **Code Rust exact mesuré :** commit `3ca3896066f68da72a47642153d9b27411e218e2`, blob du binaire `77636198f7e656db8dfa9ed9d1a6a3bb03fffff6`. Workflow dédié [run 38300703335](https://github.com/Memorithm/SML-GENIUS/actions/runs/38300703335) : `validate` job **114982658015** SUCCESS (format Rust1.89, check, Clippy `-D warnings`, 4 tests) ; répétitions **114982857087** et **114982857148** SUCCESS, sur GitHub-hosted Ubuntu 24.04 avec check de l'empreinte SHA256 du corpus. Le même corpus/test est utilisé dans les deux exécutions : ce sont **des répétitions matérielles**, pas des échantillons indépendants. Le suivi de la CI générale doit également être vert au SHA du rapport final.
- **Archives originales complètes :** réplica 1 [artefact 11769589211](https://github.com/Memorithm/SML-GENIUS/actions/runs/38300703335/artifacts/11769589211), SHA256 du ZIP téléchargé `cfa548a4ff25fe856e5f434935145c536ed6dfff8742cfb468d8cebb21763d73` ; réplica 2 [artefact 11769234470](https://github.com/Memorithm/SML-GENIUS/actions/runs/38300703335/artifacts/11769234470), SHA256 `7a9f73f6642fd05db61c90ed73fee238ec76c081f4b39ce6b5a8cbe6197ebc94`. Les deux ZIP ont été ouverts : chacun contient **109 lignes de données** dans `a6-e3.log` (2 GREEDY, 6 BANK, 1 TRAIN, 12 VAL_PURE, 60 VAL_MIX, 12 COVERAGE, 8 SELECTED, 6 METRIC, 1 ALL_FROZEN, 1 COMPLETE). Après retrait des seules durées `training_seconds`, `validation_seconds`, `test_seconds`, **0 désaccord sur 109 lignes** entre les deux réplicas, y compris les scores TEST. Les deux ont publié `decision=STOP,integrity=PASS`.
- **Corpus et effectifs :** Projet Gutenberg, Jules Verne, *Le Tour du monde en quatre-vingts jours*, [ebook #800](https://www.gutenberg.org/ebooks/800), uniquement le corps normalisé LF de **433036 octets**, SHA256 `b51f36fb2baf0847fd3632fbd6ff939b8858abc1d728b5f86c94d99e64766a74`, récupéré par `scripts/fetch-char-corpus.sh`. Découpage 80/10/10 déjà utilisé ; 64 voies sans recouvrement et sans contexte inter-voie, positions t≥4. **346112 observations TRAIN, 43008 VAL, 43008 TEST**, chaque cible est l'un des 256 octets, et chaque perte est calculée sur le **même ensemble TEST** en **bits/octet**. Ce n'est pas la tâche binaire A6-E1/E2 en bits/événement. Le ngramme utilise historiquement aussi les positions précoces t1..t3 pour apprendre ses comptes TRAIN ; les comparaisons de TEST et VAL, elles, sont strictement appariées t≥4.

## Ce que le moteur a réellement sélectionné

La recherche gloutonne minimisant la log-loss TRAIN leave-one-out a évalué **228 extensions RAW32** et **996 extensions ANF128**. Les deux recherches ont convergé vers **exactement le même ordre** d'identifiants :

`[5, 6, 14, 0, 3, 1, 2, 4]`.

Tous ces identifiants appartiennent à la première famille **0..31 de bits bruts**. **Zéro porte composée XOR/AND/triple sélectionnée** parmi les 96 fonctions supplémentaires autorisées par ANF128. Les 128 définitions publiques sont vérifiées contre les fonctions natives `WideAnfGate` par les tests Rust exhaustifs de vérité locale ; les 8 portes sélectionnées sont aussi vérifiées lors de l'entraînement réel. La sélection trainée se borne à choisir des fonctions d'une grammaire fixe, elle **n'apprend pas de nouvelles équations de portes**.

| Budget de portes k | Slots de signature | Signatures observées dans TRAIN | Compteurs par bras (octets u32) | Compteurs de totaux (octets) |
|---:|---:|---:|---:|---:|
| 4 | 16 | 15 | 16384 | 64 |
| 6 | 64 | 55 | 65536 | 256 |
| 8 | 256 | 155 | 262144 | 1024 |

Les tables de comptes sont dupliquées pour RAW/ANF afin d'assurer l'appariement des comparateurs : **690816 octets de charge logique cumulée** (2 bras ×3 tailles, compteurs de classe et totaux), hors allocations, tableaux intermédiaires, texte, n-grammes et modèle de portes. Ngramme2 stocke **11254** couples (contexte, valeur), ngramme4 **108031** ; **ne pas assimiler ces nombres de comptes à des octets de RAM**. Pour les six modèles, zéro événement VAL et zéro événement TEST n'utilise une signature absente de TRAIN, ce qui ne prouve pas une couverture exhaustive des combinaisons possibles (seulement 155/256 slots k8 présents en TRAIN).

## Sélection VAL puis TEST tenu à l'écart

La grille a réellement évalué **12 modèles purs et 60 mélanges** uniquement sur VAL. Chaque bras a choisi le même modèle pur `k=8, α=16, λ=1` et le même mélange `k=8, α=128, λ=0.10`. Les indices globaux gelés avant TEST étaient RAW pur **24**, RAW mix **26**, ANF pur **54**, ANF mix **56**. Les index `0..3` imprimés dans les lignes TEST désignent la liste **locale de ces quatre modèles gelés**, pas les index globaux VAL ; leurs attributs k/α/λ et leur ordre correspondent à la liste gelée. **TEST n'a évalué aucun des 56 autres mélanges**.

| Modèle | VAL, bits/octet ↓ | TEST, bits/octet ↓ | TEST exact, argmax correct / 43008 |
|---|---:|---:|---:|
| Unigramme TRAIN lissé | 4.65738009 | 4.70355985 | 6042 |
| Witten–Bell ngram2 | 2.74968443 | 2.73110891 | 17735 |
| Witten–Bell **ngram4** | **2.08543726** | **2.01532016** | **24886** |
| RAW pur (k8, α16) | 3.32870831 | 3.33509577 | 13209 |
| ANF pur (k8, α16) | 3.32870831 | 3.33509577 | 13209 |
| RAW mix (k8, α128, λ=.10) | **2.05794409** | **1.99022046** | **24932** |
| ANF mix (k8, α128, λ=.10) | **2.05794409** | **1.99022046** | **24932** |

Le mélange atteint un **gain de 0.02509970 bit/octet** (`2.01532016−1.99022046`) par rapport au ngram4 ; il est observé aussi en VAL, où le gain est `2.08543726−2.05794409=0.02749317`. L'exactitude argmax sur TEST augmente de **46 prédictions correctes sur 43008**, sans analyse d'incertitude par document/voie ni garantie de significativité. **ANF et RAW ont une amélioration rigoureusement identique.**

## Décision préenregistrée et portée de la conclusion

`RESEARCH_PASS` exigeait **toutes** les conditions sur le TEST commun : (1) `ANF_PURE≤ngram4−0.02`, (2) `ANF_MIX≤ngram4−0.02`, (3) `ANF_MIX≤RAW_MIX−0.01`.

- **Condition 1 : ÉCHEC.** ANF pur 3.33509577 est beaucoup moins performant que ngram4 2.01532016.
- **Condition 2 : SUCCÈS DESCRIPTIF.** ANF mix 1.99022046 est inférieur à ngram4 moins 0.02.
- **Condition 3 : ÉCHEC.** ANF mix = RAW mix = 1.99022046, différence **zéro**.

**Résultat final = STOP** ; ne pas présenter l'amélioration du mélange comme une preuve d'utilité des portes composées. Les transformations déterministes de bits **ne créent pas d'information nouvelle** par rapport à l'ensemble complet RAW32 ; à budget de huit coordonnées, elles pourraient néanmoins créer une représentation plus efficace ou échantillonnable. Ce bénéfice **n'a pas été mis en évidence** par cette sélection gloutonne. Il reste aussi impossible d'en conclure que toute grammaire ANF ou toute stratégie de sélection est incapable d'exploiter de telles interactions : des fonctions utiles collectivement peuvent être faibles marginalement dans une sélection porte-par-porte.

Cette expérience utilise une seule œuvre et des séquences historiquement proches ; elle ne constitue pas une réplication multi-corpus et les observations par voie sont dépendantes. Pas de preuve de modèle linguistique SML-2/25B, intuition émergente, banque 1 048 576 fonctions, remplacement universel de l'attention ou gains GPU. Le progrès suivant doit tester **une sélection conjointe/budgetée de fonctions**, à protocole neuf préenregistré, avec ablation brute, interactions négatives et nouvelle population d'observations ; ou isoler les facteurs expliquant le faible mais réel gain du mélange avec un test apparié par voie/corpus. Ne pas réviser A6-E3 après consultation de son TEST.
