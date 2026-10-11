# A6-C7 — Préenregistrement : attribution du crédit à une porte ANF intermédiaire (128/256 bits)

Statut : protocole publié **avant toute exécution expérimentale A6-C7**.
Expérience exclusivement Rust, sur une PR en brouillon dédiée.

## Hypothèse et rupture avec A6-C6

A6-C6 maintenait 128 ou 256 bits d'état récurrent mais toutes les
portes intermédiaires exécutaient l'identité, sans apprentissage.
Nous testons maintenant si le seul **résidu de sortie finale** peut
identifier l'emplacement, le canal et la fonction d'une **porte
intermédiaire** jusque dans un état récurrent de 256 bits.

**Ce n'est pas un réseau librement entraîné** : l'anneau de délais,
la porte racine XOR et le readout sont fixés, ainsi que l'hypothèse
qu'une seule porte intermédiaire diffère éventuellement de
l'identité. Le problème est délibérément plus restreint qu'un
apprentissage simultané de 256 portes, mais plus difficile
qu'A6-C6 : nous devons attribuer la perte terminale à une
fonction **située profondément à l'intérieur** de l'état.

## Dynamique, génération des données et architecture retenue

Pour N=128 ou 256 bits d'état, entrée bit-slicée x0..x4 :

```text
s0(t+1) = s_(N-1)(t) XOR x1(t)     [racine FIXÉE]
si(t+1) = s_(i-1)(t)                [portes d'identité FIXÉES]
sj(t+1) = f(s_(j-1)(t),x_r(t))     [UNE porte inconnue, position j >= 1]
y_hat(t) = x0(t) XOR s0(t+1)       [readout FIXÉ]
```

La fonction inconnue est une table booléenne complète à 2 entrées,
`0..15`, compilée en coefficients ANF natifs par Möbius. L'apprenant
doit retrouver j, r et f **uniquement grâce aux labels de la
sortie finale**. Il ne reçoit pas les cibles ni les états
internes du générateur.

Le bruit de label est un masque Bernoulli(1/16) indépendant,
ajouté **après** la simulation causale du circuit générateur.
Chaque flux redémarre à zéro. La simulation d'entraînement
utilise un cache circulaire `u64` avec deux segments,
équivalent à la propagation native complète du réseau.
Le gagnant et la baseline sont vérifiés contre un
`LocalAnfNetwork` de largeur N sur TRAIN et VALIDATION.
Les jeux de TEST sont générés uniquement **après le gel
de toutes les décisions**.

## Hypothèses concurrentes

1. **Identité** : aucune mutation (baseline ANNULAIRE A6-C6).
2. **Mutation inconnue** : j parmi 1..N-1, canal r parmi
   x1..x4, vérité binaire f parmi 16 tables. La table identité
   de s_(j-1) est une mutation nulle.
3. **Témoin négatif de capacité** : *deux* portes indépendantes
   injectent des signaux de canaux distincts dans deux positions
   intermédiaires. Une unique mutation binaire, limitée à un
   seul canal, ne peut expliquer exactement les deux
   dépendances indépendantes. Cet échec est recherché et
   ne doit pas être masqué.

La racine est maintenue sur `XOR(sN-1,x1)`. Dans cette
expérience, **les fonctions profondes sont apprises,
mais la racine et l'ossature de délais ne le sont pas**.
Éviter donc l'affirmation qu'A6-C7 entraîne librement
tous les coefficients et toutes les connexions.

## Protocole de mesure

Tâches préréglées :

| Tâche | N | Mutation cachée au générateur |
|---|---:|---|
| Référence 128 | 128 | aucune mutation |
| XOR 128, milieu | 128 | j=45, canal x2, XOR |
| AND 128, tardive | 128 | j=92, canal x3, AND |
| XOR 256, profonde | 256 | j=1, canal x2, XOR |
| XOR 256, milieu | 256 | j=127, canal x2, XOR |
| AND 256, tardive | 256 | j=192, canal x3, AND |
| Négatif, double XOR 128 | 128 | j=31:x2/XOR ET j=88:x3/XOR |

Cinq graines indépendantes pour chaque tâche, panneaux
**sans bruit** et **bruit 1/16** : **70 entraînements**
(7 tâches ×2 panneaux ×5 graines).

TRAIN : **512 pas ×64 voies** ; VALIDATION : idem avec
graines distinctes. Une fois **les 70 politiques gelées**,
deux TESTS indépendants de **4 096 et 16 384 pas ×64 voies**
sont générés : **140 résultats** et **28 synthèses**
(7 tâches ×2 panneaux ×2 horizons).

L'échec du témoin négatif doit être quantifié et
confronté aux améliorations éventuellement acceptées sur
la seule validation courte.

## Algorithme / limite de recherche

Pour N=128 : `127 ×4 ×16 = 8 128` hypothèses.
Pour N=256 : `255 ×4 ×16 = 16 320` hypothèses.

Chaque hypothèse est évaluée sur la seule sortie finale,
avec deux tampons circulaires de taille totale **N mots u64**,
réutilisés sans allocation dans la boucle d'exploration,
à coût **O(512)** par hypothèse. La comparaison favorise
la configuration sans mutation lorsque le gain est nul.
La validation ne sert qu'à accepter/rejeter la meilleure
mutation trouvée sur entraînement (condition stricte).

Le véritable `LocalAnfNetwork` exécutera N portes
à chaque pas, y compris l'unique porte apprise,
afin de fournir une **égalité de scores native**.
Mesurer séparément recherche train+validation, coût mémoire
logique et vitesse d'inférence native (bit-lanes/s,
PAS tokens linguistiques/s).

## Critères d'interprétation et falsification

- **PASS fonctionnel** : retrouvaille de j, canal et
  fonction sur les cinq graines des tâches à mutation
  unique, avec zéro erreur sans bruit sur 16 384 pas et
  bruit proche de 6,25 %.
- **PASS identité** : le réseau baseline annulaire reste
  intact sur la tâche sans mutation.
- **FAIL attendu au négatif** : une seule mutation ne
  reproduit pas exactement deux injections indépendantes ;
  présenter le taux de TEST réellement observé.
- **Contrôle causal** : remettre à l'identité la porte
  apprise doit dégrader la prédiction sur les tâches
  à mutation, tandis que cela ne doit rien modifier
  sur le cas de référence.
- **Contournement interdit** : ne pas entraîner de
  cibles intermédiaires, ne pas lire les paramètres
  cachés de la tâche dans le learner, ne pas sélectionner
  sur test, ne pas supprimer du rapport les échecs.

## Limites de cette étape

Une seule fonction intermédiaire dans une ossature
de mémoire connue ; pas encore d'attribution conjointe
à de nombreuses portes, pas de graphe arbitraire,
pas de bruit de processus interne, pas de corpus
linguistique, pas de performances SML-25B ni de
mesures GPU ou Dell. Exécution prévue sur runner GitHub
hébergé, Rust 1.89 ; aucun logiciel NVIDIA.

Une seule expérience A6-C7 sera conduite. Les résultats
seront présentés **avant** toute A6-C8.
