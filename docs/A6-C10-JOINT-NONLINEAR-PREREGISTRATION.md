# A6-C10 — Préenregistrement : recherche conjointe non linéaire par sortie observée

**Protocole établi le 11 octobre 2026, avant la première exécution du banc A6-C10.**
Base : SML-GENIUS/main après fusion effective de la PR #489.

## Hypothèse à falsifier

A6-C9 sait entraîner deux portes dans certains cas, mais reste
bloquée sur AND+AND128 : 12,37 % d'erreur TEST propre, et elle
échoue sur des graines AND+AND256, XOR+AND128 ou bruitées.

A6-C10 teste si **l'apprentissage conjoint des fonctions**
et des positions, en exploitant la sortie récurrente
*passée déjà observée*, permet de retrouver des **paires
AND+AND / XOR+AND** sans partir d'une porte individuelle
partiellement correcte.

L'objectif central est la récupération de **la fonction
composée** et la généralisation dans le vrai modèle ANF
en exécution libre (sans accès aux labels passés en inférence).
Les paramètres internes peuvent être non identifiables
à transformation de complément booléen près ; des scores
fonctionnels exacts et des correspondances strictes sont
rapportés séparément.

## Grammaire et dérivation causale

Le modèle conserve **N = 128 ou 256 bits d'état** par séquence,
64 voies parallèles u64, dans une ossature ANNULAIRE FIXÉE :

```text
s0(t+1) = s_(N-1)(t) XOR x1(t)      [racine FIXE]
sj(t+1) = f(s_(j-1)(t), x_r(t))    [porte cachée 1, position j]
sk(t+1) = g(s_(k-1)(t), x_q(t))    [porte cachée 2, position k>j]
si(t+1) = s_(i-1)(t)               [autres délais FIXÉS]
y_hat(t) = x0(t) XOR s0(t+1)       [readout FIXE]
```

Les indices j,k (positions distinctes) et les sources
r,q parmi x1..x4, puis **toutes les 16 vérités f**
et **toutes les 16 vérités g**, sont inconnus du
moteur d'apprentissage. Les tables retenues sont
converties via Möbius en véritables portes ANF.

Pour toute tâche de ce type, la sortie passée
*observée* fournit une équation autorégressive :

```text
R_t = y_t XOR x0_t                         [racine observable TRAIN]
z_t = R_t XOR x1_t                         [résidu sortie TRAIN]
z_t = g(f(R_(t-N), x_r(t-(N-j))), x_q(t-(N-k)))   [t >= N, sans bruit]
```

Le candidat est donc une **fonction booléenne composée à
trois entrées (Rpassé,rawA,rawB)**, construite depuis
les **sorties finales** seulement. AUCUNE cible de
porte cachée n'est fournie.

**Point méthodologique** : `R_(t-N)` est
*teacher-forcé* sur TRAIN, c'est-à-dire obtenu
à partir d'une **étiquette passée** et non
du véritable état interne fourni par le
générateur. Il n'est pas disponible pendant
le test autonome ; le test est obligatoirement
réalisé par l'exécuteur natif récurrent en
boucle ouverte. Avec bruit de labels,
l'erreur du passé contamine la cible
d'identification. Ce mécanisme d'identification
d'automate n'est PAS une recherche libre
sur toutes les topologies récurrentes.

## Recherche jointe préenregistrée

Étape 1 — **toutes les structures de deux portes** :

- N128 : `C(127,2)×4² = 128 016` couples
  (positions ordonnées et canaux r,q).
- N256 : `C(255,2)×4² = 518 160` couples.
- Sur les **64 derniers pas TRAIN**, t>=N :
  calculer pour chaque structure la meilleure
  erreur possible de *toute* table 3-entrées
  (borne inférieure par comptage sur 8 adresses).
  Le classement est issu de TRAIN uniquement.
- Conserver les **64 meilleures structures**
  avec un ordre déterministe des égalités.
  La borne vient d'une table 3-entrées
  générale, pas des sorties de chaque porte.

Étape 2 — **factorisation conjointe** :

- Pour chacune des 64 structures retenues,
  accumuler sur tout TRAIN, pour t>=N,
  les huit histogrammes conditionnels
  `(Rpast,rawA,rawB,label z)`.
- Enumerer les **16×16 = 256 couples**
  possibles de portes `(f,g)`,
  regroupés par fonction composée
  effective sur les huit adresses.
  Plusieurs réalisations peuvent être
  fonctionnellement équivalentes :
  départager par complexité ANF minimale
  puis ordre stable.
- Sur chaque structure, garder les **4
  meilleures fonctions composées selon TRAIN**.
- Évaluer ces propositions de façon
  *autonome et causale* sur **TRAIN complet**,
  par le scorer 3-segments ANF A6-C9,
  et sélectionner la meilleure proposition
  TRAIN parmi les 64×4 candidates.
- Reconstruire le gagnant dans le vrai
  `LocalAnfNetwork` et exiger l'égalité
  bit-à-bit sur TRAIN et VAL avant adoption.

Étape 3 — **choix de modèle sur VALIDATION seulement** :

Comparer le modèle A6-C9 déjà entraîné sur
les mêmes TRAIN/VAL (ses contrôles A6-C7/A6-C8
sont déjà intégrés) au nouveau couple
joint A6-C10. Ne sélectionner une paire
nouvelle que si elle réduit STRICTEMENT la
perte finale VAL du meilleur modèle ancien.

**Pas de validation dans le classement des
positions ou des tables de vérité.**
Aucune génération ni inspection de TEST avant
gel des politiques pour TOUTES les tâches.

Cette recherche est **une heuristique à deux
étages, et pas une preuve d'optimalité globale
sur toutes les paires** : une vraie structure
peut manquer le faisceau de 64 candidats
sur la petite sonde, notamment avec bruit.
Le résultat négatif devra être conservé.

## Dix tâches pré-déclarées

Utiliser de nouvelles graines distinctes d'A6-C9.

| Tâche | Largeur | Générateur connu uniquement des tests |
|---|---:|---|
| identity128 | 128 | aucune mutation |
| single_and128 | 128 | AND(8), j92/x3 |
| xor_and128 | 128 | XOR(6), j31/x2 ; AND(8), j88/x3 |
| and_xor128 | 128 | AND(8), j33/x2 ; XOR(6), j94/x3 |
| and_and128 | 128 | AND(8), j41/x2 ; AND(8), j87/x3 |
| xor_or128 | 128 | XOR(6), j29/x2 ; OR(14), j105/x3 |
| xor_and256 | 256 | XOR(6), j1/x2 ; AND(8), j190/x3 |
| and_and256 | 256 | AND(8), j51/x2 ; AND(8), j181/x3 |
| xor_xor256 | 256 | XOR(6), j1/x2 ; XOR(6), j191/x3 |
| negative_three128 | 128 | XOR(6) j19/x2, AND(8) j67/x3, XOR(6) j108/x4 |

Cinq graines par tâche, sans bruit et avec
corruption des labels Bernoulli(1/16)
indépendante. **100 choix TRAIN/VALIDATION**,
chaque split de 512 pas ×64 voies,
avec graines disjointes.

**Les 100 choix sont gelés avant la première
création de données TEST.** TEST indépendante :
4 096 et 16 384 pas ×64 voies, deux graines
supplémentaires ; **200 lignes de test, 40
lignes de synthèse**.

## Critères scientifiques et mesures

- Mesure primaire : taux d'erreur TEST autonome
  sur 16 384 pas, en plus de la perte TRAIN
  teacher-forcée et de l'oracle natif.
- Mesure de structure : deux positions, canaux
  et fonctions, exacts ou équivalents
  (gauge complémentaire), vérifiés séparément.
- Contrôles sur mêmes données : A6-C9,
  A6-C7, A6-C8, référence identité,
  dépose de chaque porte du nouveau modèle.
- Objectif de progrès : réduire AND+AND128
  par rapport aux **12,37 % précédents**,
  mais le résultat peut rester négatif ;
  éviter toute modification a posteriori.
- Vérifier régression XOR+AND, AND+XOR,
  XOR+OR, double XOR.
- Le témoin négatif trois portes doit rester
  incorrect pour la famille à deux mutations ;
  exposer l'éventuelle suracceptation VAL.
- Report de la complexité réelle : toutes
  structures sondées, factorisations,
  re-scores natifs, durée CPU et charge
  mémoire utile. Les budgets A6-C7/8/9/10
  ne sont pas égalisés.
- Tester le modèle natif exact, sans
  « teacher forcing », sur le TEST long.

**Limites** : le graphe annulaire,
le readout, la racine XOR et les identités
des autres portes sont fixés. Seulement
deux portes internes non linéaires
apprenables ; pas un entraînement autonome
de 128/256 portes quelconques. Aucun test
sur corpus naturel, prédiction linguistique
SML-25B, tokens/s ou GPU. Rust 1.89 sur
runner GitHub hébergé isolé, non Dell.

**A6-C11 ne sera pas ouvert** avant la
restitution complète des résultats A6-C10,
y compris tout échec, et revue utilisateur.
