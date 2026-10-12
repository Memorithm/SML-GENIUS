# A6-C12e — Résultats : alias temporels identifiables, équivalence native et bruit de transition caché

**Statut : expérience préenregistrée, exécutée, contrôles scientifiques du banc verts.**
Préenregistrement gelé **avant toute implémentation ou mesure A6-C12e** au commit `160197c23c543e5dbf88594c6fc800ef23821d50` : [protocole](A6-C12E-TEMPORAL-STATE-PREREGISTRATION.md).
Première campagne complète **120 politiques gelées, 120 évaluations TEST, 36 témoins natifs**, commit `6a62987b2c1db104aa58926a48ed617208c686d7` : [workflow dédié A6-C12e](https://github.com/Memorithm/SML-GENIUS/actions/runs/38210398435/job/114683041074), conclusion **SUCCESS**. Rust 1.89 `cargo fmt`, `check`, `clippy -D warnings` et tests unitaires validés. La CI générale `rust` doit être requalifiée au SHA **final** après publication documentaire.

## 1. Périmètre réellement étudié

Le générateur et l'apprenant sont des réseaux ANF **N=128 bits d'état, trois portes internes**, racine et lecture fixes. L'espace des sources contient **508 caractéristiques** (127 positions de porte ×4 canaux bruts) ; « 508 » ne désigne **pas** 508 états ni 508 portes. Les deux familles natives sont XOR→XOR→XOR et AND→XOR→XOR, mutations d'origine `j20/x2`, `j68/x3`, `j116/x4`.

36 splits TRAIN/VAL distincts par cellule tâche×panneau×mode×graine, soit 2 tâches ×2 panels (étiquettes propres ou bruit Bernoulli 1/16) ×3 modes ×3 graines. TRAIN=512, VAL=512, TEST=4096 pas ×64 voies. La stochasticité d'entrée et d'état provient de PRNGs distincts des entrées de base et du bruit d'étiquette. La génération TEST n'a lieu qu'après avoir gelé **les 120 ajustements**. Les méthodes et les modes réutilisent des graines de base appariées : il ne s'agit donc pas de 120 observations statistiques indépendantes.

Méthodes inchangées depuis A6-C12c/d :
- W2 paires arbitraires top8, extension avec toutes les sources de troisième porte, factorisation des 520 fonctions ANF composées, top16 native VALIDATION ;
- Sketch-MITM par 48 tours de 14 bits, 8 mots de présélection, top256 de score complet puis top16 vers le même factoriseur ;
- HYBRID union W2+MITM, même factoriseur et validation ;
- W3 exhaustif avec **21 336 000** triplets admissibles entièrement scorés et top16 en flux, réservé à **une graine par tâche/panneau/mode = 12 références**.

Les règles du générateur (positions, canaux et tables) ne sont pas fournies aux classeurs/factoriseurs. Les critères primaires H1/H2/H3 sont fixés dans le protocole avant toute observation.

## 2. H1 : alias temporels proches, mais non identiques — succès 12/12

`NEAR_LAG` impose `x1(t)=x2(t-1) XOR e1(t)`, `x3(t)=x4(t-1) XOR e2(t)`, avec e1/e2 Bernoulli(1/32), valeurs antérieures virtuelles nulles à t0. Les écarts de retard observés sur les TRAIN atteignent **3,149 %** pour x1/x2 et **3,085 %** pour x3/x4. Les sources restent distinctes.

La substitution native indépendante de `j68/x3` par `j67/x4`, en gardant les autres portes, produit **une sortie différente sur chacun des 12 TEST propres**. Les désaccords natifs mesurés varient de **75 550 à 77 457 bits** (XOR³) et de **14 456 à 15 171 bits** (AND→XOR→XOR), sur 262 144 bits par flux. Il s'agit d'un témoin concret d'observabilité, pas d'une preuve exhaustive d'unicité parmi tous les circuits.

| Famille et panneau | W2 exact/oracle | Sketch seul exact/oracle | **HYBRID exact/oracle** | W3 exhaustif |
|---|---:|---:|---:|---:|
| XOR³, propre | 0/3 | 3/3 | **3/3** | 1/1 |
| XOR³, label bruité | 0/3 | 3/3 | **3/3** | 1/1 |
| AND→XOR→XOR, propre | 3/3 | 0/3 | **3/3** | 1/1 |
| AND→XOR→XOR, label bruité | 3/3 | 0/3 | **3/3** | 1/1 |
| **Total** | **6/12** | **6/12** | **12/12** | **4/4** |

Le critère préenregistré de HYBRID **≥2/3 par tâche/panneau** est atteint **3/3 dans les quatre cellules**. Les 12 erreurs TEST de HYBRID sont strictement égales à l'enseignant déterministe exécuté nativement sur les mêmes entrées (0 % propre, environ 6,19 % pour XOR³ et 6,27 % pour AND→XOR→XOR avec corruption de labels).

## 3. H2 : alias temporels exacts — structure non identifiable, fonction récupérée

`EXACT_LAG` impose `x1(t)=x2(t-1)`, `x3(t)=x4(t-1)` à tous les pas, avec valeurs initiales nulles. Alors le circuit natif qui mute `j68/x3` et un second circuit dont cette mutation est déplacée à **`j67/x4`** sont **différents par position et canal**, mais produisent les **mêmes sorties sur toute séquence respectant la contrainte temporelle**, avec états initiaux nuls et identité à j67. Les deux versions sont simulées séparément ; toutes les **12/12 comparaisons sur TEST propre produisent zéro désaccord**.

Ce témoin montre une **classe d'équivalence observationnelle contenant au moins deux réalisations différentes**. Il ne constitue pas une énumération complète de la classe, ni une preuve de représentation minimale.

| EXACT_LAG (toutes familles et panels) | Paramètres strictement identiques au générateur | Prédictions sélectionnées égales à l'enseignant natif |
|---|---:|---:|
| W2 | 0/12 | 6/12 |
| Sketch-MITM seul | 0/12 | 6/12 |
| **HYBRID** | **0/12** | **12/12** |
| W3-STREAM (1 graine/cellule) | 0/4 | **4/4** |

**Ne pas classer les 0/12 d'identité des positions/canaux comme échec d'apprentissage** : ces étiquettes d'origine ne sont pas identifiables sur le support exact temporel. La validation appropriée est **l'équivalence fonctionnelle**, obtenue 12/12 par HYBRID. Les réussites « oracle » restent définies comme **égalité des taux d'erreurs TEST sur les mêmes cibles** ; ce constat expérimental seul ne prouve pas une identité universelle entre toute proposition du learner et le générateur. L'identité universelle se rapporte seulement au **couple natif témoin explicitement construit**.

## 4. H3 : perturbation effective de l'état caché — succès structurel, sortie stochastique

Le régime `STATE_FLIP` utilise des entrées brutes indépendantes. **Après** `teacher.step_lanes`, il applique `state[127] ^= Bernoulli(1/256)` lane-à-lane, **avant** le readout, et transporte cet état corrompu à la transition suivante. L'expérience mesure en moyenne **1 019,3 inversions d'état par TEST de 262 144 bits**, pour une attente de 1 024. Ce ne sont **pas** des erreurs d'étiquettes ajoutées après la simulation : ce sont des modifications réellement persistantes des trajectoires récurrentes.

Le modèle déterministe d'origine appartient toujours à la grammaire à trois portes, mais le générateur à transitions stochastiques est **hors de cette grammaire déterministe**. Le modèle déterministe correct ne peut généralement pas prévoir la réalisation d'un tirage aléatoire caché, même en disposant des règles exactes.

| État caché perturbé, 3 graines/panneau | Récupération exacte HYBRID | Erreur TEST HYBRID | Erreur TEST même modèle déterministe d'origine |
|---|---:|---:|---:|
| XOR³, labels propres | **3/3** | **6,168 %** | **6,168 %** |
| XOR³, labels Bernoulli 1/16 | **3/3** | **11,238 %** | **11,238 %** |
| AND→XOR→XOR, labels propres | **3/3** | **0,766 %** | **0,766 %** |
| AND→XOR→XOR, labels Bernoulli 1/16 | **3/3** | **6,917 %** | **6,917 %** |
| **Total** | **12/12** | **12/12 valeurs identiques** | référence |

W2 seul et Sketch seul récupèrent chacun 6/12 structures exactes, sur leurs familles respectives ; W3 récupère 4/4 structures sur la graine de référence. La preuve H3 est donc **une résistance de l'identification des paramètres à ce bruit caché faible et localisé**, pas une prédiction de ses réalisations aléatoires. L'erreur TEST du modèle exact ne constitue pas un plancher d'erreur théorique démontré pour un autre modèle qui disposerait de signaux supplémentaires sur l'état.

Le circuit XOR conserve davantage de perturbation de phase que le circuit contenant AND : sur labels propres, **6,168 %** contre **0,766 %** de discordance avec le générateur stochastique. Cette différence est observée pour ce slot perturbé et ces paramètres ; elle n'établit pas une robustesse générale des portes AND.

## 5. Coûts et références exhaustives, sans extrapolation

Temps moyen de fit mesuré sur runner GitHub-hosted x86_64, Rust 1.89 RELEASE, **criblage + factorisation native + validation**. W2, Sketch et HYBRID représentent **12 flux par mode** ; W3 seulement **4 flux par mode**.

| Mode | W2 total | Sketch total | **HYBRID total** | W3 exhaustif total |
|---|---:|---:|---:|---:|
| NEAR_LAG | 89,92 ms | 19,58 ms | **109,49 ms** | 3 860,04 ms |
| EXACT_LAG | 90,38 ms | 18,56 ms | **107,69 ms** | 3 856,44 ms |
| STATE_FLIP | 89,52 ms | 20,42 ms | **109,38 ms** | 3 826,25 ms |

**Comparaisons appariées uniquement sur les quatre mêmes jeux avec W3 :**

- NEAR_LAG : HYBRID **110,28 ms** vs W3 **3 860,04 ms**, ratio observé **35,00×** ;
- EXACT_LAG : HYBRID **108,01 ms** vs W3 **3 856,44 ms**, ratio **35,70×** ;
- STATE_FLIP : HYBRID **108,63 ms** vs W3 **3 826,25 ms**, ratio **35,22×**.

Les méthodes n'effectuent **pas** des travaux identiques : W3 parcourt 21 336 000 corrélations longues par cas, MITM sonde 48×128 016 paires de seaux et réévalue entièrement au plus 256 candidats, tandis que W2 étend les meilleures paires et factorise des milliers d'hypothèses. Le meilleur ratio mesuré n'est ni une borne sous-cubique théorique, ni un multiplicateur applicable à de nouvelles géométries, de nouveaux types de bruit ou des millions de fonctions ANF. Tous les résultats sont spécifiques aux trois portes et à la topologie fixe.

## 6. Reproductibilité, limites et décision

Version native complète : [GitHub Actions A6-C12e](https://github.com/Memorithm/SML-GENIUS/actions/runs/38210398435/job/114683041074), SHA `6a62987b2c1db104aa58926a48ed617208c686d7`. Sorties gelées : **120 `A6C12E_FROZEN`, 120 `A6C12E_TEST`, 36 `A6C12E_INPUT`, 36 `A6C12E_WITNESS`**. Le workflow refuse une mauvaise cardinalité, un témoin exact temporel contradictoire, une perturbation absente du mode STATE_FLIP et toute influence du bruit d'étiquette sur les entrées/coupons de flip.

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --bin sml-temporal-state-bench
cargo run --locked --release -p sml-core --bin sml-temporal-state-bench -- 3
```

Premiers échecs CI : ajustement de formatage et suppression d'un ancien diagnostic de paires dupliqué/non utilisé. Le protocole, graines, sources, tables, perturbations et budgets n'ont subi **aucune modification à partir des résultats**.

**Bilan scientifique :** H1 **PASS borné 12/12 exact**, H2 **PASS du témoin de non-identifiabilité et récupération fonctionnelle 12/12**, H3 **PASS de l'injection d'état et de la récupération exacte du réseau déterministe 12/12**, sans revendiquer la prédiction du bruit latent. Aucun test n'a évalué des masques d'observabilité de sources irréversibles, un graphe appris, quatre/cinq portes apprises, des distributions naturelles, du langage ou un GPU.

**Étape suivante, sous protocole distinct :** expérience `A6-C12f` sur *débit du filtre, interaction des sources sur graphes moins contraints, et sélection de structures observablement équivalentes* ; tester plusieurs intensités et plusieurs slots de perturbation d'état, des transitions stochastiques conditionnelles, et des entrées masquées **après étude d'identifiabilité**. Si l'on passe à k=4/5 portes, l'apprenant doit être étendu en capacité **avant** tout test de recherche afin d'éviter la confusion observée dans A6-C10. L'intégration à de grandes banques ANF et le modèle SML-25B restent hors du périmètre.
