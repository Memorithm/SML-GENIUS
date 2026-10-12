# A6-I4 — Résultats natifs : support ANF par paires, bénéfices et faille triple obligatoire

**12 octobre 2026. Protocole préenregistré, expérience entièrement exécutée sur GitHub-hosted, résultats positifs et contre-exemples négatifs conservés.** Le [préenregistrement](A6-I4-PAIR-SUPPORT-TRIPLE-PREREGISTRATION.md) a été commité avant code ou résultats au SHA `e2c4dd287ac6c9d035b2f00016b4126f8f82452e`. Première campagne complète : [GitHub Actions `epistemic-pair-support`, run 38263623244 / job 114860143528](https://github.com/Memorithm/SML-GENIUS/actions/runs/38263623244/job/114860143528), source `ad2c6195e78fbd9fcfb022c9914b4b80ab52b62d`, **SUCCESS** avec tests natifs, compilation, `rustfmt`, Clippy `-D warnings`, 252 ajustements gelés et 516 096 prédictions TEST. Le contrôle sur le SHA final incluant ce rapport reste nécessaire avant merge.

## 1. Avancée et limite : ce que fait réellement le composant Rust

A6-I3 retenait seulement `seen(0)` et `seen(1)` par variable de l'entrée passive, échouant dès que `(1,1)` de deux variables jamais coactivées apparaissait en déploiement. Le nouveau module Rust public `sml_core::anf_pair_support::AnfPairSupportGuard` mémorise la **couverture de toutes les combinaisons binaires de chaque paire** d'indices d'entrée, indépendamment des labels.

Le garde utilise uniquement les **512 inputs TRAIN**. Il les conserve en colonnes de 512 bits, emballées dans `u64`, puis `seal()` effectue une intersection de colonnes sur les quatre patterns de chaque paire `i<j`, créant une liste **compacte des paires insuffisamment couvertes et des patterns absents**. À chaque input OOD, `assess()` ne vérifie que cette liste et produit le nombre de paires dont la combinaison de bits est nouvelle. C'est une alarme de support empirique, **pas** la reconstruction d'une fonction ANF, une mise à jour des hypothèses de la banque ni une garantie de bonne prédiction.

`predict_guarded()` réutilise réellement le `PassiveCommittee` bayésien d'A6-I2. Une prédiction n'est émise que si : (1) le comité attribue une confiance ≥0,90 ; (2) le meilleur modèle de la banque présente une erreur TRAIN ≤1/8 ; (3) **toutes les combinaisons des paires** dans l'input OOD ont déjà été observées. Le garde ne lit ni la vraie structure du professeur ni les étiquettes TEST, et ne modifie pas le comité. Historique des deux objets exigé synchronisé ; appels invalides et budgets insuffisants rejettent avant toute mutation.

Les ressources sont explicitement bornées : `width≤1024`, `max_train=512`, `max_pair_slots=600000` (k1024 donne 523776 paires possibles), `max_missing_pairs=8192`, allocation `try_reserve`. La mémoire logique mesurée contient les colonnes et les paires manquantes compactes; elle exclut headers/capacités `Vec`, temporaires de scellement, le comité, l'allocateur et le RSS. Le recalcul O(k² × ceil(n/64)) des supports s'effectue **une fois** avant TEST, pas à chaque token; l'évaluation OOD coûte le nombre de paires **manquantes**, dont la valeur peut être élevée dans des distributions fortement censurées.

## 2. Corpus comparatif gelé, sans fuite du professeur

Le professeur natif et la banque de quatre `WideAnfGate` sont identiques à A6-I3 sur les scénarios pairwise. Les candidats sont fournis, non appris :

- `h0=x[r2] XOR x[r3]`;
- `h1=h0 XOR x[r0]x[r1]`, `h2=h0 XOR x[r4]x[r5]`;
- `h3=h0 XOR x[r0]x[r1] XOR x[r4]x[r5]`.

Le vrai professeur est soit `h0` (banque complète pour lui), soit un enseignant **absent de la banque** `g=h0 XOR x[r6]x[r7]` ou `g=h0 XOR x[r5]x[r6]x[r7]`. Les `r0..r7` sont permutés par graine parmi toutes les `k` positions et cachés du support guard. Un prior fixe `[4,2,2,1]/9` et une vraisemblance avec taux nominal de bruit 1/16 sont employés, pas de nouveau calcul de probabilité.

**Effectifs** : 7 largeurs `k∈{8,16,32,64,128,256,1024}` ×3 graines×6 régimes×2 panneaux TRAIN propre/bruité = **252 politiques**, appariées en 126 couples de distributions d'inputs selon le panneau. Chaque politique observe 512 échantillons **avant** que la première entrée TEST ne soit produite; une fois `seal()` appelé, aucune nouvelle observation n'est acceptée. Les **252×2048 = 516096** valeurs TEST sont générées indépendamment de TRAIN mais appariées entre les deux panneaux de bruit. Chaque régime contribue **42 modèles et 86016 contextes TEST**, dont 43008 déclenchent une intervention pair/triple hors support TRAIN.

## 3. Résultats exacts : taux d'erreur compté seulement sur les prédictions émises

| Régime TRAIN → TEST | Comité accepté / erreurs | Garde marginal accepté / erreurs | **Garde pairwise accepté / erreurs** | Contextes avec paire nouvelle | Banques TRAIN inadéquates |
|---|---:|---:|---:|---:|---:|
| CENSORED_ABSENT : valeur r6=1 inconnue | 86016 / **43008** | 43008 / **0** | **43008 / 0** | 43008 | 0/42 |
| PAIR_CENSORED_ABSENT : (r6,r7)=11 inconnue | 86016 / **43008** | 86016 / **43008** | **43008 / 0** | 43008 | 0/42 |
| PAIR_CENSORED_INCLUDED : paire absente mais h0 vrai | 86016 / **0** | 86016 / **0** | **43008 / 0** | 43008 | 0/42 |
| TRIPLE_CENSORED_ABSENT : (r5,r6,r7)=111 inconnue | 86016 / **43008** | 86016 / **43008** | **86016 / 43008** | **0** | 0/42 |
| FULL_INCLUDED : h0 dans la banque et inputs couverts | 86016 / 0 | 86016 / 0 | **86016 / 0** | 0 | 0/42 |
| FULL_ABSENT : professeur absent observablement inadéquat | 86016 / **43008** | 0 / 0 | **0 / 0** | 0 | **42/42** |

Les **252/252** sous-configurations satisfont leurs critères fixés, y compris l'échec triple obligatoire, et l'intégralité des sommes correspond aux 6×86016 tests. Il faut garder séparés **la couverture** (fraction d'inputs auxquels la politique répond), **l'erreur parmi les prédictions acceptées**, et la nature du problème (nouveauté marginale, paire jamais coactivée, degré trois invisible, ou banque inadéquate). Une abstention n'est jamais comptée comme bonne réponse.

### H1 — Le garde par paires corrige le piège d'A6-I3

Sur `PAIR_CENSORED_ABSENT`, chaque valeur individuelle des deux variables a été vue à l'entraînement. Le garde marginal n'enregistre donc aucune nouveauté et émet **86016 prédictions, dont 43008 fausses avec forte confiance**. Le garde pairwise détecte le motif conjoint 11 absent, refuse ces 43008 inputs et continue à répondre aux **43008 autres avec zéro erreur**. Ce résultat valide **ce problème pairwise ciblé**, pas tous les changements OOD imaginables.

### H2 — La protection a un coût réel

Sur `PAIR_CENSORED_INCLUDED`, la vraie fonction est `h0` et appartient à la banque; **aucune prédiction du comité n'est fausse**, mais le garde pairwise refuse **43008 des 86016 entrées**, uniquement parce que la paire 11 était absente de TRAIN. **43008 bonnes réponses potentielles ont été perdues.** L'absence de couverture du support n'est pas, par elle-même, une preuve de divergence du professeur.

### H3 — Échec d'ordre trois obligatoire, exactement mesuré

L'enseignant absent `g=h0 XOR x[r5]x[r6]x[r7]` est vu en TRAIN uniquement sur les **sept configurations sauf 111** des trois variables cachées; les **quatre** configurations de chacune des trois paires sont néanmoins observées au moins une fois. Ainsi `h0=g` sur **tout** le support observé, y compris avec labels Bernoulli1/16 bruités, et **la couverture par paires est parfaite**. En TEST, le nouveau triplet111 devient observable sur la moitié des contextes.

Le garde pairwise annonce **0 paire nouvelle**, accepte **86016/86016** réponses et produit **43008 erreurs confiantes**, soit **50% d'erreur conditionnellement aux prédictions acceptées** dans ce régime. C'est la réfutation **exigée par le protocole**, pas un échec logiciel corrigible par un seuil marginal. Sans une observation triple, la présence du coefficient ANF de degré trois est **informationnellement indiscernable** sur TRAIN. Aucun système de mémoire par paires ne peut faire apparaître une preuve qu'il n'a pas reçue.

### H4 — Régression et adéquation de banque

Sur `CENSORED_ABSENT`, le nouveau garde conserve le gain marginal antérieur et n'émet aucune erreur parmi 43008 réponses. Sur `FULL_INCLUDED`, **86016/86016** réponses acceptées, zéro erreur et zéro nouveauté. Sur `FULL_ABSENT`, l'interaction manquante est active dans les inputs TRAIN ; les 42 évaluations de la banque dépassent le seuil d'erreur maximal `1/8`, et le garde s'abstient sur **toutes** les valeurs TEST, sans attribuer de coordonnées connues à un professeur inexistant.

## 4. Coût mémoire : 64 KiB pour les colonnes à k1024, pas O(1) général

À `k=1024`, les 512 observations par entrée sont retenues dans `1024 × 8 u64` = **65536 octets / 64 KiB de payload historique**. La couverture complète et le scénario triple ne créent **aucun enregistrement de paire manquante**, donc la valeur reste 65536 octets. Le scénario PAIR_CENSORED crée exactement **un enregistrement manquant** par run de k1024 : 65560 octets de payload total (24 octets par enregistrement dans l'ABI mesurée). Le scénario CENSORED_ABSENT crée **1023 entrées manquantes** : 90088 octets. Le maximum observé est 1023 paires manquantes, inférieur au budget fixe 8192.

Moyenne du temps `seal()` sur les 42 versions width/seed/panel d'un même régime : **~0,25–0,27 ms** sur le runner CPU Rust RELEASE, après compilation des colonnes TRAIN. Ces chiffres décrivent les instances synthétiques et l'ABI x86_64, non une borne sur le coût pour une banque massive, un GPU Thor ou une fonction arbitrablement rare. La phase TEST parcourt la liste de paires absentes; le pire cas peut nécessiter O(k²) travail/mémoire et une censure extrême est rejetée sous quota.

## 5. CI, reproductibilité et décision

Premier run dédié complet [GitHub Actions `epistemic-pair-support`, run 38263623244, job 114860143528](https://github.com/Memorithm/SML-GENIUS/actions/runs/38263623244/job/114860143528), source SHA `ad2c6195e78fbd9fcfb022c9914b4b80ab52b62d`, **SUCCESS** : `cargo fmt --all -- --check`, `cargo check --locked -p sml-core --all-targets`, `cargo clippy --locked -p sml-core --all-targets -- -D warnings`, tests unitaires (dont le **triple invisible malgré toutes ses paires connues**) et RELEASE 252 FROZEN +252 TEST+6 SUMMARY+COMPLETE. Le premier passage CI a seulement révélé les ajustements de formatage Rust, corrigés sans changer graines, fonctions, labels, populations ni critères scientifiques.

Reproduction native :
```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --lib anf_pair_support
cargo test --locked -p sml-core --bin sml-epistemic-pair-support-bench
cargo run --locked --release -p sml-core --bin sml-epistemic-pair-support-bench
```

Runner GitHub-hosted Ubuntu24.04, Rust1.89 épinglé, checkout SHA exact sans identifiants persistants, `permissions:contents:read`, aucun runner RemoteOps privé, bibliothèque GPU NVIDIA propriétaire ou code `unsafe`.

**Critères de démonstration H1–H4 : tous satisfaits, incluant explicitement le constat d'une défaillance à l'ordre trois.** Cette expérience ne démontre pas une « intuition humaine », un posterior infaillible, une couverture absolue de la banque, l'apprentissage de nouveaux monômes ni les performances SML-25B. Elle démontre que l'amélioration de couverture **du premier vers le deuxième ordre déplace la frontière de non-identifiabilité**, sans l'annuler. Les prochaines recherches doivent quantifier le coût d'une mémoire d'interactions d'ordre 3 puis des dépendances hiérarchiques sous budget, avec **un contre-exemple obligatoire d'ordre 4**, et confronter le comité à une banque réellement enrichie après détection du support manquant.
