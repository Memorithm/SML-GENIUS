# A6-C7 — Attribution de crédit à une porte ANF intermédiaire dans un état récurrent 128/256 bits

**Statut : expérience A6-C7 mesurée, banc spécialisé vert.**
Cette expérience est distincte d'A6-C6. La PR #487 demeure en
brouillon jusqu'à restitution et accord utilisateur.
**Aucune expérience A6-C8 n'a été lancée.**

Le protocole a été versionné **avant la première exécution** :
`docs/A6-C7-INTERMEDIATE-CREDIT-PREREGISTRATION.md`.

## Hypothèse et périmètre exact

A6-C6 maintenait des structures jusqu'à 256 bits d'état, avec une
porte-racine ANF entraînée mais **des registres intermédiaires tous
fixés sur l'identité**. A6-C7 autorise une **porte intermédiaire
initialement inconnue** à apprendre sa position, son canal de source
et sa table booléenne complète, uniquement à partir de la
**perte sur les sorties finales**.

Dynamique réelle du modèle :

```text
s0(t+1) = s_(N-1)(t) XOR x1(t)       [racine FIXÉE]
sj(t+1) = f(s_(j-1)(t),x_r(t))      [UNE porte inconnue]
si(t+1) = s_(i-1)(t)                [autres registres FIXÉS]
prediction(t) = x0(t) XOR s0(t+1)  [lecture FIXÉE]
```

Le moteur ne reçoit **aucune étiquette des portes internes**. Il choisit
sur TRAIN seul :

- la position de la porte modifiée `j=1..N-1`,
- le canal brut `r` parmi `x1..x4`,
- la table à deux entrées `f` parmi **16 fonctions**.

Chaque proposition est classée par le taux d'erreur du seul label
final. Une absence de gain train conserve l'identité. Le candidat
gagnant est converti en véritable porte ANF GF(2) (Möbius), puis
ses erreurs TRAIN et VALIDATION sont recalculées indépendamment
par la simulation native `LocalAnfNetwork`, qui doit être
strictement égale au moteur de scores `u64`. Le candidat
n'est engagé qu'en cas de gain de VALIDATION strictement positif.

**Limite principale :** la racine XOR, la lecture finale et la
topologie annulaire de toutes les autres portes demeurent connues
et fixes. C'est l'apprentissage d'**une porte interne**, non
l'entraînement libre de 128 ou 256 portes interdépendantes.

## Protocole et séparation des jeux de données

Sept tâches déclarées avant exécution :

| Tâche | N | Fonction cachée injectée par le générateur |
|---|---:|---|
| Identité 128 | 128 | aucune |
| XOR 128 | 128 | `j=45, r=2, truth=6` |
| AND 128 | 128 | `j=92, r=3, truth=8` |
| XOR 256 profonde | 256 | `j=1, r=2, truth=6` |
| XOR 256 centrale | 256 | `j=127, r=2, truth=6` |
| AND 256 tardive | 256 | `j=192, r=3, truth=8` |
| **Contrôle négatif** | 128 | **deux** XOR indépendantes `j=31,r=2` et `j=88,r=3` |

Le modèle générateur utilise un vrai `LocalAnfNetwork`,
mais **seules les bitplanes d'entrée et les étiquettes finales**
passent à l'API d'apprentissage : les fonctions cachées
ne sont accessibles que dans le générateur indépendant,
pour vérifier leur récupération après entraînement.

Panneaux : données sans bruit, et étiquettes corrompues
indépendamment avec Bernoulli(1/16).

**5 graines × 7 tâches × 2 panneaux = 70 politiques figées**
sur TRAIN/VALIDATION. Chaque split possède **512 pas ×
64 séquences**. Seulement après gel des 70 décisions :
deux jeux de TEST indépendants de **4 096 et 16 384 pas
×64 séquences**, total **140 résultats** et **28 résumés**.

Deux oracles indépendants doivent produire la même perte :
l'évaluateur optimisé à deux files circulaires, et la
simulation complète des **128 ou 256 portes ANF** dans
l'exécuteur `LocalAnfNetwork`.

Budget de recherche train fixe :

- état 128 : **8 128** candidates `127 ×4 ×16`,
- état 256 : **16 320** candidates `255 ×4 ×16`.

Le score annulaire utilise exactement N mots `u64`,
sans allocation d'état par candidat ; le coût de chaque
candidat est **O(nombre de pas)**.

## Résultats réels sur TEST indépendant à 16 384 pas

**Taux d'erreur binaire (%)**, moyenne de 5 graines ;
plus bas signifie meilleur.

| Tâche | Baseline identité, sans bruit | ANF appris, sans bruit | ANF appris, bruit 1/16 |
|---|---:|---:|---:|
| Identité 128 | **0,0000** | **0,0000** | **6,2419** |
| XOR128, j=45, x2 | 49,7448 | **0,0000** | **6,2518** |
| AND128, j=92, x3 | 49,2143 | **0,0000** | **6,2444** |
| XOR256, j=1, x2 | 49,2244 | **0,0000** | **6,2498** |
| XOR256, j=127, x2 | 49,6066 | **0,0000** | **6,2588** |
| AND256, j=192, x3 | 48,4453 | **0,0000** | **6,2643** |
| **Deux XOR128, témoin négatif** | 49,8788 | **49,7031** | **49,7295** |

La baseline avec bruit de l'identité, dont le modèle n'a
aucune erreur de représentation, reçoit uniquement les
corruptions de labels, d'où le **6,24 %** observé.

**Récupération de structure** :

- **50/50** cas à une porte cachée retrouvent **exactement**
  la position `j`, le canal `r` et les quatre valeurs
  de `truth`, avec et sans bruit.
- La tâche **identité** ne déclenche **aucune mutation
  dans 10/10 cas** (5 graines × 2 panneaux).
- Au total, les **60/60 configurations représentables**
  retrouvent la bonne hypothèse de la famille
  « identité ou une mutation ».
- Les **50** propositions de mutation véritables sont
  acceptées sur validation ; l'identité ne modifie pas
  les coefficients du réseau dans les 10 cas propres
  ou bruités.
- Les **25 modèles à mutation unique sans bruit**
  atteignent **zéro erreur sur TEST 16 384 pas** ;
  avec bruit, l'erreur est proche du plancher
  attendu de 6,25 %.

Le cas XOR en position **j=1 dans 256 bits** exige
que l'effet de la porte interne soit propagé
**sur 255 pas jusqu'à la sortie**. Il est
identifié correctement sur les 10 configurations
de ce motif (avec/sans bruit) et conserve zéro erreur
sur le test long propre. C'est le point le plus
intéressant en matière de crédit temporel profond.

## Ablations, impossibilité observée et risque de validation

Si la porte apprise est remplacée par l'identité,
les cinq tâches à mutation ont une erreur de
**48–50 %**, contre zéro sans bruit lorsqu'elle
est conservée : la porte interne contribue
causalement à la sortie.

**Contrôle négatif** : le générateur contient
deux portes XOR à deux positions différentes,
avec deux canaux indépendants `x2` et `x3`.
L'apprenant A6-C7 ne dispose que **d'une**
porte à modifier, avec un seul canal brut ; il
ne peut représenter l'intégralité du signal.

L'expérience est conforme à cette limite :

- Erreur test 16 384 pas propre :
  **49,7031 %**.
- Erreur test bruitée :
  **49,7295 %**.
- **10/10** entraînements négatifs produisent
  malgré tout une proposition acceptée
  sur validation courte, à partir de
  faibles améliorations fortuites.

Ce résultat doit absolument rester visible :
l'optimisation du critère de validation sur de
courts exemples ne suffit pas à certifier
la capacité fonctionnelle de l'architecture.
Nous ne déclarons **aucun succès** sur le négatif.

## Mesures physiques et capacité logique

Plateforme de référence : **GitHub-hosted Ubuntu
x86_64, quatre processeurs logiques, Rust 1.89.0**,
pas le Dell.

| Largeur | Hypothèses train | Charge utile des états sur 64 séquences | Temps moyen train/val | Débit natif moyen 16 384 pas |
|---|---:|---:|---:|---:|
| 128 bits | 8 128 | **1 024 octets** | **≈28–29 ms** | **≈160–162 millions de valeurs binaires/s** |
| 256 bits | 16 320 | **2 048 octets** | **≈56–57 ms** | **≈82–83 millions de valeurs binaires/s** |

Ces temps incluent exploration du train et acceptation
sur validation. Les débits mesurent l'exécution native
en **bit-lanes sur séquences synthétiques**, pas des
tokens/s d'un modèle de langage.

Le stockage d'état ci-dessus compte les N mots `u64`
pour 64 séquences parallèles. Il ne couvre ni la RAM
totale du programme ni les `Vec`, tableaux ANF,
métadonnées, ni les allocations temporaires. Le
calcul d'une hypothèse utilise une file annulaire
dont les N mots sont réemployés, et la vérification
native exécute bien les **N portes**.

## Reproductibilité

- Première révision complète **mesurée** :
  `81b498e1e83b644a94a19011cea32a409dda8654`.
- Banc autonome :
  [GitHub Actions — A6-C7 Intermediate Credit](https://github.com/Memorithm/SML-GENIUS/actions/runs/38170200000).
  La source de vérité des mesures est le job
  `intermediate-credit` rattaché à ce SHA.
- Les tests de format, compilation, Clippy
  et oracles natifs sont verts. La CI générale
  sera également vérifiée sur le commit
  final comprenant ce rapport.
- Les marqueurs de sortie imposés sont
  `A6C7_FROZEN` (70),
  `A6C7_ALL_FROZEN,true` (1),
  `A6C7_TEST_RESULT` (140),
  `A6C7_SUMMARY` (28).

Commandes de reproduction après checkout :

```bash
cargo fmt --all -- --check
cargo test --locked -p sml-core anf_intermediate_credit
cargo test --locked -p sml-core --bin sml-intermediate-credit-bench
cargo run --locked --release -p sml-core --bin sml-intermediate-credit-bench -- 5
```

## Verdict scientifique et suite

**PASS A6-C7** sur le périmètre « UNE porte ANF
intermédiaire inconnue dans une mémoire annulaire
native de 128/256 bits » : position, canal
et fonction effectivement appris depuis la
perte terminale, y compris à 255 pas de la sortie.

**FAIL volontaire** pour la tâche composée à
deux portes, qui requiert une attribution de
crédit à plusieurs portes intermédiaires et
ne rentre pas dans la capacité de cette étape.

Le résultat constitue un progrès vers l'apprentissage
constructif de banques ANF mais ne démontre
pas encore une architecture libre ni
l'apprentissage simultané de centaines de
portes cachées. Il ne concerne pas
la précision de prédiction du langage,
le modèle SML-25B, les GPU ni une mémoire
générale de 16 384 tokens.

**Aucune A6-C8** ne sera lancée avant retour
utilisateur sur ces résultats.
