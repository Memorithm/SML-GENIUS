# A6-C12g / A6-C12h — Résultats mesurés, récupération sous bruit et frontière de rupture (12 octobre 2026)

**Statut : QUALIFICATION SCIENTIFIQUE LIMITÉE AUX DEUX ENSEIGNANTS SYNTHÉTIQUES.** Après les incidents de provisionnement, les workflows GitHub-hosted ont finalement réussi. Ce rapport est **postérieur aux protocoles gelés** de [C12g](../A6-C12G-NOISE-SAMPLE-PHASE-PREREGISTRATION.md) et [C12h](../A6-C12H-EXTREME-NOISE-THEORY-PREREGISTRATION.md) ; il ne réécrit ni leurs hypothèses, ni le budget, ni les graines, ni les algorithmes. La première preuve d'échec du runner ne doit plus être présentée comme le dernier état du projet.

## Traçabilité vérifiée sur les artefacts complets, non sur les intitulés de PR

| Protocole | Code de la campagne (HEAD exact) | GitHub Actions successful | Jobs réels |
|---|---|---|---|
| A6-C12g | `f73a7fc5160c996901d7afd824a4015c990ce125` | [run 38292779849](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292779849) | 114960270546, 114960270597, 114960270859 |
| A6-C12h | `6fe78c4e75c031b85612f684cb71237417155720` | [run 38292873509](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292873509) | validation 114960667341 ; mesures 114960942799, 114960942839, 114960942920 |

Les deux workflows ont terminé leurs contrôles prévus, y compris `fmt`, `cargo check`, Clippy strict, les tests et trois exécutions complètes des données. Les **blobs de chacun des deux bancs Rust sont identiques sur `main` et sur le HEAD qualifié** : C12g `ab1e3317cda7fe9e5b14a18487a067eb090f78c3` et C12h `3f268fed0a9fe1099e9b385046d79d06ab8bfd63`. L'exécution `main` [38294525153](https://github.com/Memorithm/SML-GENIUS/actions/runs/38294525153) a également passé les vérifications Rust générales après fusion #512.

| ZIP GitHub Actions (30 jours de conservation prévus) | Artifact ID | SHA256 du ZIP contrôlé |
|---|---:|---|
| C12g réplica 1 | [11766784470](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292779849/artifacts/11766784470) | `13dddd57fc2588c63df1be84ce717a74ba8d6983ea897f96948c8dd3becf8a52` |
| C12g réplica 2 | [11767264205](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292779849/artifacts/11767264205) | `d4b66f6d31665248057d40409dda16b347c6610f3199ab993e277dca19877c45` |
| C12g réplica 3 | [11767524061](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292779849/artifacts/11767524061) | `fad2fd1bed56a53af26acdefedc8603eda48105a8294c6458183ec1dd19a5ff2` |
| C12h réplica 1 | [11766794693](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292873509/artifacts/11766794693) | `17f6bb0a014cb3af0be1e34aa87bd9f0fee86ec112102ad7fe53bbb7bff3d841` |
| C12h réplica 2 | [11768015380](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292873509/artifacts/11768015380) | `a98665262627affc6de90f29dd43600b92c21e643431f24d11c35165505fb2a3` |
| C12h réplica 3 | [11767394481](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292873509/artifacts/11767394481) | `75b3699a4d4bf7820dfdce9c0eafadfebcbe21663cb3db9dc64a51d96af2a240` |

**Audit indépendant des calculs exportés :** extraction locale des six archives ZIP complètes, recomptage par type de ligne, correspondance bijective `FROZEN`↔`TEST` sur `(task,seed,rate,words,method)`, recomptage des `native_success` dans chaque `SUMMARY` et ordre de gel avant le premier TEST. C12g : **1232 FROZEN, 1232 TEST, 154 SUMMARY, 22 THRESHOLD** par réplica ; C12h : **1872 FROZEN, 1872 TEST, 180 SUMMARY, 18 THRESHOLD, 8 THEORY_THRESHOLD** par réplica. Les contenus non temporels de toutes les lignes ont été comparés entre les trois réplicas de chaque protocole : **égalité exacte, champ par champ**, les durées variant selon le runner. De plus **560 configurations identiques** partagées entre C12g et C12h produisent les mêmes `native_success`, paramètres proposés et comparaisons bit-à-bit. Les 3 réplicas sont donc **3 répétitions d'exécution, pas 3 échantillons statistiques indépendants** ; ne jamais tripler les dénominateurs Wilson.

Le tableau complet des **334 lignes agrégées** (incluant succès, Wilson, seuil heuristique et temps des 3 réplicas) est dans [A6-C12GH-PHASE-SUMMARY.csv](../../research/results/A6-C12GH-PHASE-SUMMARY.csv). Le programme Rust `sml-c12gh-phase-report-audit` recontrôle hors-ligne les effectifs, les 8/16 graines, les formules et les contre-exemples.

## Seuils empiriques A6-C12h — strict succès natif autonome sur TEST

Chaque seuil ci-dessous est le **premier budget préenregistré** atteignant au moins 7/8 ou 14/16 succès, selon que cette case avait 8 ou 16 graines. Valeurs en **bits proxy** (`words ×64`) ; `—` signifie « seuil non atteint jusqu'à 32768 bits », et NON impossibilité absolue. Les budgets et graines sont emboîtés/corrélés, et la confiance Wilson est large.

| Enseignant | Bruit étiquettes | Walsh-3 | Hybride W2+MITM | Crossover heuristique Walsh (bits) |
|---|---:|---:|---:|---:|
| XOR→XOR→XOR | 0 % | **64** | 64 | 27 |
| XOR→XOR→XOR | 20 % | **256** | — | 206 |
| XOR→XOR→XOR | 30 % | **2048** | — | 1044 |
| XOR→XOR→XOR | 40 % | **16384** | — | 16701 |
| AND→XOR→XOR | 0 % | **128** | **64** | 107 |
| AND→XOR→XOR | 20 % | **1024** | **512** | 825 |
| AND→XOR→XOR | 30 % | **8192** | **4096** | 4175 |
| AND→XOR→XOR | 40 % | — | — | 66806 |

Le témoin Gauss propre réussit sur XOR à partir de **128 bits** (2 mots), mais **n'atteint jamais** le seuil sur AND non affine. Le seuil théorique `N*=2 ln(2M)/a²`, `M=317440`, avec `a=ρ` pour XOR et `a≈ρ/2` pour AND, suppose artificiellement des faux scores de type gaussien comparables. Le proxy mélange deux étiquettes bruitées : `η_proxy=2η(1−η)`, `ρ=(1−2η)²`. Il **ne prédit ni une borne de récupération ni les performances du facteur ANF**. Le rapport à ce seuil peut varier notablement, comme sur AND 30 % (Walsh observé 8192 bits contre 4175 prédits).

### Coupes critiques, sorties réellement mesurées

- **XOR, bruit 20 % :** Walsh-3 obtient **16/16 succès** à 256 bits ; le W2+MITM plafonne à **2/8** au budget maximal de 32768 bits (et 4/16 à 256 bits). Dans ce régime de parité, l'absence d'une forte paire est un goulet d'étranglement empirique du filtrage hybride. Ce constat ne prouve pas que tout algorithme pairwise est intrinsèquement incapable de découvrir la parité.
- **XOR, bruit 30 % :** Walsh-3 passe de **8/16 à 512 bits**, **13/16 à 1024 bits**, puis **16/16 à 2048 bits** ; le W2+MITM reste à **0/8 à 32768 bits**. Les succès associés à différents budgets/graines sont corrélés.
- **XOR, bruit 40 % :** Walsh-3 réalise **7/16 à 8192 bits**, **14/16 à 16384 bits** et **16/16 à 32768 bits** ; W2+MITM reste **0/16** à 32768 bits.
- **AND, bruit 30 % :** à 2048 bits, Walsh-3 **5/16**, hybride **13/16** ; à 4096 bits, respectivement **13/16** et **14/16** ; à 8192 bits, **16/16** pour les deux.
- **AND, bruit 40 % :** à 32768 bits, Walsh-3 **8/16**, IC Wilson 95 % **[0,280 ; 0,720]**, contre hybride **11/16**, IC **[0,444 ; 0,858]**. Le seuil 14/16 n'est atteint par aucun. L'écart de succès est descriptif, pas une supériorité statistique universelle.

Sur les moments de Walsh-3, la marge vraie−meilleur faux (parmi les top16 candidats réellement évalués) progresse avec les observations : pour XOR 40 %, marge médiane **−0,009** à 8192 bits, **+0,006** à 16384 bits, **+0,014** à 32768 bits ; pour AND 40 %, marge médiane **−0,004** même au maximum. L'amplitude du score vrai a été ramenée à l'écart-type de référence nulle `√N` ; **ce n'est ni un z-score calibré après sélection ni une p-value**, surtout avec candidats recouvrants et top16 choisi sur TRAIN. Les intervalles de Wilson sont exploratoires, pas corrigés pour les multiples seuils testés.

## Contrôles, coût, réfutations

L'intégralité des TEST a été générée après le gel des modèles. La sortie `native_success` requiert structure et tables de portes exactes, validation acceptée et **zéro désaccord des prédictions natives contre l'enseignant sur TEST autonome** ; ce n'est pas une simple égalité de taux d'erreur. Les taux de bruit concernent les **étiquettes**, pas les entrées ni l'état interne. Les timings `screen`, `factor` et `total` conservent les 3 répétitions internes et les trois moyennes robustes propres aux runners ; **pas de conclusion de vitesse matérielle générale** sur machines GitHub partagées. Les trois campagnes réussies ne créent pas 3 nouvelles populations de graines.

Les échecs sont des **résultats attendus et conservés** : hybrides impuissants sur la parité bruitée avec leur budget de propositions, clean Gauss non affine, AND 40 % censuré par le budget, large incertitude à 8 graines. Ils ne doivent être ni omis ni « réparés » rétroactivement dans les hypothèses gelées. La correspondance ordre de grandeur Walsh-3/théorie dans certaines cases est une observation contrôlée, **pas** une théorie démontrée pour LPN, architectures k4+, 508 sources, longues dépendances naturelles ou grands bancs ANF.

**Conclusion modèle réel inchangée : STOP.** Sur le corpus réel de l'étude [A6-E1](../RESULTS-A6-E1-REAL-BYTE-SUPPORT-RISK.md), le comité ANF non calibré était nettement inférieur au témoin ngram4. Aucun résultat C12g/C12h n'autorise l'annonce d'un SML-2/25B de qualité linguistique, d'une intuition humaine émergente, d'une banque 1M fonctionnelle ou d'une accélération GPU.

## Travail scientifique suivant : nouveau protocole AVANT nouveau TEST

Une étude séparée doit mesurer sans oracle un sélecteur **TRAIN/VAL seulement** entre criblage pairwise rapide et contrôle spectral d'ordre 3, ainsi qu'un véritable second niveau indépendant (nouvelles graines, fonctions/topologies et bruit d'état distinct), avec contrôles à coût égal, échecs obligatoires et comparaisons probabilistes sur données réelles. Pas de retouche des taux/seuils de C12g/h en fonction des résultats ci-dessus.
