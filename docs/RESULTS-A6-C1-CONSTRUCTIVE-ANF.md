# A6-C1 — Apprentissage constructif de portes ANF par cible résiduelle

Statut : **expérience terminée**, PR #480 conservée en brouillon. Les chiffres
ci-dessous proviennent d'une exécution GitHub Actions, et **non du Dell**.

## Hypothèse et mécanisme

Un nouveau bit d'état récurrent est appris pour représenter le résidu booléen
`r_t = y_t XOR x0_t`, puis intégré dans une lecture native ANF à deux entrées :
`y_hat_t = x0_t XOR g(s_t, x_t)`. Les sorties des portes utilisent l'état
**après la mise à jour**; l'optimisation de la porte utilise seulement l'état
**avant la mise à jour** et l'entrée actuelle. Aucun bit cible futur n'est
connecté comme entrée de la porte.

La table de vérité optimale pour chaque sous-ensemble de sources est calculée
sur le seul entraînement (majorité par adresse), puis convertie en coefficients
ANF par transformée de Möbius. Une recherche conjointe exhaustive sur au plus
trois entrées autorise la découverte d'une synergie XOR3 invisible à une
sélection gloutonne d'une seule variable. Le choix est engagé dans le réseau
uniquement s'il réduit strictement l'erreur **de validation**. Tous les choix
sont figés **avant** toute génération ou évaluation du jeu de test.

Le prototype borne la recherche à 16 sources candidates et une arité
maximale de 4. L'expérience utilise 12 sources et l'arité 3 : 298
sous-ensembles explorés par porte (= 12 + 66 + 220).

## Protocole préétabli

- Cinq graines indépendantes par panneau.
- Deux portes apprises successivement dans un seul `LocalAnfNetwork` :
  1. résidu XOR3 sur `x3_t, x1_(t-1), x2_(t-2)`,
  2. résidu MUX3 (sélection par `x3_t` entre `x1_(t-1)` et `x2_(t-2)`).
- Quatre entrées binaires indépendantes, deux retards temporels exacts et figés
  (cette mémoire n'est **pas** apprise lors de l'expérience).
- Échantillons de 64 séquences par groupe : 6 144 observations
  d'entraînement, 3 072 de validation et 3 072 de test par graine et cible.
- Deux panneaux : sans bruit et bruit de label Bernoulli(1/16) indépendant.
- Contrôles : sortie baseline seule, meilleure porte 1 entrée entraînée,
  meilleure porte 2 entrées entraînée, construction conjointe <= 3 entrées.
- Les données de test utilisent d'autres graines et ne participent ni au
  choix des sources, ni au calcul ANF, ni à l'acceptation des portes.

Révision effectivement mesurée :
`72432f44700c3ba644ce4cc2ec594840ef0d77d1`.

[Journal brut de l'expérience](https://github.com/Memorithm/SML-GENIUS/actions/runs/38146259721/job/114490162719).

Exécution : GitHub-hosted Ubuntu x86_64, 4 processeurs logiques, Rust 1.89.0.
Les tests spécialisés `anf_constructive` (3), ceux du benchmark (3),
la compilation, Clippy et le formatage réussissent.

## Résultats expérimentaux

**Taux d'erreur binaire sur les cibles synthétiques indépendantes**, moyens
sur cinq graines (pourcentage, plus faible = meilleur) :

| Panneau / résidu | Baseline | ANF ≤1 entrée | ANF ≤2 entrées | ANF constructif ≤3 entrées |
|---|---:|---:|---:|---:|
| Sans bruit — XOR3 | 49,8242 % | 50,0130 % | 49,3620 % | **0,0000 %** |
| Sans bruit — MUX3 | 48,3789 % | 23,8542 % | 23,9453 % | **0,0000 %** |
| Bruit 1/16 — XOR3 | 49,5898 % | 50,4232 % | 49,4857 % | **6,4128 %** |
| Bruit 1/16 — MUX3 | 48,6589 % | 27,7083 % | 27,7604 % | **6,1458 %** |

- **20 portes sur 20 acceptées** sur validation, avant test.
- **20 sur 20** retrouvent exactement les positions utiles
  `[3, 8, 13]` (deux retards et une entrée actuelle).
- Chacune produit une représentation ANF avec **3 monômes non nuls**.
- Sans bruit, les erreurs d'entraînement, de validation et de test sont nulles
  pour les 10 portes construites.
- Avec bruit, les taux d'erreur de 6,41 % / 6,15 % sont voisins du
  plancher de bruit de 6,25 %; aucune extrapolation statistique large n'est
  revendiquée avec seulement cinq graines.

## Lecture scientifique et limitations

**Démontré :** l'algorithme peut trouver des sources et synthétiser de nouvelles
portes ANF utiles dans un réseau récurrent réel, à partir de données
supervisées d'entraînement, puis les engager sur validation. La recherche
jointe sur trois sources résout un problème XOR3 à crédit marginal nul
contre lequel les sous-ensembles 1 et 2 entrées échouent. La porte MUX3
confirme que l'apprentissage ne se limite pas à une parité.

**Non démontré :** gain sur un modèle de langue, généralisation à un corpus
naturel, apprentissage des retards eux-mêmes, composition de portes dont les
cibles auxiliaires ne sont pas connues, échelles K élevées, coût mémoire au
niveau du modèle 25B, gain de vitesse GPU/SIMD ou attribution du crédit
à travers de longues chaînes causales.

Le laboratoire a été construit intentionnellement avec des cibles algébriques
identifiables. Il valide **la mécanique d'apprentissage constructif**, pas la
performance prédictive globale du modèle. Aucune comparaison avec le résultat
A5-5 en bits/octet n'est faite : ce ne sont pas les mêmes données ni le même
objectif.

**Verdict : PASS sur un banc contrôlé**, poursuite conditionnée à une expérience
ultérieure de composition/credit assignment moins supervisée, qui doit être
proposée seulement après restitution et examen du présent résultat. L'exécution
sur Dell reste à faire lorsqu'un canal RemoteOps opérationnel sera disponible.
