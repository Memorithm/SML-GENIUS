# A6-C5 — Apprentissage de cycles ANF récurrents et extrapolation temporelle

**Statut : expérience exécutée et mesures vérifiées.** La PR #484 reste en
brouillon. Aucune A6-C6 n'a été lancée.

## Hypothèse falsifiable

Les expériences A6-C1..C4 démontrent l'apprentissage constructif de portes
ANF et la découverte de topologies causales **acycliques** dans une grammaire
réduite. A6-C5 pose une question distincte : **le réseau peut-il apprendre
lui-même les arêtes de rétroaction, sans cibles cachées, puis continuer à
fonctionner sur des séquences très au-delà de la longueur d'entraînement ?**

Architecture effectivement apprise :

```text
s0(t+1) = f(s0(t), s1(t), x(t))
s1(t+1) = g(s0(t), s1(t), x(t))
ŷ(t) = x0(t) XOR s1(t+1)
```

Les deux portes ont une **arité exactement égale à deux**, parmi six sources
candidates distinctes (les deux bits d'état précédents et quatre canaux
d'entrée actuels). Les sorties se mettent à jour **simultanément**,
uniquement à partir des états de `t`, jamais d'un état futur.
Le readout `x0 XOR s1` est figé et implémenté via l'ANF natif.

Il n'existe **aucune étiquette de cible pour s0 ou s1**. L'apprenant reçoit
seulement les séquences de bits bruts et les étiquettes de sortie observée.
Les tables de vérité booléennes sont converties en coefficients ANF grâce
au moteur Rust existant.

## Cinq tâches prévues avant exécution

| Nom | Cible résiduelle après retrait de x0(t) | Mémoire minimale attendue |
|---|---|---|
| `direct` | `x1(t) AND x2(t)` | Aucune arête récurrente |
| `lag1` | `x1(t-1)` | Un saut acyclique |
| `running_parity` | XOR de tous les `x1(0..t)` | Une porte avec auto-rétroaction |
| `stride2_parity` | `x1(t) XOR x1(t-2) XOR x1(t-4)...` | Cycle mutuel de deux portes |
| `stride3_unrepresentable` | `x1(t) XOR x1(t-3) XOR x1(t-6)...` | Trois bits d'état indépendants nécessaires |

Le dernier motif est un **témoin négatif explicitement prévu** : un système
déterministe avec seulement deux bits d'état ne peut maintenir les trois
parités de phases indépendantes, qui exigent huit configurations
distinguables. En revanche, sa fonction est produite normalement comme
étiquette finale; aucune indication de son impossibilité n'est donnée
au moteur.

## Méthode, contrôles et barrière anti-fuite

Grammaire de recherche : `C(6,2) × 16 = 240` fonctions/source possibles
par porte, soit **57 600 couples** pour le réseau intégral. Les candidats
sont notés par l'erreur binaire **sur entraînement uniquement** via
opérations `u64` bit-slicées. Chaque modèle retenu est reconstruis en
portes ANF natives et ses erreurs d'entraînement sont recalculées par
simulation complète indépendante (`LocalAnfNetwork`), avec égalité exacte
requise. L'acceptation des réseaux entiers dépend seulement d'une
**amélioration stricte sur validation**.

Contrôles construits sur les mêmes exemples :

1. Réseau de deux portes nulles.
2. Meilleur readout direct (aucun état) : 96 configurations terminales.
3. Meilleur DAG acyclique à deux étages : 15 360 couples de portes.
4. Descente coordonnée stricte depuis l'initialisation nulle, une seule
   porte à la fois.
5. Descente coordonnée avec redémarrages, **plafonnée au même maximum de
   57 600 scores d'entraînement** que la recherche exhaustive.
6. Ablation native qui remplace toutes les sources d'état des portes
   apprises par leur valeur zéro, tout en conservant les canaux bruts.
   Ablation séparée de la première porte d'état.

Le plafond de scores est identique pour contrôle avec redémarrages et
recherche exhaustive, mais le nombre **effectif** peut être inférieur
si un modèle d'entraînement exact est trouvé avant la limite. Les
budgets des contrôles direct et DAG ne sont **pas égalisés**.

Chaque combinaison tâche/panneau utilise **5 graines indépendantes**,
deux panneaux (sans bruit et bruit des étiquettes Bernoulli(1/16)),
**64 pas d'entraînement ×64 voies = 4 096 observations**, puis
**64 pas de validation ×64 voies = 4 096 observations**.

La totalité des **50 modèles train/val** est figée avant de générer les
séquences de test. Chaque modèle est ensuite testé sur **deux séries
indépendantes** : 512 pas ×64 voies = 32 768 observations et
4 096 pas ×64 voies = 262 144 observations. Le compteur temporel
recommence à zéro pour chaque série; les états commencent à zéro.
Aucune réponse de test ne sert à modifier un réseau.

## Mesures réelles à 4 096 pas

**Taux d'erreur binaire en % sur données inédites**, moyenne des
5 graines, faible = meilleur.

| Tâche / panneau | Nulles | Direct | DAG acyclique | Coordonnée monotone | Redémarrages | Recherche libre de cycles |
|---|---:|---:|---:|---:|---:|---:|
| Direct, sans bruit | 25,024 % | **0,000 %** | **0,000 %** | **0,000 %** | **0,000 %** | **0,000 %** |
| Direct, bruit 1/16 | 28,022 % | **6,223 %** | **6,223 %** | **6,223 %** | **6,223 %** | **6,223 %** |
| Retard 1, sans bruit | 49,937 % | 49,948 % | **0,000 %** | 33,328 % | **0,000 %** | **0,000 %** |
| Retard 1, bruit 1/16 | 49,987 % | 49,984 % | **6,241 %** | 35,431 % | **6,241 %** | **6,241 %** |
| Parité cumulative, sans bruit | 49,939 % | 49,948 % | 49,924 % | **0,000 %** | **0,000 %** | **0,000 %** |
| Parité cumulative, bruit 1/16 | 49,983 % | 49,956 % | 50,019 % | **6,244 %** | **6,244 %** | **6,244 %** |
| Parité stride-2, sans bruit | 50,041 % | 50,025 % | 49,926 % | 50,016 % | **0,000 %** | **0,000 %** |
| Parité stride-2, bruit 1/16 | 50,031 % | 50,002 % | 50,007 % | 41,328 % | **6,221 %** | **6,221 %** |
| Parité stride-3 (négatif), sans bruit | 50,090 % | 50,097 % | 49,998 % | 50,031 % | 49,975 % | 49,975 % |
| Parité stride-3 (négatif), bruit 1/16 | 50,001 % | 49,943 % | 49,879 % | 49,950 % | 49,898 % | 49,898 % |

Le jeu court indépendant de **512 pas** conduit aux mêmes conclusions
qualitatives : les quatre fonctions représentables sont à **zéro erreur
sans bruit**, et les variantes bruitées ont environ **6,2 à 6,3 %**
d'erreur. L'essai de parité stride-3 échoue également et reste proche
de 50 % d'erreur. Les vingt lignes détaillées pour les deux horizons
sont dans les logs.

## Découverte de structure et attribution causale

Pour les quatre tâches réalisables, les **40/40 combinaisons**
(4 tâches × 2 panneaux × 5 graines) ont retrouvé la classe de
rétroaction attendue :

| Tâche | Classe découverte | Essais corrects |
|---|---|---:|
| Direct | `direct` | 10 / 10 |
| Retard 1 | `acyclic_delay` | 10 / 10 |
| Parité cumulative | `self_feedback` | 10 / 10 |
| Parité stride-2 | `mutual_feedback` | 10 / 10 |

Les **40/40** solutions ont été acceptées sur validation.
Les fonctions représentables ont zéro erreur sur les tests longs sans
bruit, même après **4 096 pas**, alors que l'entraînement ne voit que
64 pas par séquence.

L'ablation des rétroactions explicite leur nécessité :

- Calcul direct : la suppression des arêtes récurrentes ne change rien.
- Retard 1 : erreur de 49,937 % sans l'arête mémoire.
- Parité cumulative : erreur de 49,927 % sans auto-rétroaction.
- Parité stride-2 : erreur de 50,014 % sans le cycle mutuel.

Il ne suffit donc pas d'avoir deux bits d'état : le mécanisme apprend
et exploite effectivement des liens de rétroaction.

## Résultat crucial des contrôles de recherche

Le meilleur modèle à boucles et le contrôle avec redémarrages atteignent
le même minimum d'entraînement dans **les 50/50 essais** (dont les dix
essais impossibles, où le minimum n'est pas zéro).

Sur les cas propres représentables, la recherche à redémarrages trouve
généralement la solution exacte avec beaucoup moins de propositions
que l'énumération intégrale :

- Direct : **481** propositions, contre 57 600.
- Retard d'un pas : **1 922 à 12 012**.
- Parité cumulative : **481**.
- Parité stride-2 : **961 à 4 325**.

En présence de bruit ainsi que sur le témoin négatif, la méthode à
redémarrages consomme son maximum de **57 600 évaluations** dans les
cas observés, pour atteindre ou approcher l'optimum de l'exhaustivité.

Une descente coordonnée *sans* redémarrage apprend correctement le
calcul direct et l'auto-rétroaction simple, mais elle échoue sur les
boucles mutuelles et sur certaines dépendances d'un pas. Le résultat
confirme la nécessité de sortir des minima/plateaux locaux pour
l'attribution du crédit, **sans démontrer de supériorité de la
recherche exhaustive conjointe sur une exploration locale avec
redémarrages**.

## Échec délibéré et surapprentissage du témoin négatif

La parité stride-3 **ne généralise pas** sur les séquences longues :
environ 50 % d'erreur, même si l'entraînement propose parfois une
configuration avec un léger avantage empirique.

La sélection sur validation accepte **4/5** de ces modèles sans bruit
et **5/5** avec bruit, mais les tests hors distribution de longueur
révèlent leur inefficacité. C'est un contrôle important contre une
interprétation excessive de la simple amélioration sur validation.

Une machine ne disposant que de quatre configurations d'état (deux
bits) ne peut représenter les huit configurations distinguables des
trois parités de phase nécessaires. Cette limite concerne ce banc
déterministe précis; elle n'est pas une borne générale sur les
possibilités du système SML-GENIUS.

## Provenance et ressources

- Révision initiale mesurée :
  `62f8919b6f9be3597fc4fbd54049d4ff84364729`.
- [Journal A6-C5 et mesures brutes](https://github.com/Memorithm/SML-GENIUS/actions/runs/38155400000).
- Matériel : **runner GitHub Actions Ubuntu x86_64, 4 processeurs
  logiques**, Rust **1.89.0**. **Pas une mesure Dell.**
- CI expérimentale : formatage, compilation, Clippy, tests
  `anf_credit_cycles`, tests du benchmark et 50 entraînements /
  100 évaluations de test / 20 résumés : tous réussis.
- Les deux états logiques résidents sont deux mots `u64`
  bit-slicés pour **64 séquences simultanées** (soit deux bits
  d'état par séquence). Ce constat ne décrit **pas** la RAM totale
  du moteur d'entraînement ou du programme.
- Temps de préparation/sélection observé sur le runner :
  environ **16–18 ms** par configuration propre représentable,
  environ **30–32 ms** sous bruit ; le témoin négatif propre
  avoisine également 32 ms. Les coûts incluent l'exhaustivité
  et les contrôles à redémarrages. Ce n'est pas un débit
  d'inférence en tokens/s.

Reproduction Rust 1.89 :

```bash
cargo test --locked -p sml-core anf_credit_cycles
cargo test --locked -p sml-core --bin sml-cycle-credit-bench
cargo run --locked --release -p sml-core --bin sml-cycle-credit-bench -- 5
```

## Verdict, portée et limites

**PASS A6-C5** pour la découverte de deux classes de rétroaction
récurrente par supervision terminale, et pour l'extrapolation exacte
de ces fonctions booléennes déterministes jusqu'à 4 096 pas sur
des flux aléatoires indépendants.

**FAIL volontaire A6-C5** sur une tâche de parité périodique
nécessitant au moins trois bits d'état — témoin négatif confirmant
les limites de capacité et la nécessité de distinguer validation
locale et généralisation sur les horizons testés.

L'architecture et la grammaire restent extrêmement bornées :
deux bits d'état, deux fonctions à arité deux, sources candidates
limitées, tâches booléennes synthétiques, sortie fixée. Les longues
séquences prouvent la **réutilisation répétée d'un état calculable
exact**, pas une mémoire générique de milliers de tokens. Il
n'existe aucune preuve dans ces mesures d'amélioration de
précision de prédiction linguistique, de performances en SML-25B,
de graphes de grande taille, de débit GPU/SIMD ou de
généralisation sur corpus naturels.

Comme convenu : présenter l'ensemble des résultats à l'utilisateur
**avant de lancer A6-C6**, laisser la PR #484 en brouillon
jusqu'à validation et merge explicites.
