# A6-I3 — Résultats : garde de support marginal, confiance sous décalage et piège des paires invisibles

**12 octobre 2026 — expérience préenregistrée, première exécution complète native Rust, succès des critères et observation négative obligatoire.**

Protocole initial commité **avant le premier code et les données** au SHA `81232567208876d182cf08c5326fe82ea19c2434` : [A6-I3-EPISTEMIC-SUPPORT-PREREGISTRATION.md](A6-I3-EPISTEMIC-SUPPORT-PREREGISTRATION.md). Premier run complet : source `bd75897e8ae351927e8292cf4d378acf97b1de39`, [GitHub-hosted `epistemic-support` run 38252571901, job 114822458144](https://github.com/Memorithm/SML-GENIUS/actions/runs/38252571901/job/114822458144). CI dédiée **SUCCESS** : fmt, check, Clippy strict, tests natifs et les 210 politiques suivies des 430080 labels TEST. Le run général du dépôt doit encore être vert au **dernier SHA documentaire** pour autoriser le merge.

## 1. Résultat scientifique en une phrase

Une enveloppe univariée de valeurs booléennes vues pendant TRAIN **élimine toutes les erreurs confiantes provoquées par une variable jamais activée**, mais **n'élimine aucune erreur provoquée par une paire de variables déjà observées séparément et jamais co-activées**. Elle peut également rejeter des prédictions entièrement correctes. L'algorithme est une alarme de support empirique **et non une garantie de justesse**, même si une banque de fonctions ANF déclare une probabilité proche de 1.

Le composant public ajouté est `sml_core::anf_marginal_support::AnfMarginalSupportGuard`. Il conserve, pour **chaque** position d'entrée, seulement deux indicateurs : valeurs 0 et 1 déjà vues. Cela demande exactement **k octets de payload logique** pour k coordonnées dans cette version Rust (une allocation `Vec<u8>`), hors structure et allocateur. Le garde ne stocke **aucun monôme vrai, label caché, couple observé, posterior ou mémoire externe**.

Une prédiction est autorisée seulement lorsque **les trois contrôles** suivants passent :
1. le vrai `PassiveCommittee` issu de A6-I2 retourne une prédiction sélective avec confiance ≥0,90 ;
2. le minimum des taux d'erreur des quatre candidats sur TRAIN ne dépasse pas **1/8**, `bank_adequate(0.125)` ;
3. l'input demandé n'active **aucune valeur univariée encore jamais observée** pendant TRAIN.

Les contrôles de tailles/compteurs échouent sans mutation. L'enveloppe ne change ni la banque des quatre `WideAnfGate` native, ni sa vraisemblance Bernoulli (bruit supposé 1/16), ni les données de TRAIN.

## 2. Construction auditable : quatre candidats, un professeur parfois absent

Sept largeurs k8/16/32/64/128/256/1024, trois graines, deux panels clean/noise1/16, cinq régimes = **210 modèles gelés** après chacun **512 observations passives**. Ces 210 fits se regroupent en **105 paramètres width×seed×régime appariés** sur les deux panels (et les k dimensions ne sont pas toutes réellement actives dans la banque).

Huit indices physiques `r0..r7` sont permutés de manière indépendante sur les k entrées ; le comité reçoit **quatre vrais candidats ANF** :
`h0=x[r2]⊕x[r3]` ;
`h1=h0⊕x[r0]x[r1]` ;
`h2=h0⊕x[r4]x[r5]` ;
`h3=h0⊕x[r0]x[r1]⊕x[r4]x[r5]`.
Le prior reste explicitement `[4,2,2,1]/9` : il est fixé au départ, pas appris à partir des enseignants cachés de TEST. La **vraie fonction** est soit `h0`, fournie, soit `g=h0⊕x[r6]x[r7]`, **absente de tous les quatre candidats**. Ni le garde ni `PassiveCommittee` ne reçoivent le nom de l'enseignant ; les labels du professeur sont générés à part.

Les cinq régimes TRAIN utilisent :
- **CENSORED_ABSENT** : r6=0 toujours, vérité g absente.
- **CENSORED_INCLUDED** : mêmes inputs r6=0, vérité h0 présente.
- **PAIR_CENSORED_ABSENT** : r6/r7 répètent les trois paires (0,0), (1,0), (0,1), jamais (1,1), vérité g absente.
- **FULL_INCLUDED** : r6/r7 varient sur les quatre paires, vérité h0 présente.
- **FULL_ABSENT** : mêmes variations complètes, vérité g absente et alors mesurable par les erreurs TRAIN.

Les autres dimensions sont tirées pseudo-aléatoirement ; le bruit de label du panel noisy est Bernoulli1/16 indépendant. Unité d'analyse : **42 fits** par régime (7 largeurs×3 graines×2 panels). La phase TEST démarre **uniquement après les 210 gels TRAIN et la détermination des 210 décisions d'adéquation**. Chacun reçoit 2048 inputs : **1024 entrées** avec r6=0 et r7 aléatoire, puis **1024 entrées** avec r6=r7=1, avec dimensions restantes aléatoires. Cela force une variation où g et h0 sont toujours égaux sur la première moitié, toujours opposés sur la seconde. Les panels bruit/noisy partagent la même génération de TEST pour un width/seed/régime : les 430080 lignes binaires sont donc **appariées**, pas 430080 observations indépendantes.

## 3. Résultats complets — refus, couverture et erreurs confiantes

Le premier run publie exactement **210 `A6_I3_FROZEN`**, **210 `A6_I3_TEST`**, **5 `A6_I3_SUMMARY`**, une ligne `A6_I3_ALL_FROZEN`, et **une `A6_I3_COMPLETE`**, avec `preregistered_failures=0`. Les comptes ci-dessous sont ceux des **5×86016 = 430080 prédictions par scénario**, agrégées sur les 42 fits de chaque régime. Une erreur est comptée seulement lorsque le prédicteur a émis une prédiction **acceptée**, pas lorsqu'il s'est abstenu.

| Régime | Comité : acceptées | Comité : erreurs | Enveloppe : acceptées | Enveloppe : erreurs | Contextes marginaux nouveaux | Banques inadéquates |
|---|---:|---:|---:|---:|---:|---:|
| **CENSORED_ABSENT** | 86 016 | **43 008** | **43 008** | **0** | 43 008 | 0/42 |
| **CENSORED_INCLUDED** | 86 016 | 0 | **43 008** | 0 | 43 008 | 0/42 |
| **PAIR_CENSORED_ABSENT** | 86 016 | **43 008** | **86 016** | **43 008** | **0** | 0/42 |
| **FULL_INCLUDED** | 86 016 | 0 | **86 016** | 0 | 0 | 0/42 |
| **FULL_ABSENT** | 86 016 | **43 008** | **0** | 0 | 0 | **42/42** |

**H1 — succès, mais seulement sous nouveauté marginale :** pour les 42 tâches avec enseignant g absent et r6 constamment zéro sur TRAIN, le contrôle de nouveauté rejette la moitié des TEST (r6=1) et émet **43 008 prédictions toutes correctes** sur l'autre moitié ; le comité natif sans cette enveloppe se trompait sur **43 008 de ses 86 016 prédictions très confiantes**, et ne levait aucune alerte d'inadéquation sur TRAIN. La propriété « g≡h0 lorsque r6=0 » est algébrique et vaut sur **tout** le domaine TRAIN censuré, pas seulement ces 512 échantillons.

**H2 — échec obligatoire révélateur :** dans PAIR_CENSORED_ABSENT, les valeurs 0 et 1 de r6 et r7 ont été **toutes deux observées individuellement** pendant TRAIN, mais jamais la paire (1,1). Le garde O(k) accepte **les 86 016 inputs OOD**, sans voir de nouveauté marginale, et commet **43 008 erreurs avec forte confiance** : exactement le même nombre que le comité non protégé. Cela démontre pourquoi la conservation des seuls supports univariés ne suffit pas à qualifier l'extrapolation de fonctions ANF dotées d'interactions.

**H3 — coût et bénéfice réel :** quand h0 **est** le professeur en CENSORED_INCLUDED, le garde refuse **43 008 prédictions qui auraient toutes été correctes**, uniquement parce que r6=1 était nouveau : **50 % de couverture sacrifiée**, sans avantage en erreur. Sous FULL_INCLUDED, aucune nouveauté marginale, **100 % couverture et 0 erreur** pour les deux méthodes.

**H4 — diagnostic complémentaire de banque manquante :** sous FULL_ABSENT, r6∧r7 s'active pendant TRAIN. Le comité fourni ne sait pas le représenter, mais son taux minimal de discordance d'entraînement dépasse le seuil fixe 1/8 **dans 42/42 cas** : l'enveloppe complète s'abstient donc sur **100 % des 86 016 TEST**, sans qu'une nouveauté des inputs soit nécessaire. Ce refus n'est pas une preuve de l'identité de la fonction manquante et ne reconstruit aucun monôme absent.

## 4. Portée algébrique des deux contre-exemples

Deux fonctions `h0` et `g=h0⊕x6x7` sont identiques sur toutes les entrées dont `x6x7=0`. Cela inclut les deux supports d'observation suivants :
- `x6=0` : le garde **peut** remarquer l'entrée x6=1 jamais vue et refuser ;
- `(x6,x7)∈{00,10,01}` : le garde ne voit rien d'anormal dans x6=x7=1, puisque chaque bit a déjà pris 0 et 1. Il existe néanmoins une **interaction non observée** entraînant une sortie contraire.

Le suivi des marges **ne contient aucune information permettant de distinguer ces deux situations**, même si sa mémoire TRAIN est parfaite. Le second contre-exemple est donc **structurel**, pas une défaillance d'optimisation ou un mauvais réglage des seuils de confiance. Le test d'une banque entièrement vraie FULL_INCLUDED est nécessaire pour mesurer le taux de faux refus.

## 5. Reproductibilité et limitations

Première mesure indépendante : [run natif GitHub Actions A6-I3](https://github.com/Memorithm/SML-GENIUS/actions/runs/38252571901/job/114822458144), source SHA `bd75897e8ae351927e8292cf4d378acf97b1de39`. Les étapes `cargo fmt --all -- --check`, `cargo check --locked -p sml-core --all-targets`, `cargo clippy --locked -p sml-core --all-targets -- -D warnings`, les tests de marge/paire invisibles et le banc RELEASE des 210 fits sont toutes **SUCCESS**. Le premier échec CI n'était qu'un écart de formatage `rustfmt`, corrigé via les hunks émis par Rust1.89 sur runner propre ; aucune modification des procédures, des graines, des taux ou des hypothèses après la génération TEST.

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --lib anf_marginal_support
cargo test --locked -p sml-core --bin sml-epistemic-support-guard-bench
cargo run --locked --release -p sml-core --bin sml-epistemic-support-guard-bench
```

L'exécution est **Rust sûr, locale au binaire de test et hébergée chez GitHub**, sans service résident ni SDK NVIDIA propriétaire, checkout du SHA exact en lecture seule. L'enveloppe demande **k octets de stockage logique de marginales** pour k bits d'input, hors overhead et banque de candidats déjà présente ; elle n'est pas une surveillance pairwise O(k²). Le benchmark n'a qualifié ni les séquences produites par un modèle de langage, ni l'apprentissage autonome des candidats, ni le vrai routage MOR/Delta-KV, ni des banques d'un million de portes.

**Décision expérimentale : H1–H4 passent leurs critères (dont la réussite de H2 est de constater un échec précis et non de le cacher).** Les conclusions s'appliquent à des fonctions synthétiques construites et à un décalage TEST volontairement marqué, **pas** à une calibration de probabilité universelle ou à l'intuition humaine. La prochaine voie scientifique pertinente est un suivi **des interactions observées** sous budget mémoire avec des audits probabilistes de couverture, en conservant un contre-exemple d'ordre supérieur — aucun filtre finite-order n'est suffisant sur une ANF de degré arbitraire.
