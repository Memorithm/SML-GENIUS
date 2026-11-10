# BPB-3 : représentation booléenne à plans de bits et échelle dyadique partagée — validation arithmétique, trafic mémoire et perspectives pour l'attention

**BPB-3: Boolean Bitplanes with Shared Dyadic Scaling — Arithmetic Validation, Memory-Traffic Measurements, and Implications for Attention**

**Version :** 0.1.0 — 7 novembre 2026  
**Statut :** manuscrit de recherche évolutif (*working paper*), non évalué par les pairs, non soumis à une conférence  
**Auteurs et affiliations :** à compléter et confirmer avant diffusion académique formelle  
**Projet :** Memorithm, programme de recherche « Boolean Posit Block »  
**Dépôt principal :** https://github.com/Memorithm/SML-GENIUS/tree/main/research/boolean-posit  
**Licence et droits du manuscrit :** à déterminer avant republication ou soumission (ne pas inférer du dépôt de code).  
**Politique de révision :** toute modification substantielle de protocole ou de résultat exige une nouvelle version, des références de commits immuables et une note de changement.

> **Avertissement scientifique.** Ce manuscrit synthétise des implémentations et observations consignées dans des sources primaires datées et identifiées. Il n'affirme ni nouveauté démontrée, ni conformité au Posit Standard 2022, ni supériorité universelle, ni amélioration mesurée de la qualité ou du débit d'un grand modèle de langage. Les expériences sont majoritairement synthétiques ; les mesures de reconstruction sur un modèle entraîné ne constituent pas une évaluation en aval du modèle.

## Résumé

Nous étudions **BPB-3/v0**, une représentation expérimentale de poids quantifiés fondée sur trois plans de 64 bits et un exposant dyadique partagé par groupe de 64 coefficients. Son format sérialisé contient 26 octets par groupe (3,25 bits/poids), pour des coefficients signés appartenant à [-3,3] et une échelle 2^k. Un produit scalaire avec entrées bipolaires est calculé directement à l'aide de masques logiques et de comptages de bits, sans développement dense des 64 poids en flottants. Un oracle INT8 indépendant a validé l'identité arithmétique sur des corpus déterministes, dont une campagne matérielle de 256 millions de blocs (16,384 milliards de coefficients). Sur un serveur Dell PowerEdge T430 à deux sockets, six configurations qualifiées comparant le noyau BPB-3 à des références INT8 SIMD AVX2 ou SSE4.1 rapportent de **43,91 % à 44,67 % de lectures DRAM en moins** et des rapports de durée de balayage de **1,217× à 1,286×**, sous réserve des limites instrumentales et du caractère mono-machine et synthétique des charges. Sur Apple M4/Metal, un banc de quatre représentations (BPB-3, INT3, INT4, INT8) a passé ses contrôles d'égalité numérique ; BPB-3 a présenté la plus faible médiane « soumission hôte + attente » dans trois sessions proches, sans qualification du temps GPU isolé. Une comparaison sur quatre matrices d'un checkpoint entraîné Pythia-14M montre toutefois que le format BPB-3 avec deux résidus FP16 reste **36,6 % à 43,1 % moins bon en RMSE** que GGUF Q3_K à coût égal (110 octets pour 256 coefficients). Une variante distincte, **BPB3-K**, réorganise sans perte les données Q3_K en plans de bits tout en conservant ses échelles ; elle préserve alors exactement la reconstruction Q3_K dans les tests qualifiés, mais n'établit aucun avantage de performance. Nous explicitons les implications envisageables pour le moteur d'attention fusionnée FLAT-ATTENTION, sans confondre quantification de poids, sélection booléenne de clés et attention end-to-end.

**Mots-clés :** bitplanes, quantification à faible précision, POPCOUNT, XNOR, mémoire DRAM, INT8 SIMD, GGUF Q3_K, WGPU, attention fusionnée, reproductibilité.

## Abstract

We investigate **BPB-3/v0**, an experimental 64-weight block representation with three 64-bit Boolean bitplanes and a shared dyadic scale, serialized in 26 bytes (3.25 bits/weight). For quantized signed coefficients in [-3,3] and bipolar inputs, exact integer dot products are computed using Boolean masks and population counts without unpacking the complete weight block into dense floating-point values. Deterministic scalar oracles were exercised at up to 256 million blocks (16.384 billion coefficients). Six accepted dual-socket Dell PowerEdge T430 hardware configurations reported **43.91–44.67% fewer DRAM read-CAS bytes** than selected AVX2/SSE4.1 dense INT8 baselines, with **1.217–1.286×** scan-time ratios; evidence is limited to one machine and synthetic matched workloads. On a physical Apple M4 using Metal, four GPU kernels passed blockwise numerical parity; BPB-3 had the lowest host-submit-plus-fence median in three temporally adjacent sessions, without isolated GPU kernel timing. On actual trained Pythia-14M matrices, an equal-budget BPB-3 plus two indexed FP16 residuals had **36.6–43.1% higher reconstruction RMSE** than official GGUF Q3_K. A separate Q3_K-compatible Boolean-transcoding layout, **BPB3-K**, achieves bit-exact Q3_K reconstruction by retaining Q3_K's quantization metadata, with no accuracy or performance superiority claim. We describe possible but unproven interactions with FLAT-ATTENTION and identify necessary prospective evaluations.

## 1. Introduction

La multiplication des paramètres des réseaux neuronaux accroît la quantité de données à stocker, déplacer et exploiter lors de l'inférence. Pour de nombreuses charges, les accès aux différents niveaux de mémoire contribuent significativement au coût global. Les algorithmes d'attention sensibles aux entrées/sorties, tels que FlashAttention [1], montrent l'intérêt de réduire les transferts de données plutôt que de n'optimiser que le nombre d'opérations.

La question étudiée ici est circonscrite : **une disposition en plans de bits des poids quantifiés, assortie d'une échelle commune, peut-elle préserver les résultats entiers d'un produit scalaire bipolaire tout en diminuant les octets lus en mémoire physique ?** Cette question doit être séparée de trois autres : la conservation de la précision des poids d'origine, la vitesse d'un noyau GPU optimisé et la qualité d'un modèle de langage complet.

Les contributions actuellement justifiées sont : (C1) un contrat de format BPB-3/v0 et une primitive de calcul booléen vérifiée ; (C2) une validation de très grande taille par oracle de bloc ; (C3) des mesures CPU matérielles avec contrôles du trafic et de la topologie NUMA ; (C4) une vérification de parité GPU sur Apple M4 ; (C5) des comparaisons de reconstruction sur poids entraînés, incluant un **résultat défavorable** ; (C6) une variante de transcodage BPB3-K numériquement équivalente à Q3_K. Ces travaux constituent un programme de recherche **autonome**, sans intégration autorisée à un runtime de modèle SML-GENIUS finalisé.

## 2. Travaux antérieurs et délimitation de l'originalité

Le Posit Standard 2022 [2] spécifie un système numérique comportant notamment régime et précision variables. BPB-3 n'est **pas** un type posit standard ; son exposant partagé *inspiré* du codage par régime ne produit pas la sémantique arithmétique des posits scalaires. L'emploi du nom « Boolean Posit Block » est historique et descriptif, non une revendication de conformité.

L'utilisation de plans de bits, des opérations XNOR/POPCOUNT et des réseaux binaires ou ternaires appartient à un corpus de travaux établi, dont BitNet [3]. Les formats à échelles partagées sont également représentés par la spécification de microscaling MX de l'Open Compute Project [4]. Les codecs Q3_K/Q4_K du projet ggml/llama.cpp [5] constituent des références industrielles pertinentes en quantification basse précision.

**Hypothèse de contribution technique à auditer :** la combinaison exacte d'un format de bloc à trois plans, d'un exposant partagé à codage particulier et d'un noyau de produit scalaire bipolaire avec preuves arithmétiques et mesures de trafic. **Aucune revendication de brevetabilité, de première mondiale ou de nouveauté académique n'est établie.** Une revue systématique des antériorités et une comparaison équitable avec des noyaux bit-sliced concurrents restent nécessaires.

## 3. Contrat mathématique et représentation

### 3.1. Domaine

Pour chaque bloc de N=64 coefficients, nous posons :

- q_i ∈ {-3,-2,-1,0,1,2,3} ;
- k ∈ [-24,27] entier ;
- ŵ_i = q_i × 2^k ;
- x_i ∈ {-1,+1} pour les entrées bipolaires.

L'exposant est commun au bloc : k = 4r+e, avec e ∈ {0,1,2,3} ; le régime r est codé dans un mot de 16 bits par une séquence unaire suivie de deux bits d'exposant et de remplissage nul canonique. Ce codage n'est pas celui d'un posit scalaire [6].

### 3.2. Sérialisation et empreinte mémoire

Le format sérialisé comprend un mot d'échelle de 16 bits et trois mots de 64 bits, nommés *negative*, *lo* et *hi* :

B_serial = 2 + 3 × 8 = **26 octets pour 64 poids**, soit 26×8/64 = **3,25 bits/poids**.

Les plans *lo* et *hi* encodent les deux bits d'amplitude absolue, tandis que *negative* indique le signe des coefficients strictement négatifs. Le zéro négatif est interdit. Les blocs invalides ou les exposants hors domaine sont rejetés par le décodeur.

Une structure résidente Rust alignée occupe **32 octets** par bloc ; ce nombre n'est pas interchangeable avec les 26 octets du flux sérialisé. Sur le GPU, l'expérience [10] charge **24 octets de plans de poids** par bloc et omet du payload chronométré l'échelle commune lorsqu'elle est fixée. Les ratios de mémoire doivent donc toujours désigner leur périmètre exact.

Pour 64 poids, les charges sérialisées comparées sont de 26 octets pour BPB-3, 64 pour INT8 et 256 pour FP32, **en considérant uniquement le stockage des poids**. L'économie logique BPB-3 contre INT8 est (64−26)/64 = **59,375 %** ; elle n'est pas une mesure physique de DRAM.

### 3.3. Produit scalaire booléen

Soit X le mot de 64 bits marquant x_i = −1 et S le plan *negative*. Définissons E = NOT(S XOR X), avec une négation limitée aux 64 voies, et pour un masque M :

C(M) = 2 × popcount(E AND M) − popcount(M).

Le produit scalaire entier du bloc est exactement :

D = C(lo) + 2 × C(hi).

La valeur représentée est le couple (D,k), interprété mathématiquement comme D × 2^k. L'exactitude vaut **vis-à-vis des coefficients déjà quantifiés**, et non vis-à-vis des poids flottants originaux. Les coûts d'alignement d'échelle interblocs, de normalisation, d'arrondi et de gestion de débordement dans une accumulation multi-blocs ne sont pas entièrement spécifiés par BPB-3/v0.

L'identité ci-dessus s'explique voie par voie : E_i vaut 1 lorsque le signe de l'entrée correspond au signe du coefficient non nul ; la transformation 2E_i−1 donne alors le facteur ±1 attendu, tandis que les masques d'amplitude sélectionnent les contributions de magnitude 1 et 2.

## 4. Méthodes de qualification

### 4.1. Exactitude et grande taille

Une implémentation scalaire INT8 indépendante calcule chaque produit sur les **mêmes coefficients quantifiés** et les mêmes entrées bipolaires. Les tests initiaux incluent valeurs limites, round-trip de sérialisation, exposants et cas négatifs. Les validations par somme agrégée ont été renforcées par un oracle **par bloc**, car deux erreurs opposées peuvent s'annuler dans une somme totale [7].

Sur Dell T430, des campagnes contrôlées par RemoteOps ont progressé jusqu'à **256 000 000 blocs de 64 coefficients**, soit **16 384 000 000 coefficients synthétiques**. Les campagnes initiales ne vérifiaient que des sommes agrégées ; une qualification ultérieure avec l'oracle par bloc a été distinguée et documentée [7,8]. Le volume résident de 26,624 Go rapporté pour le plus grand corpus additionne les structures BPB résidentes, les vecteurs INT8 de référence et les entrées, non le seul BPB sérialisé.

### 4.2. Mémoire physique CPU

Le serveur Dell PowerEdge T430 utilise deux sockets Intel Xeon Broadwell-EP et une architecture NUMA. Les expériences [8,9] comparent les mêmes produits scalaires quantifiés avec sorties contrôlées par oracle indépendant, puis mesurent les événements Intel iMC *read-CAS* multipliés par 64 octets par événement. La méthode corrigée lit les **quatre types de PMU iMC exposés pour chaque socket**, soit huit descripteurs de comptage, et conserve l'affectation NUMA effective, les compteurs bruts, les temps d'activation et les témoins de bruit.

Chaque configuration qualifiée exige 12 paires acceptées de scans avec ordre alterné, quatre contrôles « même noyau contre lui-même », une exclusion lorsque le trafic de fond dépasse le seuil préétabli et un rejet si les témoins dérivent de plus de 15 %. L'ouverture privilégiée des descripteurs matériels précède un abandon irréversible des privilèges. Les charges restent confinées à un runner RemoteOps et aux limites de mémoire/temps du protocole.

**Point de correction important :** les premières mesures ne couvraient qu'un seul représentant de socket. Elles ne doivent **pas** être interprétées comme un résultat agrégé bi-socket. Les données rapportées au §5.2 proviennent des campagnes corrigées [9].

### 4.3. GPU physique

Le banc GPU [10,11] emploie des noyaux WGSL/WGPU distincts BPB-3, INT3 signé compact, INT4 signé compact et INT8. Chaque bras traite 4 096 blocs de 64 poids quantifiés pris **dans la même plage [-3,+3]**, avec les mêmes entrées bipolaires et vérification individuelle avant/après mesure. Les poids GPU résidents par bras occupent respectivement 98 304, 98 304, 131 072 et 262 144 octets, hors champs d'échelle non transportés et autres buffers.

Le chronomètre mesure le temps mur **soumission hôte + achèvement/fence**, non le temps intrinsèque du noyau GPU ; les données sont déjà chargées avant les régions chronométrées. L'expérience physique reconnue utilise Apple M4, Metal, sur un Mac mini réel, et non l'adaptateur Metal paravirtualisé des CI hébergées.

### 4.4. Reconstruction de poids entraînés

Une étude indépendante [12] compare les poids originaux d'un checkpoint entraîné EleutherAI/Pythia-14M (Apache-2.0, révision et SHA256 fixés) avec les sorties des **encodeurs officiels** ggml Q3_K et Q4_K et des variantes BPB. Quatre matrices de deux couches fournissent chacune 24 blocs DEV et 24 blocs TEST de 256 poids, avec indices disjoints et provenance enregistrée. Les poids source FP16 sont convertis exactement en FP32 avant quantification. Les blocs de 256 éléments couvrent deux lignes contiguës de largeur 128, ce qui constitue un **reblocking contrôlé**, pas une preuve sur le layout d'inférence GGUF réel.

Les métriques rapportées comprennent RMSE, nRMSE, MAE, biais, queues d'erreur, cosinus, erreur de produit scalaire et nombre exact d'octets de codec. Le candidat à deux résidus FP16 indexés utilise 104+6 = **110 octets/256**, égal au Q3_K de référence ; le nombre de résidus a été fixé avant examen de TEST. Les mêmes ensembles TEST déjà consultés ne pourront pas devenir des tests de confirmation pour un futur quantificateur retouché.

## 5. Résultats

### 5.1. Exactitude arithmétique

Les oracles des blocs quantifiés n'ont pas identifié de divergence sur les campagnes acceptées [7,8]. L'exécution de 256 millions de blocs constitue une démonstration de **scalabilité de l'implémentation de référence sur corpus synthétique**, mais ne constitue ni une preuve d'exactitude pour toutes les entrées possibles hors domaine, ni une preuve de qualité des poids avant quantification.

### 5.2. Lectures DRAM mesurées sur Dell

Le tableau ci-dessous reprend les **six configurations acceptées** du bilan bi-socket [9]. La réduction est la médiane des lectures matérielles *read-CAS* du noyau BPB-3 par rapport à la référence INT8 indiquée. Le rapport temporel est temps du noyau INT8 divisé par temps du noyau BPB-3 dans cette configuration.

| Référence INT8 | Corpus et NUMA | Réduction de lectures DRAM | Rapport temporel INT8/BPB |
|---|---|---:|---:|
| AVX2 signed-widening | 8m blocs, NUMA0 | 44,67 % | 1,236× |
| AVX2 signed-widening | 32m blocs, NUMA0 | 44,30 % | 1,221× |
| SSE4.1 signed-widening | 8m blocs, NUMA0 | 43,91 % | 1,280× |
| AVX2 signed-widening | 8m blocs, NUMA1 | 43,9295 % | 1,2168× |
| SSE4.1 signed-widening | 8m blocs, NUMA1 | 44,2411 % | 1,2616× |
| SSE4.1 signed-widening | 32m blocs, NUMA1 | 44,1185 % | 1,2857× |

Deux configurations 32m (SSE4.1/NUMA0 et AVX2/NUMA1) ont été **rejetées pour dérive des témoins**, malgré le succès des vérifications arithmétiques ; elles ne sont pas incluses dans les six lignes positives. L'ensemble est issu d'**une seule machine**, de charges synthétiques et d'une instrumentation de lectures aux contrôleurs ; il ne décrit pas la totalité des accès caches, le coût énergétique ou un débit de tokens/s.

### 5.3. Parité et observations temporelles Apple M4

Le run physique [11] a validé les douze bras-session (4 formats × 3 processus) contre l'oracle scalaire sur 4 096 blocs par bras-session, avec zéro divergence. Les médianes en microsecondes, converties des nanosecondes documentées, sont :

| Session (15 mesures/bras) | BPB-3 | INT3 compact | INT4 compact | INT8 |
|---|---:|---:|---:|---:|
| 1 | 174,000 | 224,750 | 183,625 | 220,208 |
| 2 | 213,791 | 268,459 | 222,083 | 264,459 |
| 3 | 212,458 | 269,000 | 222,333 | 263,542 |

BPB-3 possède la plus faible médiane de **ces noyaux** dans les trois processus. Ce résultat **n'établit pas** un gain du temps de calcul GPU isolé, une supériorité sur les meilleurs noyaux INT3/INT4, ni une stabilité inter-journalière. Des tentatives de réplication ont rencontré une fin de job GitHub avant publication d'artefact (HTTP 403) malgré des résultats arithmétiques corrects dans les logs ; ces tentatives restent des exécutions **échouées pour la qualification complète** [11].

### 5.4. Qualité numérique : résultat négatif sur Pythia-14M

Les RMSE sur les poids entraînés originaux sont les suivantes [12] :

| Matrice | BPB-3, 104 B/256 | BPB-3 + 2 résidus, 110 B/256 | Q3_K officiel, 110 B/256 | Q4_K officiel, 144 B/256 |
|---|---:|---:|---:|---:|
| Attention output | 0,002770944 | 0,002502617 | **0,001829436** | 0,000871819 |
| Attention QKV | 0,009519990 | 0,008765355 | **0,006123866** | 0,002866453 |
| MLP expansion | 0,006690600 | 0,006192494 | **0,004336688** | 0,002046186 |
| MLP projection | 0,006002291 | 0,005290174 | **0,003873590** | 0,001829517 |

À budget égal de 110 octets/256, le candidat BPB-3 corrigé est **36,6–43,1 % plus élevé en RMSE** que Q3_K selon la matrice. Les deux résidus diminuent le RMSE du BPB-3 non corrigé, mais **ne résolvent pas** son désavantage sur ce jeu de poids. Le format Q4_K a ici les RMSE les plus faibles, au prix d'un budget de stockage distinct de 144 octets/256. Les chiffres ne sont ni une mesure de perplexité ni une comparaison multi-modèles statistiquement concluante.

### 5.5. Variante distincte BPB3-K

BPB3-K [13] transpose les coefficients Q3_K vers quatre groupes de trois plans de 64 bits (96 octets), en conservant les 12 octets d'échelles de sous-groupes et les 2 octets d'échelle FP16 globale. Le total reste **110 octets/256 = 3,4375 bits/poids**. Le domaine entier devient [-4,+3].

Un transcodage **sans perte représentationnelle** des octets Q3_K et un décodeur Rust ont été testés contre le décodeur Rust Q3_K qualifié ; l'égalité des coefficients FP32 reconstruits est bit à bit dans les 25 cas indépendants déterministes déclarés. BPB3-K obtient donc la **même reconstruction** que Q3_K grâce à la conservation de ses échelles et de ses coefficients, **pas** grâce à un quantificateur devenu plus précis. Aucun noyau GPU BPB3-K ou gain DRAM BPB3-K n'est encore qualifié.

## 6. Discussion et lien avec FLAT-ATTENTION

**FLAT-ATTENTION** [14] est un moteur Rust d'attention fusionnée, organisé autour du calcul softmax(QKᵀ/√d)V sans matérialiser une matrice complète N×N de scores/probabilités. Il dispose d'un oracle scalaire, de chemins WGSL/WGPU et d'une feuille de route d'intégration SciRust ; la version du projet reste expérimentale et les jalons de production ne sont pas tous clos. Ce moteur traite un problème proche des algorithmes *IO-aware* [1], mais **BPB-3 n'est pas aujourd'hui intégré à un chemin attention end-to-end qualifié**.

Trois directions distinctes méritent évaluation :

1. **Poids quantifiés pour des projections liées à l'attention.** Tester des chemins de produits scalaires directement consommables sans expansion complète des poids, en intégrant le coût des activations non bipolaires et les variations d'échelle. Les résultats des microbenchmarks bipolaires ne peuvent être transférés tels quels aux projections Q/K/V d'un Transformer réel.
2. **Représentations comprimées pour certains segments K/V.** Une représentation quantifiée pourrait réduire les octets de certains buffers, mais il faut spécifier séparément clés et valeurs, compter toutes les métadonnées et démontrer l'effet sur sortie O, log-sum-exp, latence, qualité, caches et allocations. **Pas de promesse de déquantification nulle** : l'absence de matérialisation dense n'exclut pas des conversions en registres.
3. **Filtrage booléen de candidats.** FLAT étudie un frontal booléen sélectionnant des blocs/pages avant calcul exact. Ce programme relève d'un mécanisme **différent** du codec de poids BPB-3. Des tests de fidélité doivent prouver que la sélection ne rejette pas d'informations importantes. Les comptes logiques de blocs non visités ne prouvent ni économies physiques ni équivalence softmax.

L'hypothèse système formalisée par FLAT est : **T_filtrage + T_attention_survivants < T_attention_dense**, à qualité numérique contrôlée. Elle doit être testée sur des appareils et des modèles réellement représentatifs, avec repli dense explicite. Les programmes Multi-Algebra Attention, signatures F₂ et polynômes de Zhegalkin [14] sont des pistes apparentées, **non des résultats BPB intégrés**.

## 7. Menaces à la validité et résultats non acquis

**Validité interne.** Les entrées et poids des grands bancs sont synthétiques, les sorties sont comparées après quantification et la mesure DRAM inclut des événements matériels système. Les contrôles de bruit, de placement NUMA et d'identité de source limitent certains biais sans tous les éliminer. Certains essais ont été rejetés ; toutes les répétitions n'ont pas une qualification complète.

**Validité externe.** Un seul serveur Dell et un seul Apple M4 ne permettent pas d'extrapoler aux autres processeurs, accélérateurs ou modèles. Les tests GPU chronomètrent soumission et attente, pas une fenêtre noyau isolée. Le comparateur INT4 garde la plage [-3,+3], ce qui ne mesure pas la meilleure qualité d'un quantificateur INT4 exploitant toute sa plage.

**Validité numérique.** L'égalité bit à bit après transcodage BPB3-K ne garantit aucune qualité de génération de texte ni supériorité sur Q3_K. Les poids Pythia-14M couvrent peu de couches et un seul modèle ; leur TEST étant observé, toute nouvelle optimisation nécessitera un autre jeu de confirmation.

**Limites architecturales.** La seule opération booléenne testée concerne des entrées bipolaires. Une accumulation multi-échelle, des activations réelles, des corrections résiduelles, le backward, un cache KV complet et le calcul d'attention de bout en bout sont des chantiers non clos. Une réduction du trafic de poids ne démontre pas l'élimination du coût de lecture de toutes les données.

**Antériorité.** Les plans de bits, POPCOUNT, scales partagés et quantification ternaire sont des familles connues ; seule une analyse ciblée de la contribution et des comparateurs permettra d'apprécier l'originalité.

## 8. Programme expérimental prospectif

Les étapes suivantes ne doivent pas être présentées comme des résultats :

**E3 — Précision à budget fixé.** Préenregistrer sur DEV des variantes à sous-échelles, codebooks, résidus et tailles de bloc ; intégrer tout en-tête, index, alignement et padding. Qualifier sur de nouveaux checkpoints et couches TEST gelés, différents de Pythia-14M déjà examiné. Rapporter RMSE, erreurs de dot, p95/p99/p99.9, intervalles de confiance appariés et quantiles par couche.

**E4 — Noyaux BPB3-K compétitifs.** Écrire un décodeur booléen CPU SIMD et un chemin WGPU/Metal recevant directement les bits de BPB3-K et ses échelles, sans conversion dense cachée. Comparer au code Q3_K de référence optimisé, au même budget et aux mêmes entrées numériques ; mesurer séparément lecture, latence, coût de conversion et ressources CPU/GPU.

**E5 — Réplications multi-appareils.** Reproduire les expériences iMC sur différentes architectures et fenêtres de charge contrôlées ; employer lorsque possible des compteurs GPU et une métrologie énergétique qualifiée. Enregistrer tous les résultats négatifs.

**E6 — Intégration modèle et attention.** Construire un cas d'utilisation réel de projection ou de cache K/V avec oracle dense, activations non bipolaires, données d'évaluation gelées, précision/qualité de modèle, RSS/VRAM, latence, débit et énergie séparément. BPB/FLAT ne sera promu que si les critères du chemin concerné sont satisfaits.

**E7 — Revue d'antériorité.** Comparer formellement le codage d'échelle, les bitplanes, les noyaux SWAR/POPCOUNT et les métadonnées à BitNet, ggml, MX et aux formats voisins ; supprimer toute revendication de nouveauté non démontrée.

## 9. Reproductibilité, sources exactes et périmètre des assertions

Les preuves primaires résident dans les référentiels publics de code et les exécutions GitHub Actions ci-dessous. Les commits immuables servent d'identifiants ; une branche mobile ne vaut pas preuve à elle seule.

- **Spécification BPB-3/v0 et oracle :** SML-GENIUS [6], PR #837 et #839 ; source scientifique épinglée des mesures Dell : **200ba411a7f33962d15d0746d2e64382cfb7244d**.
- **Vérification 256m par bloc :** RemoteOps [8] ; une campagne antérieure 256m par sommes globales ne vaut pas cette qualification.
- **Instrumentation corrigée dual-socket :** RemoteOps PR #1337, #1338, #1339, runs **42358417643** et **42361052210** ; protocole de lecture matérielle et rejets détaillés dans [9].
- **GPU portable :** FLAT-ATTENTION PR #364 et #365 ; source GPU physique quatre bras **fe79917fa0bc1951753f78b83a640d8e2305e1fc**.
- **GPU Apple M4 :** RemoteOps run **42380329491**, Apple M4/Metal, trois processus, 15 mesures/bras/session [11].
- **Poids entraînés :** SML-GENIUS PR #855 ; run **42393428193** ; Pythia-14M révision **94f7c35d5e9f2e9bac8ca839329f505b4d007d5d**, fichier modèle SHA256 **116a02532db461f91386a5b20f942ff2c8d4de7341e21b55caafc3d7b25f49a1** ; GGML épinglé à **f7591a6a01f9273de03331732bdc0b39721168be**.
- **BPB3-K :** SML-GENIUS PR #856, protocole et transcodage [13] ; il s'agit d'une autre représentation que BPB-3/v0.
- **Gouvernance et maturité :** SML-GENIUS AGENTS.md et feuille de route scientifique ; FLAT-ATTENTION AGENTS.md, ROADMAP.md et overlays off-main ; aucun résultat d'une CI logicielle n'est automatiquement assimilé à une performance physique ou à une qualité de modèle.

**Note de méthode.** Les chiffres historiques sont repris des rapports, artefacts et journaux identifiés ci-dessus ; la rédaction de ce manuscrit n'a pas constitué une campagne supplémentaire de mesures. Pour une soumission, archiver les artefacts bruts et scripts reproductibles indépendamment de leur politique de rétention GitHub Actions, produire les intervalles de confiance et faire répliquer le protocole par une équipe distincte.

## 10. Conclusion

BPB-3/v0 offre un contrat précis pour effectuer des produits scalaires entiers avec entrées bipolaires à partir de plans de bits et d'une échelle commune. Les mesures sur un serveur Dell indiquent, dans six configurations acceptées, une diminution notable des lectures DRAM par rapport à certains noyaux INT8 SIMD. Les essais Apple M4 établissent la correction de noyaux GPU exploratoires, sans preuve de supériorité matérielle généralisée. Les essais sur vrais poids entraînés montrent en revanche que BPB-3/v0, même corrigé de deux résidus à budget égal, reconstruit moins bien les poids que Q3_K. La variante BPB3-K préserve exactement la reconstruction Q3_K par transcodage booléen, mais son intérêt physique reste à mesurer. Ainsi, les contributions crédibles à ce stade concernent **l'exactitude arithmétique, la représentation et le trafic mémoire sur des charges bornées**. Une amélioration de l'attention FLAT ou de l'inférence d'un modèle complet demeure **une hypothèse ouverte, non une conclusion**.

## Références

[1] T. Dao, D. Y. Fu, S. Ermon, A. Rudra, C. Ré. « FlashAttention: Fast and Memory-Efficient Exact Attention with IO-Awareness ». *NeurIPS*, 2022. https://proceedings.neurips.cc/paper/2022/hash/67d57c32e20fd0a7a302cb81d36e40d5-Abstract-Conference.html

[2] Posit Working Group. *Posit Standard 2022*. 2 mars 2022. https://posithub.org/docs/posit_standard-2.pdf

[3] H. Wang et al. « BitNet: 1-bit Pre-training for Large Language Models ». *Journal of Machine Learning Research*, 26(125), 2025. https://www.jmlr.org/beta/papers/v26/24-2050.html

[4] B. Darvish Rouhani et al. *OCP Microscaling Formats (MX) Specification*, v1.0, septembre 2023. Open Compute Project. https://www.opencompute.org/documents/ocp-microscaling-formats-mx-v1-0-spec-final-pdf

[5] ggml-org, *llama.cpp* / ggml, fichiers ggml-common.h, ggml-quants.c et gguf-py/gguf/quants.py, révision figée f7591a6a01f9273de03331732bdc0b39721168be. https://github.com/ggml-org/llama.cpp/tree/f7591a6a01f9273de03331732bdc0b39721168be

[6] Memorithm, « BPB-3/v0 — Boolean Posit Block », contrat du format et code prototype, 6–7 novembre 2026. https://github.com/Memorithm/SML-GENIUS/blob/main/research/boolean-posit/README.md ; https://github.com/Memorithm/SML-GENIUS/pull/837

[7] Memorithm/SML-GENIUS, PR #839, oracle indépendant par bloc et tests de compensations d'erreurs, 6 novembre 2026. https://github.com/Memorithm/SML-GENIUS/pull/839

[8] Memorithm/RemoteOps, PR #1309, qualification des corpus Dell par bloc. https://github.com/Memorithm/RemoteOps/pull/1309 ; run de validation identifié dans https://github.com/Memorithm/SML-GENIUS/pull/840

[9] Memorithm/RemoteOps, PR #1337–#1339, instrumentation complète bi-socket, matrice de mesures et commentaire de résultats du 7 novembre 2026. https://github.com/Memorithm/RemoteOps/pull/1337 ; https://github.com/Memorithm/RemoteOps/pull/1339#issuecomment-6705252869 ; https://github.com/Memorithm/RemoteOps/actions/runs/42361052210

[10] Memorithm/FLAT-ATTENTION, PR #364 et #365, comparateurs GPU BPB-3/INT3/INT4/INT8. https://github.com/Memorithm/FLAT-ATTENTION/pull/364 ; https://github.com/Memorithm/FLAT-ATTENTION/pull/365

[11] Memorithm/RemoteOps, « BPB-3 — qualification d'inférence sur le Mac mini physique », 7 novembre 2026 ; run 42380329491. https://github.com/Memorithm/RemoteOps/blob/main/docs/BPB3_NATIVE_MAC_METAL.md ; https://github.com/Memorithm/RemoteOps/actions/runs/42380329491

[12] Memorithm/SML-GENIUS, PR #855 et issue #853, reconstruction sur vrais poids entraînés Pythia-14M, 7 novembre 2026. https://github.com/Memorithm/SML-GENIUS/issues/853#issuecomment-6710236853 ; https://github.com/Memorithm/SML-GENIUS/actions/runs/42393428193

[13] Memorithm/SML-GENIUS, « BPB3-K: equivalent GGUF Q3_K numeric reconstruction in Boolean bitplanes », PR #856, 7 novembre 2026. https://github.com/Memorithm/SML-GENIUS/blob/main/research/boolean-posit/BPB3_K_EQUIVALENCE.md ; https://github.com/Memorithm/SML-GENIUS/pull/856

[14] Memorithm/FLAT-ATTENTION, README, ROADMAP et « Multi-Algebra Attention », version consultée au 7 novembre 2026. https://github.com/Memorithm/FLAT-ATTENTION ; https://github.com/Memorithm/FLAT-ATTENTION/blob/main/ROADMAP.md ; https://github.com/Memorithm/FLAT-ATTENTION/blob/main/docs/research/MULTI_ALGEBRA_ATTENTION.md

## Annexe A — journal de révisions

| Version | Date | Statut | Modifications |
|---|---|---|---|
| **0.1.0** | 2026-11-07 | Manuscrit initial | Contrat BPB-3, preuves CPU/GPU, résultat négatif sur Pythia-14M, variante BPB3-K, distinction des implications FLAT, références et protocole de révision |

**Règle :** conserver les résultats négatifs et les anomalies instrumentales ; ne jamais réécrire le statut historique d'un test après correction de son protocole. Lorsqu'une nouvelle expérience change une conclusion, ajouter une section *Réévaluation* avec ancien et nouveau protocole, SHA, mesures et justification. Ne pas présenter le manuscrit comme soumis, évalué par pairs ou publié sans événement réel correspondant.

## Annexe B — éléments à compléter avant une soumission

- Identité, ordre et contributions des auteurs ; affiliations et accord explicite de chacun.
- Licence du texte, disponibilité durable du code, des poids et des artefacts ; éventuelle déclaration de conflits d'intérêts.
- Graphiques avec intervalles de confiance et protocole statistique indépendant.
- Reproduction multi-machines et confrontation aux meilleurs noyaux quantifiés publiquement disponibles.
- Corpus de poids d'au moins un nouveau modèle et TEST final non observé.
- Mesure de qualité de tâches d'IA et, pour FLAT, de sortie O/LSE d'attention.
- Résultats négatifs complets, métadonnées matérielles, environnement logiciel et scripts de reconstruction du papier.
- Revue approfondie d'antériorité avant toute revendication de contribution originale.
