# A6-C12f — Résultats : matrice de bruit caché et égalité bit-à-bit des réseaux ANF

**Statut : protocole préenregistré exécuté intégralement ; les conclusions sont bornées à la topologie et aux distributions déclarées.**

Le [préenregistrement](A6-C12F-STATE-NOISE-OBSERVABILITY-PREREGISTRATION.md) a été commité en premier au SHA `87034ddfc2a5bd78dabb7f3badc850008135e7b4`, **avant tout code A6-C12f et avant tout résultat**. Premier jeu de mesures natives intégral : commit source `163d59de7ef66831c39e109ed7556df6b38a0ff6`, [CI dédiée, run 38212608809 / job 114689960323](https://github.com/Memorithm/SML-GENIUS/actions/runs/38212608809/job/114689960323), **SUCCESS** après format/check/Clippy strict, unit tests et campagne de 240 politiques. La CI générale Rust de cette première révision a également conclu **SUCCESS** ; vérifier encore les contrôles au SHA final après ajout du rapport.

## 1. Ce qui a été réellement exécuté

La classe d'apprenants **n'a pas été élargie** : anneau `LocalAnfNetwork` de **N128 bits d'état** avec racine et readout fixes et **trois** portes booléennes inconnues, toutes représentables. Il y a **508 sources candidates** (127 emplacements internes j1..127 × quatre canaux), non 508 états, et quatre comparateurs de sélection de sources/factorisation :
1. Walsh-2 sur toutes les **128 016 paires** admissibles, top8 et extension de troisième source ;
2. Sketch-MITM, **48 esquisses de 14 bits**, collision XOR, 8 mots pour présélection, top256 long puis top16 ;
3. HYBRID, union des sources proposées par W2 et MITM, avec le **même factoriseur ANF 520 fonctions composées et rescore natif** ;
4. Walsh-3 exhaustif en flux, **21 336 000 corrélations d'ordre 3** sur tout TRAIN et top16, uniquement sur seed0.

Les circuits générateurs sont XOR→XOR→XOR et AND→XOR→XOR, portes à j20/x2, j68/x3, j116/x4, avec tables 6 ou 8. Les paramètres cachés n'ont pas été fournis au criblage/factoriseur. Les TRAIN/VAL sont de 512 pas×64 voies, les TEST indépendants de la sélection 4096 pas×64 voies. Les labels sont propres ou ont un bruit Bernoulli(1/16) indépendant.

**Six régimes** : `slot∈{0,68,127}` × `p_flip∈{1/256,1/64}`, où le bit de l'état est réellement modifié par `state[slot] ^= mask` **après** chaque transition et **avant** la lecture native. Cet état corrompu est transmis à la prochaine transition. Le PRNG de perturbation est distinct du PRNG des inputs et du bruit d'étiquettes.

Effectifs gelés : **72 flux TRAIN/VAL**, W2+Sketch+Hybrid sur chacun = 216 modèles ; W3 sur seed0 des 24 strates tâche×panneau×slot×taux = 24 modèles ; soit **240 modèles gelés avant toute génération de TEST**. Ensuite **72 flux TEST** et **240 lignes TEST**. Les 72 flux sont appariés selon leur graines et les configurations de slots ; ils ne constituent pas 72 répétitions indépendantes d'un même problème.

## 2. Point méthodologique corrigé : identité de prédiction plutôt qu'identité de taux d'erreur

Les études A6-C12b–e comparaient notamment `native_errors(model,TEST)` à `native_errors(teacher,TEST)`. Cette égalité de **nombres d'erreurs** n'est pas une preuve que les deux fonctions prédisent les **mêmes bits**, car différents réseaux peuvent se tromper le même nombre de fois à des endroits distincts.

A6-C12f ajoute le **comparateur natif indépendant des labels** `native_prediction_disagreements(model, teacher, TEST.inputs)`. Chaque réseau est exécuté **séparément**, depuis un état zéro, sur tous les **4096 pas×64 voies**, puis leurs **sorties binaires correspondantes sont XORées et comptées**. Les mesures du code publient **les deux** : score contre les cibles bruitées/stochastiques et désaccord exact de prédictions. Un test unitaire expose explicitement deux vecteurs de prédictions qui obtiennent le même nombre d'erreurs mais n'ont aucun bit égal.

Cette nouvelle comparaison confirme la récupération **sur ces TEST** et non l'identité mathématique globale de toutes les fonctions représentables. Le témoin temporel ci-dessous possède, lui, une justification algébrique spécifique à son domaine d'entrées.

## 3. H1 et H2 — Résultats exacts de récupération

Critère préfixé : à chaque taux et dans **chaque** cellule des 2 tâches×2 panels×3 slots, **au moins 2 des 3 graines** doivent retrouver les positions, canaux et tables ANF d'origine **ET** obtenir **0 désaccord bit-à-bit** de sortie HYBRID contre le modèle déterministe d'origine sur TEST.

| Taux de transition | Strates passant le seuil 2/3 | HYBRID : paramètres exacts | HYBRID : 0 bit de désaccord sur TEST | W2 : paramètres exacts | Sketch seul : paramètres exacts | W3 exhaustif : paramètres exacts / sorties |
|---|---:|---:|---:|---:|---:|---:|
| **1/256** | **12/12** | **36/36** | **36/36** | 18/36 | 18/36 | **12/12** |
| **1/64** | **12/12** | **36/36** | **36/36** | 18/36 | 18/36 | **12/12** |
| **Total** | **24/24** | **72/72** | **72/72** | 36/72 | 36/72 | **24/24** |

Chaque succès « 0 bit de désaccord » couvre 262 144 comparaisons binaires natales sur son propre TEST, indépendamment des labels. Sur **72/72** TEST HYBRID, la proposition comme le réseau sélectionné ont zéro désaccord avec le déterministe de référence. Les modèles natifs sont réellement autonomes (pas de sorties enseignant injectées pendant TEST).

**Important** : ce succès est la récupération d'un **modèle déterministe sous-jacent**, pas la reconstruction des tirages aléatoires qui ont perturbé les trajectoires internes de l'enseignant. Il ne faut pas annoncer une erreur nulle contre les cibles stochastiques. Pour ces cibles, l'apprenant retrouve l'erreur exacte de la référence déterministe, qui peut être élevée.

## 4. Propagation de bruit : résultats directs plutôt qu'une valeur unique

Le nombre moyen de flips d'état réellement appliqués par TEST de 262 144 bit-voies est **1 019,33** au taux 1/256 et **4 088,25** au taux 1/64 (moyenne sur 12 flux par slot et taux), pour des valeurs théoriques 1 024 et 4 096. Les comptes sont identiques entre les slots sur une même entrée/graine parce que le flux des masques aléatoires est fixé et réutilisé : la comparaison **isole le slot**.

**Erreur TEST du modèle déterministe canonique contre l'enseignant avec état perturbé mais sans corruption supplémentaire des labels**, moyenne sur 3 graines par tâche/slot :

| Slot perturbé | XOR³, 1/256 | AND→XOR→XOR, 1/256 | XOR³, 1/64 | AND→XOR→XOR, 1/64 |
|---:|---:|---:|---:|---:|
| 0 | 6,1708 % | 0,7572 % | 19,0080 % | 2,9762 % |
| 68 | 6,0107 % | 0,7692 % | 18,5418 % | 2,9605 % |
| 127 | 6,1684 % | 0,7656 % | 19,0015 % | 2,9828 % |

Sur le panel ayant **en plus** le bruit Bernoulli(1/16) des labels, l'erreur de la même référence déterministe est comprise parmi les instances XOR³ entre **11,10–11,26 %** au faible bruit d'état et **22,50–22,88 %** au fort bruit d'état ; pour AND→XOR→XOR entre **6,91–6,93 %** et **8,75–8,84 %**. Les valeurs sont reportées par graine/slot dans les logs `A6C12F_TEST`.

La porte AND amortit les perturbations par rapport à la parité dans **ce banc construit**, sans démontrer une supériorité de robustesse générale d'AND pour d'autres architectures, états, distributions ou fréquences de bruit. De même, les différences slot68/slot0/slot127 sont des observations, pas une loi de sensibilité dérivée du modèle.

## 5. H3 — Certificat causal d'équivalence de sources

Contrôle indépendant généré **après** le gel des 240 politiques ; aucun calcul de cette expérience ne modifie l'apprenant ou la sélection. Nous comparons deux réseaux natifs distincts :

- **Générateur original** : porte interne XOR à j68 lisant x3 ;
- **Générateur alternatif** : aucune mutation en j68, porte XOR en j67 lisant x4, autres mutations identiques.

Dans la géométrie fixée de l'anneau N128, la contribution retardée de la première porte est `x3(t-(128-68))=x3(t-60)` et celle de la seconde `x4(t-(128-67))=x4(t-61)`. Sous la contrainte **`x3(u)=x4(u-1)` et `x3(0)=0` avec histoire négative nulle**, les deux contributions sont égales à chaque pas ; les autres composantes natives étant inchangées, le système a les mêmes sorties après transport et induction sur les transitions initialisées à zéro. Cette équivalence repose sur la porte **XOR** déplacée et l'**identité de la position j67** ; elle ne vaut pas pour une mutation AND arbitraire ni sans contrainte d'entrées.

Les mêmes réseaux ont été exécutés indépendamment sur **6 nouveaux flux propres** de 4096 pas×64 voies (2 tâches ×3 graines) :

| Contrôle | Désaccord bit à bit |
|---|---:|
| EXACT_LAG `x3(t)=x4(t-1)` | **0 bit sur 6/6 flux** |
| NEAR_LAG, erreur Bernoulli(1/32) indépendante | **6/6 flux avec désaccord positif** |

Les nombres de bits différents sur les contrôles NEAR_LAG sont **76 174, 76 216, 75 811** pour XOR³, puis **14 616, 14 637, 15 158** pour AND→XOR→XOR, chacun sur 262 144 bit-voies.

Le certificat valide donc **une relation spécifique entre deux réalisations** sur un support d'entrées déclaré, et non l'énumération exhaustive d'une classe d'équivalence de tous les circuits ANF.

## 6. Coût mesuré : entier du pipeline, et comparaison appariée

Moyennes du temps **criblage TRAIN + factorisation/validation native** sur runner GitHub-hosted Linux x86_64, Rust 1.89 RELEASE. Pour W2, Sketch, HYBRID : 36 flux par taux ; pour W3 : 12 flux par taux, seed0 de chaque tâche/panneau/slot. La comparabilité temporelle est fondée exclusivement sur des flux appariés, pas sur une projection théorique.

| Méthode | 1/256 (moy. totale) | 1/64 (moy. totale) | Succès paramétriques |
|---|---:|---:|---:|
| W2 paires et factorisation | 126,260 ms | 125,999 ms | 36/72 |
| Sketch-MITM seul | 33,087 ms | 33,093 ms | 36/72 |
| **HYBRID W2+MITM** | **158,488 ms** | **158,181 ms** | **72/72** |
| Walsh-3 exhaustif (n=12/taux) | **5 578,185 ms** | **5 568,201 ms** | 24/24 |

Pour les **24 seuls flux où W3 et HYBRID sont directement appariés**, W3 = **5 573,193 ms** en moyenne, HYBRID = **158,610 ms**, soit **35,138×** d'écart observé en temps total. Les ratios par slot et taux vont d'environ **34,97× à 35,25×**. Tous les chiffres proviennent du même profil d'exécution, mais ne sont ni une preuve de O(n²), ni des mesures de GPU, Tensor Cores ou d'inférence de modèle linguistique.

Au-delà du nombre de corrélations complètes de Sketch (**256 par flux**), la méthode réalise en moyenne **~190 300 hits de buckets** et **~62 360 structures uniques de présélection** : ne pas confondre nombre de corrélations longues et nombre de toutes opérations, et ne pas annoncer un gain CPU de 83 000× en divisant simplement 21 336 000 par 256.

## 7. Reproductibilité, limites et décision

Premier run complet : [GitHub Actions A6-C12f](https://github.com/Memorithm/SML-GENIUS/actions/runs/38212608809/job/114689960323) au SHA `163d59de7ef66831c39e109ed7556df6b38a0ff6`. `cargo fmt --check`, `cargo check --locked -p sml-core --all-targets`, `cargo clippy ... -D warnings`, tests unitaires et 240 politiques : **SUCCESS**. Aucun résultat n'a été utilisé pour ajuster les budgets ou les masques ; le premier échec CI sur cette branche était un écart de formatage `rustfmt`, corrigé sans modification de protocole, d'algorithme ou de graines.

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --bin sml-state-observability-bench
cargo run --locked --release -p sml-core --bin sml-state-observability-bench -- 3
```

**H1 PASS sur 12/12 strates à 1/256, H2 PASS sur 12/12 strates à 1/64, H3 PASS sur les 6+6 témoins.** Les tests apportent une preuve opérationnelle de sélection native bit-à-bit sur ce domaine borné, mais **pas** une démonstration statistique de taux de généralisation hors distribution, une borne théorique de coût, une récupération de bruit latent, un apprentissage d'architecture libre ou une banque à un million de fonctions. La source du bruit est ponctuelle dans l'état, indépendante et de faible probabilité ; les six régimes partagent des PRNGs de base. Les circuits ont seulement trois portes cachées dans un anneau fixe de 128 bits d'état.

**Prochains critères de recherche** : perturber plusieurs états simultanément ou des entrées/endpoints masqués, tester bruits temporellement corrélés et populations biaisées, définir les classes d'équivalence conditionnées au support de distribution sans sur-interpréter les témoins finis, puis augmenter la grammaire d'apprentissage à quatre/cinq portes et comparer aux nouveaux coûts combinatoires. Des banques ANF massives, SML-25B, mémoire SBG/MOR/Delta-KV et accélération GPU ne sont pas qualifiés par A6-C12f.
