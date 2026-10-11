# A6-C12a — Résultats : échec spectral sur parité représentable

**Statut : résultats mesurés, contrôle de cardinalité CI corrigé sans modifier le banc.**
**Protocole préenregistré avant les mesures :** `docs/A6-C12A-SPECTRAL-ADVERSARIAL-PREREGISTRATION.md`, commit initial `0ba625defc74a13b31cdd9e4df1b9a9c80ff3e8f`.

## Résultat principal : échec du filtrage, pas de la capacité

Les deux nouvelles fonctions sont **exactement représentables dans la grammaire à trois portes** : `R⊕a⊕b⊕c` avec les tables XOR/XOR/XOR, et `(R∧a)⊕b⊕c` avec AND/XOR/XOR. Le générateur natif à trois portes fournit zéro erreur sur TEST propre et environ 1/16 sur TEST bruité. En revanche, **aucun des trois classements préenregistrés ne récupère les trois sources dans le top24 sur les cas adversariaux**, ni avec 512 ni avec 2048 pas.

Chaque ligne du tableau représente les 5 graines × 2 panneaux × 2 tailles TRAIN = **20 classements corrélés** par famille et méthode. Critère préfixé : **les trois vraies sources parmi les 24 meilleures** (sur 508 candidats). Les taux sont des comptes, pas des intervalles statistiques.

| Famille native | W1 marginal | W1+R passé | W1+R+peeling XOR |
|---|---:|---:|---:|
| **XOR→XOR→XOR (parité)** | 0/20 | 0/20 | **0/20** |
| **AND→XOR→XOR (mixte)** | 0/20 | 0/20 | **0/20** |
| XOR→AND→XOR (contrôle C11D) | 0/20 | 0/20 | **20/20** |
| AND→AND→AND (contrôle C11D) | 20/20 | 20/20 | **20/20** |

À l'échelle des 508 sources, **les vraies structures des deux cas adversariaux ne sont jamais présentes dans les triplets du top24**, y compris en élargissant les évaluations complètes de 64 à 640. Les **rangs individuels** sont tous conservés dans les lignes `A6C12A_RANK` du journal ; les distributions (médiane et pire rang de chacune des trois sources) sont conservées dans ses 48 lignes `A6C12A_RANK_SUMMARY`.

La comparaison des deux contrôles révèle une information nouvelle : **le peeling XOR unique porte effectivement la récupération de XOR→AND→XOR**, tandis que le motif AND→AND→AND est déjà détectable par Walsh marginal. Le peeling n'est pas pour autant un solveur GF(2) de parités générales.

## Prédiction indépendante : représentation correcte, apprentissage faux

TEST autonome natif de 4096 pas ×64 voies (262 144 bits/flux). Moyennes sur **cinq graines**, pour le protocole de référence TRAIN=512 / budget=64 (tous les autres budgets et longueurs restent disponibles dans les logs).

| Famille | TEST propre sélectionné | TEST propre oracle | TEST bruité sélectionné | TEST bruité oracle | Structures exactes dans les 40 politiques de la famille |
|---|---:|---:|---:|---:|---:|
| XOR→XOR→XOR | **49,701 %** | **0 %** | **49,777 %** | **6,257 %** | **0/40** |
| AND→XOR→XOR | **49,776 %** | **0 %** | **49,759 %** | **6,230 %** | **0/40** |
| XOR→AND→XOR | **0 %** | **0 %** | **6,246 %** | **6,246 %** | **40/40** |
| AND→AND→AND | **0 %** | **0 %** | **6,245 %** | **6,245 %** | **40/40** |

Sur les quatre configurations TRAIN 512/2048 × budget 64/640 :
- Parité XOR : les moyennes TEST des réseaux retenus varient **49,701–49,867 %** selon panneau et configuration.
- Cas mixte : **49,702–49,786 %**.
- Deux contrôles positifs : récupérations paramétriques et erreurs oracle strictement conservées sur **toutes** les variantes.

Le **nombre de politiques apprises est 160** (4×2×5×2×2), et non 80 : voir erratum explicite dans le préenregistrement. Les **240 observations de classement** proviennent des **40 flux tâche/panneau/graine** avec préfixes communs et scores couplés ; elles ne constituent pas 240 essais indépendants. Les 160 politiques ont été gelées avant la création des 40 TEST indépendants. L'unique premier échec du job résultait d'une assertion de cardinalité `80` alors que le banc a terminé les **160** politiques et 48 synthèses ; aucune sélection n'a été modifiée lors de la correction.

## Contrôle analytique de falsifiabilité

Le test Rust `parity_and_masked_and_are_adversarial_to_low_order_screening` vérifie exactement les 16 affectations possibles des quatre variables :
- `R⊕a⊕b⊕c` ne possède de coefficient de Walsh non nul que pour le sous-ensemble complet `{R,a,b,c}`.
- `(R∧a)⊕b⊕c` annule les corrélations que le score de C11D sonde avec `x` ou `x⊕R`, mais possède un coefficient **non nul** sur `b⊕c` (degré 2 arbitraire).

Cette distinction est essentielle : **le cas mixte n'est pas invisible à tout Walsh de degré 2**, seulement à la famille particulière de tests d'ordre faible implémentée par notre classement actuel. Et l'unique « peeling » soustrait une caractéristique marginale dominante, sans élimination de Gauss.

## Attribution causale et limites

- **Capacité** : contrôlée par oracle natif appartenant à la même classe de trois portes ; donc **pas** l'explication.
- **Classement/filtrage** : échec attesté par 0/20 top24 pour les trois méthodes et chacun des deux cas difficiles.
- **Budget** : élargir 64 → 640 *après* top24 ne peut pas récupérer une source absente du top24 ; cette campagne n'évalue pas une méthode de classement alternative.
- **Données** : 512 → 2048 ne restaure pas les corrélations populationnelles nulles ; insuffisance statistique ponctuelle ne doit pas être confondue avec la limite algébrique.
- **Bruitage** : sous bruit de labels, résoudre directement des équations par Gauss n'est plus un solveur exact de parité ; il faut distinguer le problème des parités bruitées (*Learning Parity with Noise*).
- **Generalisation** : topologie annulaire et root/output fixés, 64 voies bit-slicées, entrées synthétiques indépendantes, uniquement N128. Aucune preuve d'avantage sur du langage, sur N512/1024, sur GPU ou sur banques ANF massives.

## Reproductibilité et provenance

Première exécution complète du programme : révision `350005a2c41a693a8fdec86dba1e1204bc22fbd6`, [job GitHub-hosted A6-C12a](https://github.com/Memorithm/SML-GENIUS/actions/runs/38182344480/job/114597361506). Le programme et tous les contrôles de format/compilation/Clippy/tests unitaires ont terminé correctement. L'échec à la dernière vérification CI était **strictement dû au contrôle erroné de 80 au lieu de 160 politiques**, corrigé dans la branche ; contrôler la nouvelle CI finale séparément.

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core anf_three_gate_diagnostic
cargo test --locked -p sml-core --bin sml-spectral-adversarial-bench
cargo run --locked --release -p sml-core --bin sml-spectral-adversarial-bench -- 5
```

## Décision scientifique

**FAIL utile du mécanisme spectral existant sur deux tâches représentables**, et **PASS de sa falsification expérimentale**. Ne pas agrandir maintenant aveuglément le faisceau ni la largeur de la banque. Préenregistrer séparément :

1. **C12b-affine propre** : référence GF(2) par élimination de Gauss, avec observabilité et rang de matrice vérifiés, sur parité sans bruit uniquement ;
2. **C12b-nonlinear** : scores Walsh de paires **arbitraires** / interactions conditionnelles, sans fuite des vraies sources, sur AND→XOR→XOR et les contrôles ;
3. **C12c-bruité** : séparer le régime LPN de la résolution linéaire exacte, avec budget mémoire/temps déclaré ;
4. **C12d-k variable** : extension de grammaire avec vérification *a priori* de représentabilité, puis recherche adaptative de k.

Chaque intervention devra battre **le baseline mesuré ici** sur **les mêmes TEST indépendants**, sans utiliser les labels TEST pour sélectionner la méthode. La priorité est la **récupération de sources masquées par des interactions de parité**, avant toute extrapolation à des banques ANF de millions de fonctions.
