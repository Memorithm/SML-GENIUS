# A6-WL2 — Résultats : découverte de quatre monômes ANF sans sélecteurs connus

**12 octobre 2026 — banc complet exécuté et conforme au préenregistrement.** Le protocole a été gelé **avant toute implémentation ou mesure A6-WL2** au SHA `14f8dd0830afe4fc5aaaa78cc819a65a454f7d09` : [préenregistrement](A6-WL2-UNANCHORED-ANF-PREREGISTRATION.md). Premier run natif complet : [GitHub Actions A6-WL2, run 38228697671 / job 114742507250](https://github.com/Memorithm/SML-GENIUS/actions/runs/38228697671/job/114742507250), code SHA `0fe8dd8097eb79aaa8d006e5458c9cfa361d9bdd`, **SUCCESS**, avec 42 fits, 42 TEST, 7 contrôles négatifs et une page SBG apprise.

## 1. La différence méthodologique essentielle avec la PR #500

Le prototype A6-WL1 exigeait quatre **positions de sélecteur connues** pour apprendre les autres littéraux d'un monôme. Cette information n'est **plus fournie** au nouvel apprenant `anf_wide_unanchored_learn::fit_unanchored_disjoint_sparse`. Il ne prend que `fan_in`, le nombre de monômes attendu (quatre), les limites de degré et de ressources, une graine de **requêtes** indépendante de la vérité, et une **callback d'étiquetage à la demande** utilisée sur TRAIN.

Le modèle générateur est :

`f(x) = (AND_{j∈S1} x_j) XOR (AND_{j∈S2} x_j) XOR (AND_{j∈S3} x_j) XOR (AND_{j∈S4} x_j)`

avec **supports inconnus deux à deux disjoints**, de degrés [2,2,2,2] pour `k=8`, et [2,3,4,5] pour `k≥16`. Les supports sont tirés sans remise après permutation Fisher–Yates seedée des `k` indices physiques, triés seulement pour permettre une comparaison canonique. Chaque monôme est positif (conjonction sans négation) et il y a **exactement quatre termes**. Ce sont des hypothèses structurelles **très fortes**, annoncées avant tout apprentissage.

Un monôme AND ne porte pas d'étiquette de « sélecteur » intrinsèque : pour un support `{i,j,k}`, choisir `i`, `j` ou `k` comme facteur particulier produit exactement la **même fonction**. A6-WL2 évite donc de mesurer une exactitude de « vrai nom de sélecteur » non identifiable, et vérifie plutôt le **support entier non ordonné**.

## 2. Apprentissage effectif sans divulgation des supports

L'apprenant interroge `FnMut(&[bool])->Result<bool,...>` sur des entrées choisies **pendant TRAIN**, sans jamais recevoir la liste des vrais monômes. Pour chaque support à découvrir :

1. générer des témoins `x` bit à bit indépendants Bernoulli(1/2) ; demander à un oracle **résiduel** s'ils donnent 1, après XOR des monômes déjà appris ;
2. sur un témoin positif, supprimer des groupes de bits ; conserver l'effacement si la sortie résiduelle reste 1 ; sinon restaurer et explorer les deux demi-groupes, jusqu'à obtenir une affectation positive minimale ;
3. accepter seulement un support nouveau **de degré 2 à 5**, disjoint des précédents ; rejeter les faux témoins et continuer sans ajuster les budgets ;
4. compiler les quatre supports appris en une vraie `WideAnfGate::from_monomials`.

Cette procédure est justifiée uniquement sous la grammaire connue : un résidu positif active un nombre impair de monômes (1 ou 3). Trois monômes disjoints de degré ≥2 réclament au moins six bits actifs, donc le plafond de degré 5 bloque cette ambiguïté après minimisation. Un autre support, des monômes chevauchants, des termes négatifs, des coefficients bruités temporellement ou une absence d'interventions peuvent invalider ce raisonnement. Les expériences **n'ont observé aucun témoin finalement rejeté** sur les graines sélectionnées ; ce n'est pas une garantie hors du corpus.

L'oracle TRAIN propre fournit un label par requête logique. Le panel bruité inverse indépendamment chaque réponse avec une probabilité Bernoulli(1/16), et utilise **21 répétitions à vote majoritaire** par requête logique. Les limites de construction sont `k≤2048` demandé au constructeur, quatre monômes, degré≤5, 25 000 observations physiques, 512 essais témoins par monôme et les budgets de `WideAnfLimits` natifs. Ces plafonds sont **des budgets expérimentaux**, non les limites algébriques de l'ANF.

## 3. Population, VALIDATION et TEST indépendants du fit

Sept largeurs `[8,16,32,64,128,256,1024]` × 3 graines × 2 panels TRAIN = **42 ajustements**, pour **21 structures cachées distinctes** (les deux panels de bruit partagent un générateur). Après chaque fit, une validation **non utilisée pour réajuster le modèle** utilise 512 requêtes aléatoires +32 requêtes structurées construites dans le banc, avec l'enseignant (dont tous les supports isolés et combinaisons). Toutes les **42 politiques et décisions de validation ont été figées avant toute génération TEST**. Les TEST sont des séquences indépendantes de 4096 entrées par fit, partagées entre panels pour les mêmes largeur/graine. Les six modèles k8 ont en plus subi le contrôle exhaustif des 256 affectations possibles.

**Résultats primaires** :

| Critère préenregistré | Mesure observée |
|---|---:|
| Ajustements gelés avant TEST | **42/42** |
| Récupération exacte des supports **non ordonnés** | **42/42** |
| Zéro erreur de validation indépendante (544/fit) | **42/42** |
| Zéro désaccord bit-à-bit sur TEST (4096/fit) | **0 désaccord sur 172 032 sorties** |
| Exhaustivité de la vérité k8 | **6/6**, zéro différence sur 1536 affectations |
| Contrôles hors grammaire rejetés | **7/7** |
| Porte k1024 réellement apprise, exécutée dans SBG | **16/16** sorties égales à l'enseignant |

La dernière mesure place **la porte apprise**, et non les paramètres du générateur, dans un `WideAnfPageAtlas<1024>` avec 1024 coordonnées `SbgCoordinate::Value`, sortie `Meaning[0]` et décision MOR explicite `PageId(19)`. Les 16 tests mélangent requêtes aléatoires, tous les bits à 1/0 et monômes isolés. Cela valide la compatibilité avec le chemin de pages SBG publié en PR #499, **pas** une sélection MOR apprise ni un entraînement end-to-end avec Delta-KV.

## 4. Budget d'apprentissage réel et coûts CPU

Moyennes sur les **trois graines** par ligne (panel propre et panel bruité voient la même vérité). Les temps incluent l'oracle synthétique **local en Rust**, non la collecte physique de 8 000 observations externes.

| Largeur | Requêtes logiques | Requêtes physiques TRAIN bruité (×21) | Tentatives de témoin | Temps propre | Temps bruité |
|---:|---:|---:|---:|---:|---:|
| 8 | 70,3 | 1477 | 10,3 | 0,0100 ms | 0,0321 ms |
| 16 | 162,7 | 3416 | 52,7 | 0,0188 ms | 0,0842 ms |
| 32 | 187,0 | 3927 | 35,3 | 0,0229 ms | 0,0585 ms |
| 64 | 216,7 | 4550 | 29,0 | 0,0287 ms | 0,0854 ms |
| 128 | 315,0 | 6615 | 77,7 | 0,0629 ms | 0,1189 ms |
| 256 | 335,7 | 7049 | 72,0 | 0,0790 ms | 0,1569 ms |
| **1024** | **391,3** | **8218** | **46,0** | **0,1825 ms** | **0,3237 ms** |

Pour `k1024`, le support sélectionné contient 4 monômes d'un degré maximum de 5, compilés en **8960 octets de payload logique de porte**, **15 mots u64 de scratch** (hors métadonnées d'allocateur et autres buffers de la boucle d'apprentissage). Les budgets réellement consommés restent sous 25 000 observations physiques par ajustement.

La comparaison indicative avec A6-WL1 (`k1024`: 240 requêtes logiques avec sélecteurs connus et 15 répétitions du bruit) montre ici **plus de requêtes logiques pour éliminer le prior sur les sélecteurs**, mais ne permet pas une simple comparaison de coûts physiques, car **21** votes ont été fixés pour A6-WL2. Le runner CPU, les callbacks et la grammaire sont différents ; les durées ne justifient aucun facteur de vitesse généralisé.

## 5. H3 : les fonctions hors grammaire doivent être refusées

Chaque contrôle construit une ANF des quatre monômes de base **plus une cinquième porte AND de degré `k`**, donc exclue à la fois par le nombre de termes et par la borne de degré 5. Un tel terme est extraordinairement difficile à observer par échantillonnage uniforme lorsque `k` est grand (`P(all-one)=2^{-k}`) : une validation purement aléatoire risquerait de le déclarer inexistant. Le protocole comprend donc un **témoin tout-un** dans le panneau de 32 configurations structurées.

Le fitter trouve un modèle représentable à quatre termes pour chacune des 7 largeurs, mais **la validation rejette tous les sept** : erreur VAL **3/544** à `k8`, et **1/544** à chacune des largeurs `k16,32,64,128,256,1024`. Tous les contrôles ont produit `accepted=false`. Il s'agit d'un contrôle de couverture *volontairement ciblé* par le protocole, pas d'un détecteur général d'ANF hors hypothèses. La preuve de ces rejets est visible en lignes `WIDE_UNANCHORED_NEGATIVE` du journal du binaire.

## 6. Exécution/CI et périmètre restant

Le premier code a nécessité uniquement les ajustements `rustfmt` et le retrait d'un import inutilisé du banc ; les populations, budgets, degrés, monômes, graines, structures de VAL/TEST et seuils restent ceux du document préenregistré. Premier run complet : [A6-WL2 GitHub Actions](https://github.com/Memorithm/SML-GENIUS/actions/runs/38228697671/job/114742507250), conclusion SUCCESS sur `0fe8dd8097eb79aaa8d006e5458c9cfa361d9bdd`. `cargo fmt`, `check`, `clippy -D warnings`, tests unitaires et le binaire RELEASE ont été validés. **Une dernière exécution de la CI générale et de la CI dédiée est exigée au SHA final documentaire avant fusion.**

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --lib anf_wide_unanchored_learn
cargo test --locked -p sml-core --bin sml-wide-unanchored-learn-bench
cargo run --locked --release -p sml-core --bin sml-wide-unanchored-learn-bench
```

**Conclusion bornée :** H1/H2 PASS sur **42 ajustements de 21 enseignants**, H3 PASS sur **7 contrôles négatifs spécifiques**. C'est une avancée de la découverte d'ANF sparse de faible degré sans annotation de sélecteur, mais sous un **prior de quatre monômes positifs et disjoints** et avec un **oracle actif**. Il reste notamment à résoudre les supports chevauchants, l'apprentissage uniquement passif, l'identification de la cardinalité des supports, la factorisation de réseaux récurrents à k≥4 portes cachées et la qualité du modèle SML-25B, avant tout projet de banque à 1 048 576 fonctions effectivement apprises.
