# A6-C10 — Crédit conjoint non linéaire par identification de sortie récurrente

**Statut : résultat expérimental mesuré, réussite du banc, échecs conservés.**
La PR #490 reste en brouillon ; aucune A6-C11 n'a été lancée.

Protocole fixé **avant la première exécution** :
`docs/A6-C10-JOINT-NONLINEAR-PREREGISTRATION.md`.

## 1. Résultat scientifique central

**A6-C10 résout, dans sa grammaire contrainte, l'échec AND+AND128
observé dans A6-C9.** La recherche conjointe identifie les positions,
les deux canaux d'entrée et les deux tables ANF complètes, malgré
l'absence de cible supervisée pour chacune des deux portes cachées.

Sur dix tâches synthétiques, neuf sont représentables et
une (trois injections cachées) est volontairement hors capacité.

Pour les **sept tâches à deux portes représentables**, le moteur
A6-C10 a retrouvé **70/70 paires de portes strictement identiques**
au générateur (7 tâches × 2 panneaux × 5 graines). Ce chiffre
comprend les tables exactes AND, XOR et OR, non seulement
une sortie fonctionnellement équivalente.

Pour les **neuf tâches représentables** (sept paires, une tâche
à une seule porte et une identité), les règles finalement retenues
sur validation sont strictement correctes dans **90/90 essais**.
Certaines règles exactes provenaient déjà du contrôle A6-C9 ;
elles ne sont pas attribuées artificiellement au nouvel algorithme.
Le nouveau **couple proposé** est quant à lui strictement correct
sur **70/70** des cas à deux portes.

## 2. Modèle et mécanisme réellement appris

Le banc exécute un automate booléen GF(2) à **128 ou 256 bits
d'état par séquence**, sur 64 séquences parallèles `u64` :

```text
s0(t+1) = s_(N-1)(t) XOR x1(t)       [racine FIXÉE]
sj(t+1) = f(s_(j-1)(t), x_r(t))     [porte interne apprise]
sk(t+1) = g(s_(k-1)(t), x_q(t))     [porte interne apprise]
si(t+1) = s_(i-1)(t)                [toutes les autres portes FIXÉES]
y_hat(t) = x0(t) XOR s0(t+1)       [lecture FIXÉE]
```

Les positions distinctes `1<=j<k<N`, les sources brutes
`r,q∈{1,2,3,4}` et les deux **tables complètes de 16
fonctions chacune** sont apprises sur TRAIN. Il n'existe
aucun label d'état interne fourni au moteur.

### Identification conjointe à partir de la sortie passée

La sortie supervisée passée est elle-même observable :

```text
R_t = y_t XOR x0_t
z_t = R_t XOR x1_t
z_t = g(f(R_(t-N), x_r(t-(N-j))), x_q(t-(N-k)))   (t >= N, sans bruit)
```

Cet objectif utilise `R_(t-N)` comme **observation passée de
la sortie finale TRAIN**, c'est-à-dire avec teacher forcing de
la sortie autorégressive en phase **d'identification uniquement**.
L'algorithme ne reçoit pas `f` ni `g`, leurs sorties,
des états cachés ou les étiquettes TEST.

La nouvelle recherche teste **conjointement** les
positions et canaux dans la représentation
`(Rpassé,rawA,rawB)`, puis factorise les tables
3-entrées obtenues en **deux portes ANF**, avant
de soumettre les réseaux complets à une
exécution autonome sur TRAIN. Les décisions
finales utilisent VALIDATION seulement.

**Il ne s'agit pas d'une rétropropagation
universelle** : la méthode exploite explicitement
une sortie d'état observable et une
topologie annulaire connue.

## 3. Protocole et recherche (respectés)

- **10 tâches** : identité128, AND unique128,
  XOR+AND128, AND+XOR128, AND+AND128,
  XOR+OR128, XOR+AND256 (porte profonde j=1),
  AND+AND256, XOR+XOR256, et témoin
  négatif à trois mutations128.
- **5 graines indépendantes ×2 panneaux** :
  étiquettes intactes ou bruit Bernoulli(1/16).
- Pour chaque cas, **512 pas d'entraînement
  ×64 lanes**, validation indépendante de même taille.
- **100 modèles gelés avant génération des jeux
  TEST indépendants**, puis 4 096 et 16 384
  pas ×64 lanes : **200 résultats et 40 synthèses**.
- Aucune sélection de fonction ou source via
  les données VAL/TEST ; seule la décision
  de conserver la nouvelle paire plutôt
  que le contrôle A6-C9 passe par VAL.

### Coût de l'identification

| Largeur | Structures position/sources sur sonde TRAIN | Meilleures structures conservées | Vérités 3-entrées factorisables | Réévaluations autonomes TRAIN |
|---|---:|---:|---:|---:|
| 128 | **128 016** | 64 | 88 | 256 |
| 256 | **518 160** | 64 | 88 | 256 |

La sonde est constituée des **64 derniers pas TRAIN**
(avec t>=N). Pour chacune des 64 structures,
les 8 histogrammes conditionnels sont recalculés
sur tout TRAIN ; on représente les 16×16 couples
de tables par **88 fonctions 3-entrées distinctes**,
puis seules les 4 meilleures factorisations
passent dans la véritable simulation autonome TRAIN.
Cette réduction est une heuristique **avec faisceau**,
pas la preuve d'un optimum global sur toutes les
positions et tous les couples de fonctions.

Le scorer temporel à trois segments de A6-C9
et le réseau GF(2) complet `LocalAnfNetwork`
doivent fournir exactement le même nombre
d'erreurs TRAIN pour le modèle sélectionné ;
le contrôle est également reproduit sur VAL.
**Les TEST utilisent exclusivement l'exécuteur
natif autonome, sans teacher forcing.**

## 4. Résultats : TEST indépendant de 16 384 pas

**Erreurs binaires moyennes sur cinq graines, en % ;
plus faible = meilleur.**

| Tâche | A6-C9 propre | A6-C10 retenu propre | A6-C10 retenu bruit 1/16 |
|---|---:|---:|---:|
| Identité128 | 0,0000 | **0,0000** | 6,2598 |
| AND unique128 | 0,0000 | **0,0000** | 6,2395 |
| XOR+AND128 | 0,0679 | **0,0000** | **6,2525** |
| AND+XOR128 | 0,0000 | **0,0000** | 6,2600 |
| **AND+AND128** | **12,4047** | **0,0000** | **6,2423** |
| XOR+OR128 | 0,0000 | **0,0000** | 6,2526 |
| XOR+AND256, porte j=1 | 0,0000 | **0,0000** | **6,2380** |
| **AND+AND256** | **2,4640** | **0,0000** | **6,2607** |
| XOR+XOR256 | 0,0000 | **0,0000** | 6,2305 |
| **Trois portes128, témoin négatif** | 24,8653 | **24,8653 (échec)** | **27,9762 (échec)** |

**Ces contrôles A6-C9 ont été ré-entraînés
sur les MÊMES TRAIN/VAL et les MÊMES tests A6-C10**
avec de nouvelles graines, et ne sont pas
des pourcentages simplement copiés du rapport
A6-C9 précédent.**

### Tâche bloquante AND+AND128 : zéro erreur

La famille précédente conserve sa meilleure
porte isolée et échoue sur la composition :

- A6-C7/A6-C9 : **12,4047 % d'erreur**
  TEST propre à 16 384 pas.
- A6-C10 : **0,0000 %** sur **5/5**
  graines sans bruit.
- A6-C10 avec corruption 1/16 :
  **6,2423 %** sur **5/5 graines**, près
  du bruit nominal 6,25 %.
- Les deux positions et deux fonctions
  AND cachées sont strictement retrouvées
  dans **10/10 variantes** (avec/sans bruit).

Sur la tâche AND+AND256, le score précédent
**2,4640 %** devient également **0 %**
sur cinq graines propres, avec **6,2607 %**
sur les données à labels bruités.

### Réduction des autres limites de A6-C9

- XOR+AND128 : **0,0679 → 0 %** propre ;
  l'ensemble des 10 variantes retrouve
  les deux portes exactes.
- XOR+AND256 bruité : **10,5452 → 6,2380 %** ;
  les 5/5 cas bruités sont strictement corrects.
- AND+XOR128, XOR+OR128 et XOR+XOR256
  restent exacts ; pas de régression.
- Pour les tâches à zéro/une seule porte,
  la validation conserve le contrôle A6-C9
  à représentation minimale, sans introduire
  de mutation fictive.

Le nouvel algorithme propose une paire exacte
même lorsque le contrôle A6-C9 déjà sélectionné
atteint zéro erreur ; dans ce cas la validation
conserve l'ancien modèle sur égalité,
ce qui explique que **la nouvelle origine
ne soit pas systématiquement sélectionnée**
malgré les 70/70 nouvelles propositions exactes.

## 5. Attribution causale : ablations

Dans les modèles AND+AND, supprimer une
seule porte n'amène **pas** nécessairement
50 % d'erreur, car les fonctions sont
conjonctives. Les valeurs réelles sont :

| Cas propre 16K pas | Deux portes | Sans première | Sans deuxième |
|---|---:|---:|---:|
| **AND+AND128** | **0 %** | 16,5088 % | 16,4858 % |
| **AND+AND256** | **0 %** | 16,3315 % | 16,3467 % |
| **XOR+AND128** | **0 %** | 24,8406 % | 49,2369 % |

L'ablation des deux composants démontre leur
utilité causale dans la solution, sans
faire d'affirmation erronée sur le taux
de 50 % pour des compositions AND.

## 6. Témoin négatif : trois portes cachées

La grammaire contient au maximum deux
mutations. Le générateur négatif comprend
trois injections indépendantes XOR/AND/XOR.
Le nouveau modèle joint donne :

- **24,8673 %** TEST propre, comparable
  au précédent **24,8653 %**.
- **27,9762 %** avec bruit, identique
  au contrôle A6-C9 arrondi à quatre décimales.

La validation ne retient pas la nouvelle
paire lorsque son score est moins favorable.
Le témoin n'est donc **pas résolu**.
C'est un résultat indispensable pour ne
pas surestimer la portée du crédit
conjoint à deux portes.

## 7. Performances, mémoire et provenance

Mesures sur un **runner GitHub Actions
Ubuntu x86_64 à 4 processeurs logiques,
Rust 1.89.0**. Ni Dell, ni GPU.

| Groupe | Nombre de structures sondées | Temps fit TRAIN+VAL moyen | Débit de sortie ANF natif à 16K pas |
|---|---:|---:|---:|
| 128 bits, tâche deux portes | 128 016 | **≈627–646 ms** | ≈194–196 M bit-lanes/s |
| 256 bits, tâche deux portes | 518 160 | **≈1 797–1 817 ms** | ≈98–99 M bit-lanes/s |
| 128 bits, identité | 128 016 | **≈443–444 ms** | ≈199 M bit-lanes/s |

Les durées comprennent les **contrôles A6-C7/8/9**
ré-exécutés sur les mêmes TRAIN/VAL, ainsi
que l'identification conjointe A6-C10.
Les budgets ne sont pas égalisés, et
le débit ci-dessus concerne des **bit-lanes
synthétiques**, jamais des tokens/s du SML-25B.

- Tampon d'état/scoreur : N mots u64 pour 64
  séquences, soit **1 024 octets** à N128
  ou **2 048 octets** à N256.
- Tas de 64 structures : **1 024 octets
  de charge utile** (hors en-têtes Vec/BinaryHeap).
- Les observations TRAIN, tables ANF,
  modèles complets, allocations du moteur,
  métadonnées et RAM totale ne sont **pas**
  incluses dans ces mesures.

## 8. Source, audit et limites

- Révision du premier benchmark complet réussi :
  **89f453256cc7725d5be5c104517205e7443a63d3**.
- [Journal GitHub Actions A6-C10, 100/200/40 et
  10 tâches](https://github.com/Memorithm/SML-GENIUS/actions/runs/38177489605/job/114582842773).
- Les contrôles ciblés format/compilation/Clippy,
  équivalence des 256 factorisations booléennes,
  apprentissage AND+AND, absence de fuite
  de validation, oracle natif et 100/200/40
  ont tous réussi.
- La suite CI générale et le replay sur le
  SHA documentaire final sont vérifiés
  séparément.

Pour reproduire :

```bash
cargo fmt --all -- --check
cargo test --locked -p sml-core anf_joint_nonlinear_system_id
cargo test --locked -p sml-core --bin sml-joint-nonlinear-id-bench
cargo run --release --locked -p sml-core --bin sml-joint-nonlinear-id-bench -- 5
```

**Limites principales :** deux mutations maximales,
anneau et lecture fixés ; besoin de sorties
supervisées passées en TRAIN, sans lesquelles
le raccourci d'identification n'est pas
défini ; heuristique à faisceau (64 structures)
plutôt que preuve de recherche globale ;
pas de corpus naturel ni de graphe libre
de 256 portes, ni de preuve de qualité
linguistique ou de gain GPU.

**Verdict : PASS A6-C10** sur le périmètre
expérimental, avec correction mesurée de
l'échec AND+AND128 et consolidation
d'AND+AND256/XOR+AND bruité.
Le témoin à trois portes reste
négatif comme préenregistré.

**Stop scientifique :** ne pas commencer
A6-C11 avant de présenter ces résultats
et d'obtenir la décision utilisateur.
