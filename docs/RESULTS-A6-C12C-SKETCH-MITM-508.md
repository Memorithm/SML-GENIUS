# A6-C12c — Rapport mesuré : esquisses, hybridation et comparaison W3 à 508 sources

**Statut : banc préenregistré exécuté, réussite bornée de l'hybride, limites/échecs conservés.** 
Protocole gelé **avant le premier code** au commit `cbbb8b3fd8caec69ebba3cede8e3df3763001395` : [préenregistrement](A6-C12C-SKETCH-MITM-508-PREREGISTRATION.md). SHA des premières mesures intégrales : `9c7a118f12e85aa9bcbe5f2f58d56020dde4dda8`. Job Rust 1.89 x86_64 GitHub-hosted : [A6-C12c / 272 politiques](https://github.com/Memorithm/SML-GENIUS/actions/runs/38185793347/job/114607704753). Workflow dédié **SUCCESS**, format/check/Clippy et tests internes **SUCCESS** sur ce SHA.

## 1. Question testée

Le banc précédent A6-C12b trouvait les vrais triplets via une recherche Walsh-3 exhaustive mais devait évaluer jusqu'à **21 336 000** structures admissibles à 508 sources (127 positions ×4 canaux). Cette expérience compare la force brute à un **criblage par esquisses 14 bits, 48 tours, rencontre au milieu**, plus la recherche W2 par paires de A6-C12b et leur union.

Le modèle est **toujours un anneau récurrent à 128 états, racine/output fixes, trois portes internes**, pas un SML-25B. Les positions j20/j68/j116, canaux x2/x3/x4 et tables booléennes sont accessibles au générateur et aux contrôles **ex post**, jamais à l'algorithme qui reçoit seulement inputs/labels TRAIN/VAL. Les grilles n32/n128 préservent la géométrie C12b ; la grille n508 contient toutes les positions internes j1..127 et les quatre canaux bruts. Les bruits de labels sont indépendants Bernoulli(1/16). Les états et sorties TEST sont exécutés nativement sans teacher forcing.

Les 24 jeux TRAIN/VAL distincts (4 tâches ×2 panneaux ×3 graines) sont appariés d'une méthode/largeur à l'autre. Les 24 TEST indépendants de 4096 pas ×64 voies ont été créés **uniquement après gel des 272 sélections**. Ce sont **24 flux**, pas 272 expériences indépendantes. La référence W3 n508 est réservée à **8 flux** (seed 0 de chaque tâche/panneau) : l'écart de taille est déclaré, et les ratios temporels utilisent seulement les 8 flux appariés.

## 2. Paramètres exacts et métriques

- **W2-PAIR** : score Walsh de toutes les paires arbitraires de positions distinctes ; top8 paires, extension par tous les troisièmes candidats valides, factorisation commune de 520 masques composés et top16 native.
- **SKETCH-MITM** : 48 hachages de 14 bits aléatoires déterministes fixés avant exécution ; jointure sur `hash(p) XOR hash(x_i) XOR hash(x_j) == hash(x_k)` ; trois portes distinctes et déduplication. Mesure des collisions, score d'abord sur 8 mots u64, conservation top256, score ensuite sur les 384 mots TRAIN, conservation top16 puis même factoriseur ANF.
- **HYBRID** : union des candidats W2 et MITM, factorisation et validation identiques. Pour la parité, MITM restaure le triplet. Pour les circuits non linéaires, W2 fournit les sources manquantes.
- **W3-STREAM** : toutes les corrélations d'ordre 3 complètes, top16 retenu en mémoire avec un maintien incrémental ; **pas de matérialisation/tri de 21M candidats**. C'est encore une exploration exhaustive en calcul mais moins coûteuse en RAM que le tri intégral du banc C12b.

Les coûts en temps **incluent filtre et factorisation**, sans être artificiellement égalisés entre méthodes ; toutes les méthodes reçoivent le même jeu de données et le même code de validation native.

## 3. Résultats n508 : paramètres retrouvés exactement

Succès = les trois positions, trois canaux et trois tables ANF identiques au générateur **et** sortie du modèle choisi identique à l'erreur TEST de l'oracle natif, évalué sur la même séquence.

| Famille réelle | W2-PAIR | SKETCH seul | **HYBRID** | W3-STREAM |
|---|---:|---:|---:|---:|
| XOR→XOR→XOR (parité) | 0/6 | **6/6** | **6/6** | 2/2 |
| AND→XOR→XOR (mixte) | **6/6** | 0/6 | **6/6** | 2/2 |
| XOR→AND→XOR (contrôle) | **6/6** | 0/6 | **6/6** | 2/2 |
| AND→AND→AND (contrôle) | **6/6** | 0/6 | **6/6** | 2/2 |
| **Total** | **18/24** | **6/24** | **24/24** | **8/8** |

Le résultat principal préenregistré « ≥2 graines sur 3 par panneau » est atteint **3/3 pour chacune des quatre tâches et pour les deux panneaux**, y compris parité bruitée et mélange AND+XOR. Le véritable triplet est présent dans l'ensemble des candidats MITM **sur les six cas de parité**, sans utilisation des paramètres du professeur par le générateur de candidats. Les témoins non linéaires ne sont **pas résolus par le sketch seul** : cet échec est attendu et conservé.

Sur n32 et n128, l'hybride récupère également 24/24 configurations dans chaque grille. Au total, l'hybride donne **72/72 réussites**, contre 54/72 pour W2 seul et 18/72 pour SKETCH seul. W3 retrouve 56/56 cas effectivement évalués (n32/n128 : 24+24 ; n508 : 8).

## 4. Coût total mesuré et bascule

Moyennes sur les **24 flux** pour W2, SKETCH, HYBRID ; W3 sur **24 flux à n32/n128** et **8 flux à n508**. Millisecondes en RELEASE sur le même type de runner x86_64 GitHub-hosted Rust 1.89. Colonnes « coût total » = criblage TRAIN + factorisation ANF/validation. Ne pas interpréter les chiffres comme des performances matérielles Dell/Thor/GPU.

| Sources n | W2 total | MITM seul total | **HYBRID total** | W3 exhaustif total |
|---:|---:|---:|---:|---:|
| **32** | 4,95 ms | 1,46 ms | **5,29 ms** | **2,39 ms** |
| **128** | 22,81 ms | 2,88 ms | **24,50 ms** | 83,80 ms |
| **508** | 125,93 ms | 33,46 ms | **158,26 ms** | 5 527,43 ms |

**Comparaison rigoureusement appariée à n508** : sur les **huit mêmes flux** (seed 0 pour chacune des quatre tâches ×2 panneaux), HYBRID = **157,662 ms** en moyenne contre **5 527,426 ms** pour W3 streaming, soit **35,06×** de différence de temps total observé. Le ratio n'est pas extrapolé à d'autres k, distributions, ni au pire cas.

À n32, **W3 exhaustif est plus rapide que HYBRID** : le coût fixe des paires+factorisation dépasse le coût des 3 584 triplets. À n128, HYBRID devient plus rapide : 24,50 vs 83,80 ms. Ce croisement rend nécessaire un choix adaptatif par taille, pas un remplacement systématique de W3.

### Dénombrement réel n508, par flux

- Triplets admissibles au total : **21 336 000** (W3 évalue chacune de ces corrélations sur 384 mots, 8 flux).
- Paires admissibles W2 : **128 016**.
- Probes (paires×48 tours) de MITM : **6 144 768** ; ce ne sont que des recherches de seaux, pas des corrélations longues.
- Collisions de seaux moyennes : **190 289** (répétitions incluses, certaines invalides).
- Triplets distincts récupérés via hash, moyenne : **62 334**.
- Scores Walsh sur **8 mots** de ces candidats : **62 334** en moyenne.
- Scores complets sur **384 mots** : **256** par flux ; top16 transmis au factoriseur.
- Coût MITM seul : **32,152 ms filtrage +1,308 ms factorisation** ; HYBRID : **78,216 ms criblage W2+MITM +80,042 ms factorisation**. La factorisation W2 (plusieurs milliers de candidats) demeure un coût dominant.

La méthode compte donc **256 corrélations complètes d'ordre 3**, mais traite aussi toutes les paires W2, 6,14M probes de hash et des dizaines de milliers de scores courts. **Ne jamais présenter 256/21,336,000 comme un gain 83 000× de coût CPU** : la comparaison pertinente est la durée globale appariée **35,06×** et la précision conservée.

## 5. Portée de la preuve et limites

1. **Trois portes seulement**, anneau d'identités connue, racine et readout fixés. L'état natif est N128, le nombre 508 désigne les **caractéristiques source candidates**, pas la largeur du réseau.
2. **Quatre fonctions synthétiques connues**, trois graines par tâche et panneau, entrées indépendantes. Cette campagne n'établit pas de taux de succès général. Dans le bruit récurrent teacher-forcé, la corrélation pure attendue est `49/64` et non `7/8`.
3. **Complexité probabiliste non certifiée** : en distribution défavorable tous les candidats peuvent collisionner dans chaque seau, et MITM redevient cubique en visites. Pas de preuve de détection pour sources corrélées, parités à 4/5 variables, état partiellement observable ou bruit non i.i.d.
4. Les huit références W3 n508 ne couvrent qu'une graine par famille/panneau ; les autres 16 flux ne possèdent pas de comparaison W3 n508. Les différences de timing et de matériel doivent être requalifiées indépendamment.
5. Le **moteur existant n'est pas remplacé** dans l'inférence de SML-GENIUS. Il s'agit d'un banc de sélection et de factorisation ; aucune mesure token/s, perplexité, GPU, VRAM ou de banque à un million de fonctions n'a été faite.

## 6. Reproductibilité / CI

Révision de première mesure sur Actions : `9c7a118f12e85aa9bcbe5f2f58d56020dde4dda8`. Le job spécialisé a conclu **SUCCESS** : format, compilation, Clippy strict, tests analytiques et **272 FROZEN/272 TEST/72 grilles**. La CI générale `rust` demeure soumise à une vérification séparée sur la révision finale du rapport (ne pas la supposer verte par transitivité).

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core anf_three_gate_diagnostic
cargo test --locked -p sml-core --bin sml-sketch-mitm-bench
cargo run --locked --release -p sml-core --bin sml-sketch-mitm-bench -- 3
```

## 7. Orientation suivante, strictement conditionnelle

**PASS A6-C12c pour ces bancs bornés** : le mélange W2+MITM permet le recouvrement des deux familles de parité et de non-linéarité testées, avec un avantage mesuré face au W3 exhaustif à n508.

Avant toute promotion du moteur de recherche : explorer les **distracteurs fortement corrélés**, les distributions asymétriques, l'identifiabilité théorique lorsque des variables sont masquées, la sensibilité à des bruits temporels non i.i.d. et des **grammaires à k=4/5** si l'apprenant a lui-même cette capacité. Tester la sélection et le coût en environnement GPU/SIMD séparément, avec les briques natives Memorithm et aucune technologie logicielle NVIDIA hors drivers. Ne pas confondre un prototype de screening avec le modèle SML-25B.
