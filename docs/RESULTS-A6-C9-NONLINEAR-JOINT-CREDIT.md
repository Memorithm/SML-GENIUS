# A6-C9 — Résultats du crédit conjoint non linéaire ANF, états 128/256 bits

**Statut : expérience exécutée et contrôlée ; succès partiel, échecs conservés.**
La PR #489 reste en brouillon. Aucune campagne A6-C10 n'est ouverte.

Le protocole est versionné dans
docs/A6-C9-NONLINEAR-CREDIT-PREREGISTRATION.md.
Un amendement d'algorithme (deux amorces et les deux ordres de
raffinement), motivé par un échec **de test unitaire ciblé**,
a été enregistré **avant la première campagne expérimentale
de 100 cas**. Ni les dix tâches, ni les graines, ni les
horizons TEST n'ont été modifiés après observation.

## Hypothèse

A6-C8 réussissait à attribuer le crédit conjoint à deux
portes affines XOR/XNOR mais échouait à résoudre XOR+AND.
A6-C9 autorise désormais **les 16 fonctions booléennes**
sur chacune des **deux portes cachées**, en gardant le
même anneau récurrent et le même readout fixes. La recherche
doit n'utiliser **que la cible finale** ; aucune cible
intermédiaire, aucun état interne du générateur, aucune
donnée de TEST dans le moteur d'apprentissage.

## Architectures / limites strictes

- Mémoire récurrente native ANF : **128 ou 256 bits par
  séquence** ; 64 séquences bit-slicées en parallèle via u64.
- Racine fixe : s0(t+1)=s_(N−1)(t) XOR x1(t).
- Lecture fixe : y_hat(t)=x0(t) XOR s0(t+1).
- Registres intermédiaires : identité, sauf **au maximum deux**
  positions cachées distinctes, choisies sur TRAIN.
- Pour chacune : position, entrée parmi x1..x4 et table
  complète de vérité à 2 entrées parmi 16 fonctions.
- Une même instance du générateur natif crée les labels
  finaux à partir de portes connues **seulement du générateur**.

Le réseau n'apprend **ni une topologie libre**, ni 256 portes
simultanément. Le résultat concerne une ossature de délais
fixée et deux mutations au maximum.

### Nouvel algorithme

Le calcul d'intervention joint utilise **trois files circulaires**
partitionnant les N mots d'état : délai root→porte1,
porte1→porte2, porte2→root. Les sorties sont calculées
synchoniquement à partir des états précédents, en O(T)
par hypothèse complète. L'égalité exacte de score
TRAIN entre ce scorer et la véritable simulation
LocalAnfNetwork est obligatoire pour le gagnant.

Les amorces sont entraînées uniquement sur TRAIN :
A6-C7 (une porte parmi 16 fonctions) et A6-C8 (deux
portes affines XOR/XNOR). La meilleure porte de A6-C7
est tenue fixe pendant une recherche exhaustive de
la seconde porte parmi les positions restantes,
quatre canaux et 16 fonctions. Ensuite, **chaque amorce**
est raffinée indépendamment selon les deux ordres des
axes, une passe coordonnée complète par ordre,
en acceptant uniquement les améliorations strictes
de perte TRAIN.

Le choix final sur VALIDATION seule compare :
référence (zéro porte), A6-C7 (une porte),
A6-C8 (deux portes affines), A6-C9
(deux portes potentiellement non linéaires).
Une égalité de pertes préfère le plus petit modèle.

**Important :** méthode heuristique non exhaustive à deux
portes. Les budgets des trois familles ne sont pas égalisés.
Une supériorité universelle n'est donc pas établie.

## Protocole de données réellement respecté

- **10 tâches préenregistrées**, dont un témoin négatif
  à trois injections distinctes.
- Deux panneaux : propre, et corruption indépendante
  Bernoulli(1/16) du **label final uniquement**.
- Cinq graines par tâche et panneau :
  **100 entraînements/validations**.
- TRAIN : 512 pas ×64 lanes ; VAL :
  512 pas ×64 lanes, sur une graine distincte.
- **Les 100 décisions figées AVANT la génération
  d'un seul jeu TEST.**
- Jeux TEST indépendants sur 4 096 et 16 384
  pas ×64 lanes, soit **200 résultats et 40 synthèses**.

Le workflow Rust 1.89 a contrôlé les compteurs
(100/200/40), les équivalences oracle et
l'exactitude du gel pré-test.

## Résultats TEST à 16 384 pas

Taux d'erreur binaire (en %), moyenne
sur cinq graines. **Plus faible = meilleur.**
Les chiffres sont propres au banc booléen
synthétique, non à la prédiction du langage.

| Tâche | A6-C7 une porte propre | A6-C8 affine propre | A6-C9 choisi propre | A6-C9 choisi bruit 1/16 |
|---|---:|---:|---:|---:|
| Identité128 | 0,000 | 0,000 | **0,000** | 6,249 |
| AND unique128 | **0,000** | 49,200 | **0,000** | 6,230 |
| XOR+AND128 | 24,834 | 49,305 | **0,140** | **6,312** |
| AND+XOR128 | 24,788 | 49,291 | **0,000** | **6,253** |
| AND+AND128 | **12,370** | 49,494 | **12,370** | **17,082** |
| XOR+OR128 | 24,863 | 49,301 | **0,000** | **6,236** |
| XOR+AND256 (position profonde j=1) | 29,444 | 48,407 | **0,000** | **10,565** |
| AND+AND256 | 15,517 | 48,915 | **2,460** | **10,557** |
| Double XOR256 (contrôle affine) | 49,248 | **0,000** | **0,000** | **6,247** |
| Négatif : trois portes128 | **24,810** | 49,280 | **24,821** | **27,970** |

### Récupération des règles cachées

Pour les **six tâches à deux portes non linéaires**,
les nombres de récupérations exactes des **deux
positions, entrées et tables** (sur cinq graines) sont :

| Tâche | Sans bruit | Bruit 1/16 |
|---|---:|---:|
| XOR+AND128 | 3/5 | 4/5 |
| AND+XOR128 | **5/5** | **5/5** |
| AND+AND128 | **0/5** | **0/5** |
| XOR+OR128 | **5/5** | **5/5** |
| XOR+AND256 | **5/5** | 4/5 |
| AND+AND256 | 4/5 | 3/5 |

**43/60 récupérations strictes** des règles cachées
sur les six tâches non linéaires à deux portes.
Le double XOR256, déjà représentable par A6-C8,
reste à **10/10** exact ; la tâche identité est
préservée à 10/10, et la porte AND seule à 10/10.

Total de l'ensemble des neuf tâches représentables :
**73/90 paramètres/structures strictement retrouvés**.
Ce taux est plus informatif que de n'afficher que
les taux d'erreur moyens, car plusieurs mauvaises
représentations produisent des erreurs concentrées
au début de la séquence puis diluées dans le long TEST.

### Succès établis

**AND+XOR128 et XOR+OR128** : les deux portes
sont retrouvées sur les 10/10 variantes
propres/bruitées de chaque tâche ;
la prédiction propre est rigoureusement
exacte à 16 384 pas. La corruption des labels
se retrouve sous forme d'environ 6,25 % d'erreur.

**XOR+AND256** : les 5/5 modèles propres
sont exacts à 16 384 pas, y compris une
mutation au slot j=1 dont l'effet traverse
255 pas pour parvenir à la sortie finale.
Le panneau bruité est **moins robuste** :
4/5 règles exactes, erreur moyenne
**10,565 %**, supérieure au bruit nominal
de 6,25 %.

**XOR+AND128** (ancien témoin d'échec A6-C8) :
l'erreur propre passe de **24,834 %**
avec A6-C7 à **0,140 %** avec A6-C9 ;
mais seulement **3/5** règles sont
retrouvées exactement sans bruit.
Les deux autres entraînements gardent
une erreur TRAIN/VAL notable malgré
une faible moyenne sur le TEST long.
Il serait incorrect de qualifier cette
tâche de résolue uniformément.

### Échecs ou succès partiels

**AND+AND128** : **aucun gain** sur
le contrôle A6-C7. Le système conserve
une seule porte sur les dix essais,
avec **12,370 %** d'erreur propre et
**17,082 %** sous bruit à 16 384 pas.
C'est un échec net du mécanisme conjoint
sur cette distribution.

**AND+AND256** : amélioration significative
sur le contrôle A6-C7, mais pas un
apprentissage fiable. Quatre des cinq
cas propres retrouvent deux portes exactes,
un échoue ; sous bruit, seulement **3/5**.
L'erreur moyenne est de **2,460 %**
sans bruit, **10,557 %** avec bruit.

**Contrôle négatif à trois portes** :
une architecture limitée à deux
mutations ne reproduit pas trois
dépendances indépendantes. Sur TEST,
**24,821 % d'erreur propre** et
**27,970 %** bruitée, pratiquement
identique à A6-C7. La validation
peut retenir 1 ou 2 portes partielles,
sans reconstruire la vraie fonction.
Ce contrôle n'est pas un succès.

### Ablations causales

Pour les modèles à deux portes
correctement apprises, remplacer
l'une ou l'autre porte par une identité
dégrade la sortie de façon marquée.
Exemple **AND+XOR128, panneau propre** :

- Deux portes apprises : **0 % d'erreur**.
- Sans la première : **49,291 %**.
- Sans la deuxième : **49,902 %**.

Pour XOR+AND128 :
deux portes retenues **0,140 %** ;
sans la première **24,915 %** ;
sans la seconde **49,534 %**.
Ces observations confirment la
contribution causale de deux
éléments dans le modèle choisi,
sans prouver la récupération exacte
dans les cinq graines.

## Coûts physiques et budgets exacts

Mesure du banc sur runner **GitHub Actions Ubuntu
x86_64 à quatre processeurs logiques, Rust 1.89**.
**Pas de Dell ni de mesure GPU.**

| Largeur | Ensemble de cas | Évaluations conditionnelles complètes TRAIN | Dont raffinement coordonné | Temps moyen total |
|---|---|---:|---:|---:|
| 128 bits | Amorces A6-C7 disponibles | 72 577 | 64 512 | ≈419 ms |
| 256 bits | Amorces A6-C7 disponibles | 146 305 | 130 048 | ≈880 ms |
| 128 bits | Identité (pas d'amorce A6-C7) | 32 257 | 32 256 | ≈213 ms |

En amont, les moteurs A6-C7 et A6-C8 ont
**leurs propres recherches**, exécutées dans
le même fit A6-C9, avec des budgets non
égalisés. Le temps ci-dessus comprend
ces contrôles et les oracles natifs.

Chaque évaluation conditionnelle utilise
**N mots u64 réemployés** :
1 024 octets utiles pour 128 bits d'état
×64 lanes, 2 048 octets pour 256 bits.
La mémoire **réelle totale** est plus
grande (données, réseaux, coefficients,
tables du contrôle A6-C8, métadonnées Vec),
et n'a pas été mesurée comme RSS.

Débit natif de la sortie sélectionnée
dans les TEST de 16 384 pas :
environ **160 millions de bit-lanes/s**
à N128, **81 millions de bit-lanes/s**
à N256. **Ce ne sont pas des tokens
de langage par seconde**, et aucun
facteur d'accélération général n'est revendiqué.

## Provenance et reproduction

- Code du banc réellement mesuré :
  SHA **79c693253c477121c5c9611b76844b2f6e92ac72**.
- [Exécution A6-C9 verte, 100/200/40 résultats](https://github.com/Memorithm/SML-GENIUS/actions/runs/38175487278/job/114576880609).
- [CI générale Rust du SHA mesuré](https://github.com/Memorithm/SML-GENIUS/actions/runs/38175487344) : fmt,
  check, Clippy, tests et smoke **verts**.
- Oracles unitaires : toutes les paires de
  tables booléennes 16×16 vérifiées contre
  la trajectoire native pour un anneau
  contrôlé ; test de fuite de validation
  et rejet de formes invalides.
- Les marqueurs machine exigés ont
  été observés : **100** A6C9_FROZEN,
  **200** A6C9_TEST_RESULT,
  **40** A6C9_SUMMARY, un
  A6C9_ALL_FROZEN avant le TEST.

Commandes de reproduction Rust 1.89 :

```bash
cargo fmt --all -- --check
cargo test --locked -p sml-core anf_nonlinear_joint_credit
cargo test --locked -p sml-core --bin sml-nonlinear-credit-bench
cargo run --locked --release -p sml-core --bin sml-nonlinear-credit-bench -- 5
```

## Verdict et décision scientifique

**PASS partiel A6-C9** : apprentissage
joint de certaines paires ANF
non linéaires XOR+AND, AND+XOR, XOR+OR,
et parfois AND+AND, jusqu'à
256 bits d'état récurrent avec
un scorer causal exact et
supervision terminale uniquement.

**FAIL explicite** sur AND+AND128
et sur plusieurs graines bruitées ;
les deux AND ont encore un
problème d'attribution de crédit
qu'une recherche coordonnée à
partir d'amorces simples/affines
ne surmonte pas systématiquement.

La prochaine question sera
d'explorer un mécanisme de
propositions non linéaires
véritablement **jointes**,
plutôt que seulement des
variations unilatérales.
Mais **aucun banc A6-C10
n'est lancé ici** : restituer
d'abord tous les résultats
à l'utilisateur et attendre
son accord, selon notre
discipline expérimentale.
