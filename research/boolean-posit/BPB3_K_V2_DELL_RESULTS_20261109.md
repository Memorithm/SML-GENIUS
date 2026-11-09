# BPB3-K fused AVX2 v2 — rapport de qualification

Décision au 9 novembre 2026 : **E — qualification physique incomplète**.
La fidélité numérique bornée est validée ; les deux campagnes Dell complètes
présentent une contamination. Aucun gain physique qualifié n'est revendiqué.

## Implémentation retenue

Le candidat `fused_v2_stock` conserve le format BPB3-K de 110 octets, les poids,
la quantification et tous les anciens noyaux. Le mode par défaut reste scalaire.
Il décode 32 coefficients signés à la fois, applique les sous-échelles dans des
entiers 16 bits et écrit directement dans le tampon de sortie, sans allocation
de vecteur par appel.

Le calcul flottant reproduit le véritable stock optimisé : regroupement de
quatre coefficients contigus par voie, FMA par bloc et réduction équilibrée
pour les petits lots ; réduction entière du bloc puis épilogue FMA du tiled
pour les lots admis par le stock. AVX2 et FMA sont vérifiés à l'exécution.

L'ancienne v2, `fused_v2`, reste archivée avec son contrat générique et ses
échecs de fidélité. Elle ne remplace pas le nouveau candidat qualifié.

## Preuves numériques

| Contrôle | Résultat |
|---|---:|
| Encodages FP16, toutes catégories | 65 536 couverts |
| Cas entiers indépendants issus des octets Q3_K originaux | 4 096 |
| Comparaisons binaires contre le C AVX2 original | 896 / 896 |
| Comparaisons binaires contre le tiled C++ original | 112 / 112 |
| Cas de sélection du contrat stock | 10 / 10 |
| Tests de noyaux par processus de mode | 23 réussis |
| Tests complets du dépôt avant publication | 2 505 réussis, 0 échec, 3 ignorés explicites |
| Cas SmolLM2 : 3 prompts × 2 contextes × 2 nombres de threads × 2 processus | 24 / 24 |
| Logits candidat / stock comparés | 9 437 184 |
| Écart absolu maximal / RMSE | 0 / 0 |
| Tokens gloutons comparés | 192 identiques |
| Fichiers bruts candidat / stock | 24 / 24 identiques octet par octet |

Les deux zéros signés, 2 046 sous-normaux, 61 440 nombres normaux, deux infinis
et 2 046 NaN FP16 sont distingués, avec vérification des charges utiles à la
conversion. La propagation d'une charge utile NaN dans l'arithmétique n'est pas
une propriété revendiquée.

Le critère initial est inchangé : tokens identiques et, pour chaque logit fini,
`abs(candidat − stock) ≤ 2e−4 + 2e−5 × abs(stock)`. Les répétitions fraîches
doivent être identiques bit par bit. La parité exacte v1 / ancienne v2 reste
un contrôle obligatoire ; leurs écarts au stock restent publiés.

Les 24 cas couvrent les contextes 128 et 512, dont des entrées réellement plus
longues, à deux et huit threads. Ce corpus ne prouve pas une équivalence
universelle ni une qualité générale de langage.

Preuve : [run SML 42591132613](https://github.com/Memorithm/SML-GENIUS/actions/runs/42591132613),
artefact `13661976926`, SHA256
`0084fd5b1f479784358282d0d6b218bb4d5edcc29e2e51a6dc60f49de29a5fd8`.
L'archive a été téléchargée et les paires brutes ont été vérifiées indépendamment.

## Cause de l'échec précédent

L'ancienne v2 reproduisait exactement la v1, mais toutes deux conservaient
l'ordre du C générique plutôt que celui du stock AVX2/tiled. Sur le prompt
français, l'écart maximal pouvait atteindre 25,95397758 avec des tokens
différents. La première divergence observée se situait dans `ffn_out-0`.

Un essai `nrc=1` a laissé la divergence inchangée : cette hypothèse de découpage
des lignes a été réfutée. Le profil diagnostic qui reprend uniquement l'ordre
AVX2 a ensuite rendu 16 des 24 cas exactement identiques au stock, avec les
4 096 points de trace bornés identiques ; les huit cas longs restaient différents.
Le candidat final reprend aussi l'ordre tiled et rend les 24 fichiers identiques.
Les seuils n'ont pas été ajustés après ces observations.

## Campagne Dell

Source SML fusionnée : `9eedaa6d454ae8bbbb1522f11375b98be33f0910`
([PR #894](https://github.com/Memorithm/SML-GENIUS/pull/894)). Son arbre Git
est identique au SHA qualifié `23943835f3bb947327009ff45fddef72f869f4f0`.
Les 75 contrôles de ce head étaient terminés : 12 réussis et 63 ignorés.

Contrôleur RemoteOps : `b90cafd9c62e40e042ac99a1d10a1b18987157d0`,
après les PR [#1416](https://github.com/Memorithm/RemoteOps/pull/1416) et
[#1417](https://github.com/Memorithm/RemoteOps/pull/1417), toutes deux fusionnées
après leurs contrôles exacts. La seconde conserve les observations rejetées ou
interrompues avant tout nettoyage, sans masquer le code d'échec du test.

[Run physique 42596339483](https://github.com/Memorithm/RemoteOps/actions/runs/42596339483),
déclenché sur `main` par `CHECKUPAUTO`, terminé. Les 203 processus et les
2 002 durées brutes ont été conservés puis audités indépendamment.

Le protocole conserve les six références et ajoute `fused_v2_stock` comme
septième variante : qualification `-t2 -p64 -n32 -r3`, puis 14 tours appariés
équilibrés à deux et huit threads, `-p64 -n32 -r5`. Chaque variante occupe chaque
position deux fois, et l'ordre des cohortes de threads alterne. Cela représente
203 processus de mesure et 2 002 durées brutes.

Le verrou coopératif, huit cœurs physiques d'un seul nœud NUMA, la priorité 15,
la limite de mémoire virtuelle par processus de 8 Gio, l'admission de 12 Gio de
RAM disponible et 8 Gio de disque, le budget disque observé de 5 Gio et le délai
de 95 minutes sont conservés. Il n'y a ni SDK GPU ni modification de service.

Les intervalles utilisent les 14 rapports de débits appariés, leur logarithme
et Student à 13 degrés de liberté. Un excès de CPU non attribué supérieur à
0,10 seconde CPU par seconde murale conserve la catégorie E ; aucun échantillon
lent n'est supprimé. Une session avec réservation coopérative ne prouve pas une
isolation du serveur ni une performance générale sur d'autres hôtes.

### Première session : observations conservées, catégorie E

Une mesure sur 203 dépasse le seuil de contamination : candidat, tour 5,
huit threads, CPU `[0,2,4,6,8,10,12,14]`. Sur 1,721495515 seconde murale,
12,48 secondes CPU occupées sont observées sur le groupe, contre 12,04 secondes
CPU attribuées au processus. L'excès normalisé vaut **0,2555917202**, supérieur
à 0,10. La session reste E, sans suppression de cette observation. La source
de l'activité supplémentaire n'est pas identifiée par ces compteurs.

Les chiffres ci-dessous décrivent cette session E et ne constituent pas une
qualification de gain. Les moyennes portent sur les 14 processus appariés
de chaque mode ; l'écart-type est calculé entre leurs débits moyens.

| Threads | Mode | Génération moyenne ± écart-type (tok/s) | Prompt (tok/s) | ms/token | Pic RSS (Kio) |
|---:|---|---:|---:|---:|---:|
| 2 | Stock Q3_K | 105,824 ± 0,698 | 355,968 | 9,452 | 171 168 |
| 2 | Scalaire | 22,740 ± 0,048 | 31,342 | 43,979 | 170 964 |
| 2 | SIMD | 17,359 ± 0,031 | 22,167 | 57,611 | 172 064 |
| 2 | Tiled | 11,777 ± 0,007 | 17,313 | 84,909 | 172 224 |
| 2 | Hybride | 17,429 ± 0,052 | 26,956 | 57,378 | 171 208 |
| 2 | Fused v1 | 50,088 ± 0,235 | 80,847 | 19,968 | 171 944 |
| 2 | Fused v2 stock | 78,750 ± 0,448 | 153,484 | 12,700 | 171 964 |
| 8 | Stock Q3_K | 260,262 ± 2,641 | 1 095,643 | 3,844 | 172 812 |
| 8 | Scalaire | 86,117 ± 0,903 | 120,957 | 11,622 | 170 572 |
| 8 | SIMD | 67,836 ± 0,803 | 86,615 | 14,760 | 170 584 |
| 8 | Tiled | 47,860 ± 0,453 | 67,912 | 20,903 | 170 552 |
| 8 | Hybride | 67,488 ± 1,320 | 105,280 | 14,842 | 170 628 |
| 8 | Fused v1 | 163,464 ± 2,106 | 305,898 | 6,123 | 170 596 |
| 8 | Fused v2 stock | 211,386 ± 2,967 | 560,008 | 4,732 | 170 652 |

| Threads | Rapport apparié v2 / référence | Intervalle 95 % descriptif de la session E |
|---:|---|---|
| 2 | v1 : 1,572227 | [1,564661 ; 1,579830] |
| 8 | v1 : 1,293145 | [1,279207 ; 1,307234] |
| 2 | stock : 0,744159 | [0,740362 ; 0,747976] |
| 8 | stock : 0,812167 | [0,803458 ; 0,820971] |

Les observations suggèrent une amélioration sur v1 et un déficit face au stock ;
leur contamination interdit de les présenter comme la catégorie B ou A. Aucun
critère de régression mémoire n'est déclenché. Les valeurs historiques fournies
au départ ne sont pas une référence interchangeable avec cette session.

Variabilité des 70 répétitions brutes par mode et cohorte : coefficient de
variation du débit (écart-type / moyenne), toujours descriptif de la session E.

| Mode | Génération 2 threads | Prompt 2 threads | Génération 8 threads | Prompt 8 threads |
|---|---:|---:|---:|---:|
| Stock | 1,492 % | 1,984 % | 1,809 % | 3,719 % |
| Scalaire | 0,894 % | 0,741 % | 2,778 % | 1,707 % |
| SIMD | 0,699 % | 0,852 % | 3,258 % | 1,538 % |
| Tiled | 0,425 % | 0,328 % | 2,004 % | 1,236 % |
| Hybride | 0,728 % | 0,780 % | 3,899 % | 1,441 % |
| Fused v1 | 1,128 % | 1,129 % | 2,841 % | 1,947 % |
| Fused v2 stock | 1,195 % | 2,425 % | 1,458 % | 2,762 % |

Sur ce même Dell, les 24 paires candidat / stock sont identiques octet par octet :
9 437 184 logits et 192 tokens, avec répétitions fraîches et parité ancienne v2/v1
exactes. L'audit a aussi recalculé l'ordre équilibré, les durées, l'affinité,
les intervalles et la catégorie depuis les fichiers bruts.

Artefact `13665644320`, SHA256
`a4db0128c001cbceabbc2d2e8cba5213f19658e5ee5f1099e0b1b54b6b533d00`.
Audit indépendant : `bpb3k-v2-dell-attempt1-audit.json`.

### Diagnostic de noyau : portée limitée

Les quatre microbenchmarks conservés exécutent chacun 20 000 produits directs,
sans constituer un benchmark de modèle ni une estimation de variabilité.

| Superblocs | v1 (ns/produit) | v2 i16 (ns/produit) | v2 ancienne (ns/produit) | v2 stock (ns/produit) |
|---:|---:|---:|---:|---:|
| 3 | 715,263 | 364,070 | 315,904 | 316,163 |
| 6 | 1 255,573 | 610,320 | 504,785 | 472,114 |
| 9 | 1 854,523 | 896,748 | 747,234 | 697,538 |
| 36 | 7 271,219 | 3 450,069 | 2 859,218 | 2 676,912 |

L'ablation i16/ancienne v2 isole le décodage groupé de 32 valeurs à contrat
arithmétique identique, tandis que v2 stock a un contrat flottant différent,
vérifié contre les deux oracles originaux. Ces durées uniques appuient
l'intérêt d'étudier le décodage et la fusion, mais ne répartissent pas les
causes du temps total du modèle. Il manque des profils d'instructions/cache et
de synchronisation pour attribuer le déficit restant face au stock. Une
réorganisation persistante des poids ou un autre déroulage ne sont pas promus
sans nouvelle qualification indépendante.

### Deuxième tentative : refus d'admission, aucune mesure

Le [run 42623362364](https://github.com/Memorithm/RemoteOps/actions/runs/42623362364),
contrôleur `c75856113be44ccf2cfc975a3fe1ec91033411e0`, s'arrête sur
`STOP_SELECTED_CORES_BUSY` avant tout test de modèle ou chronométrage.
Artefact `13676808559`, SHA256
`c79380cb94c59ab57d795ed3815bc96cdd1210ad773bd2e5ec7f597d90c3bfca`.
Aucune charge existante n'est interrompue.

### Réplication avec sélection initiale de cœurs disponibles

La [PR RemoteOps #1423](https://github.com/Memorithm/RemoteOps/pull/1423)
sélectionne huit cœurs physiques disponibles dans un seul socket/nœud NUMA,
avec chacun de leurs frères SMT sous le seuil initial inchangé de 10 %.
La sélection précède tout test ou timing ; toutes les variantes conservent
le même groupe. Les critères numériques, les sept variantes, les 203 processus
et le seuil ultérieur de contamination sont inchangés. Les sondes complètes
et la politique de choix sont archivées. Les 16 tests Python et 283 tests Rust,
le formatage et Clippy strict passent ; les deux checks du head exact
`3f9628e8b31c4b82be1ed5056c38b5ab76b4af64` sont success/skipped avant fusion.

[Run 42626090419](https://github.com/Memorithm/RemoteOps/actions/runs/42626090419),
contrôleur `ae8ccdcf9b20099bdc25b16f868ea70515483b3f`, terminé le 9 novembre
2026. L'audit indépendant vérifie les 203 processus, 2 002 durées, l'ordre,
les ressources, la sélection initiale des cœurs et les 24 paires numériques.
L'admission choisit le socket/nœud NUMA 1, CPU `[1,3,5,7,9,11,13,29]` ;
tous leurs frères SMT sont initialement sous 2 % d'occupation. La sonde
initiale ne garantit pas une disponibilité constante pendant toute la session.

**Résultat : E. Trois mesures sur 203 dépassent le seuil de contamination.**
Les identifiants de tour ci-dessous sont ceux du fichier brut, indexés à zéro.

| Tour | Threads | Mode | CPU non attribué / seconde murale | Seuil maximal |
|---:|---:|---|---:|---:|
| 1 | 8 | Fused v1 | 0,153604 | 0,10 |
| 2 | 8 | Fused v2 stock | 0,220747 | 0,10 |
| 10 | 8 | Fused v1 | 0,595767 | 0,10 |

Aucun processus ni échantillon n'est retiré. Aucun critère de régression
mémoire n'est déclenché. Les chiffres suivants restent descriptifs de cette
seconde session complète E, sans regroupement avec la première.

| Threads | Mode | Génération moyenne ± écart-type (tok/s) | Prompt (tok/s) | ms/token | Pic RSS (Kio) |
|---:|---|---:|---:|---:|---:|
| 2 | Stock Q3_K | 106,474 ± 0,703 | 357,851 | 9,394 | 170 492 |
| 2 | Scalaire | 22,739 ± 0,083 | 31,349 | 43,982 | 172 492 |
| 2 | SIMD | 17,378 ± 0,041 | 22,088 | 57,546 | 174 432 |
| 2 | Tiled | 11,762 ± 0,041 | 17,296 | 85,026 | 174 468 |
| 2 | Hybride | 17,415 ± 0,119 | 26,972 | 57,432 | 172 500 |
| 2 | Fused v1 | 50,181 ± 0,196 | 80,686 | 19,930 | 170 936 |
| 2 | Fused v2 stock | 78,973 ± 0,361 | 154,609 | 12,665 | 172 172 |
| 8 | Stock Q3_K | 262,778 ± 3,173 | 1 096,738 | 3,806 | 172 832 |
| 8 | Scalaire | 87,435 ± 1,371 | 122,135 | 11,455 | 170 564 |
| 8 | SIMD | 68,624 ± 0,588 | 87,288 | 14,579 | 172 024 |
| 8 | Tiled | 47,053 ± 3,921 | 67,676 | 21,470 | 170 528 |
| 8 | Hybride | 68,588 ± 1,021 | 105,560 | 14,589 | 171 056 |
| 8 | Fused v1 | 163,773 ± 4,804 | 302,267 | 6,121 | 171 136 |
| 8 | Fused v2 stock | 211,453 ± 4,543 | 563,315 | 4,733 | 170 596 |

| Mode | Génération 2 threads | Prompt 2 threads | Génération 8 threads | Prompt 8 threads |
|---|---:|---:|---:|---:|
| Stock Q3_K | 1,503 % | 1,588 % | 1,603 % | 6,406 % |
| Scalaire | 0,932 % | 0,815 % | 3,518 % | 1,854 % |
| SIMD | 0,709 % | 1,472 % | 2,087 % | 1,386 % |
| Tiled | 0,659 % | 0,862 % | 8,714 % | 2,728 % |
| Hybride | 1,299 % | 1,012 % | 2,496 % | 1,526 % |
| Fused v1 | 1,102 % | 2,081 % | 4,522 % | 10,248 % |
| Fused v2 stock | 1,368 % | 1,452 % | 2,632 % | 3,091 % |

| Threads | Rapport apparié v2 / référence | Intervalle 95 % descriptif de la session E |
|---:|---|---|
| 2 | Fused v1 : 1,573770 | [1,567916 ; 1,579645] |
| 2 | Stock Q3_K : 0,741725 | [0,738866 ; 0,744594] |
| 8 | Fused v1 : 1,291386 | [1,278461 ; 1,304441] |
| 8 | Stock Q3_K : 0,804559 | [0,796414 ; 0,812788] |

Les 24 paires de fichiers candidat/stock sont de nouveau identiques :
9 437 184 logits, 192 tokens, max absolu et RMSE nuls, répétitions et parité
v1/ancienne v2 exactes. L'archive est l'artefact `13682198523`, 235 621 763 octets,
SHA256 `91a1a6aa4c276a4221e454b22eba976607ebe72153c8f68547e4115ffa783bb6`.
Le modèle original est le même GGUF Q3_K_S SmolLM2-135M-Instruct, SHA256
`7add77b8d3736d6b2fd21dc96e69be026f19bde843e9988b43cef1315fc5eebe`.
Les identités de chaque binaire et de la conversion figurent dans
`binary-model-sha256.txt` de chaque archive ; compiler à nouveau ne garantit
pas de reproduire leurs octets. Les deux archives brutes restent conservées
localement ; leur rétention GitHub initiale est de trente jours.

### Décision scientifique

**E — qualification physique incomplète.** La fidélité numérique du candidat
est confirmée sur le corpus fixé, sur deux sessions Dell et sur l'hôte CI.
Les deux sessions Dell observent une v2 plus rapide que v1 et plus lente que
le véritable stock optimisé, mais leurs contaminations interdisent de les
promouvoir en A ou B. Les objectifs de supériorité sur le stock ne sont pas
démontrés. Aucun seuil, label ou échantillon n'a été modifié pour les atteindre.

Le choix de cœurs initialement disponibles corrige l'admission, sans établir
une réservation durable. Le prochain travail justifié porte sur l'attribution
de la charge concurrente et une fenêtre de mesure effectivement disponible,
avec une stratégie de réplication annoncée avant de nouvelles mesures. Ne pas
enchaîner des relances jusqu'à obtenir un essai favorable. Les observations E
restent publiées séparément et ne deviennent pas des gains qualifiés.

L'audit portable utilise uniquement la bibliothèque standard Python. Exemple
pour la réplication (archive téléchargée depuis le run ci-dessus) :

```text
python verify_bpb3k_dell_artifact.py dell-v2-attempt3.zip --sha256 91a1a6aa4c276a4221e454b22eba976607ebe72153c8f68547e4115ffa783bb6 --controller ae8ccdcf9b20099bdc25b16f868ea70515483b3f --out audit.json
```

## Enseignements transférables

Pour SciRust et FLAT-ATTENTION : confronter le noyau au véritable chemin optimisé
du comparateur, pas seulement à son implémentation générique ; conserver des
oracles indépendants issus du format d'origine ; prouver les bornes entières ;
tester les strides et sentinelles ABI ; distinguer FMA, ordre de réduction,
fidélité de modèle et vitesse. Toute réutilisation doit repasser les oracles du
projet destinataire et ses propres mesures physiques.
