# HYB H-2c — un expert de recopie après la tête H-1d : étude de faisabilité

**Rédigé le 25 octobre 2026, avant toute exécution sur données réelles.** Cette étude ne rend aucun verdict sur un échantillon réservé. Elle dit seulement si un expert de recopie vaut la peine d'être préenregistré pour EVAL4.

Elle ne lit que la calibration de H-1d (CAL-FIT et CAL-SEL), déjà sur le Dell. Elle n'ouvre ni EVAL4, ni le DEV S5h, ni TEST.

## Pourquoi

L'étude des templates (H-2t, `RESULTS-HYB-H2T-TEMPLATE-FEASIBILITY.md`) a mesuré au passage un signal que P1 ne voit pas : un article se répète à l'intérieur de lui-même (noms propres, notations, formules, références). Un modèle de recopie document par document gagnerait jusqu'à 0,31 BPC sur CAL-SEL. Ce chiffre supposait un choix parfait, octet par octet, entre P1 et la recopie. Il surestime donc le gain.

Le 25 octobre, le propriétaire a choisi cette piste. H-2c mesure le gain réel, une fois la recopie mélangée à la tête A1d dans le codage bit à bit.

## Méthode (`sml-hyb-h2-s5h copy`, module `hyb_h2c_copy`)

1. **Expert de recopie.** C'est un modèle LZP par document : il repart de zéro à chaque article, avec une table de 2^18 entrées. Il cherche la dernière occurrence des `m` derniers octets et prédit l'octet qui l'a suivie. Une prédiction juste prolonge la correspondance, une fausse l'arrête. Il ne lit que les octets déjà décodés.
2. **Mélange après la tête.** Pour chaque décision binaire de l'octet, tant que les bits déjà codés concordent avec l'octet prédit, la recopie donne un bit attendu. Le mélangeur déplace le stretch de la tête A1d vers ce bit (ou s'en éloigne) d'un décalage appris. Ce décalage dépend de trois choses :
   - la longueur de la correspondance, en 6 classes : 0, 1 à 3, 4 à 15, 16 à 63, 64 à 255, 256 et plus ;
   - la profondeur de la décision (8) ;
   - si la tête penche déjà vers le bit attendu ou non.

   Cela fait 96 décalages. Quand la prédiction ne concorde plus, ou qu'il n'y en a pas, la tête A1d code seule.
3. **Apprentissage sur CAL-FIT.** Chaque décalage est le meilleur, en longueur de code exacte, parmi les valeurs de −1024 à 2047 par pas de 8. Une case vue moins de 1 000 fois garde un décalage nul. Les têtes A1d publiées (graines 11 et 23) sont reprises telles quelles, avec leur λ.
4. **Choix sur CAL-SEL.** On essaie trois longueurs de contexte, `m` = 6, 8 et 12. On retient celle qui donne le meilleur BPC moyen des deux graines sur CAL-SEL.
5. **Contrôle nul.** Il garde le rythme de l'expert `m` = 12 (mêmes octets couverts, mêmes longueurs) mais prédit un octet tiré au hasard. Le mélangeur, appris de la même façon, ne doit rien gagner. *(Remplacé après le premier run : voir la révision plus bas.)*

Sont publiés, pour chaque variante :
- la couverture et le taux de réussite ;
- les décisions et leur taux de réussite par classe de longueur ;
- le BPC et le gain sur A1d, par graine ;
- les 96 décalages appris.

Le BPC de A1d sur CAL-SEL doit retrouver celui de H-2 (2,5540 et 2,5584). C'est un contrôle de non-régression.

## Critère de faisabilité (figé ici, remplacé par la révision plus bas)

`COPY_FEASIBLE` exige les deux conditions suivantes. Sinon, le verdict est `COPY_NOT_FEASIBLE`.
- À la longueur `m` retenue, la recopie gagne au moins **0,01 BPC sur A1d** sur CAL-SEL pour les deux graines. C'est la porte de H-2 et de H-2t.
- Le contrôle nul reste à moins de **0,002 BPC** de A1d pour les deux graines.

## Révision des contrôles (25 octobre, après le premier run)

Premier run : RemoteOps #858, SML `c7bba16e`, `h2c.tsv` sha256 `1c27a4830d66cdc758667523fe801047abbf229e44461ecb281e140e376800f8`. Résultats : `RESULTS-HYB-H2C-COPY-FEASIBILITY.md`.

La recopie a gagné 0,54 et 0,56 BPC sur A1d (contexte 6). Le verdict a pourtant été `COPY_NOT_FEASIBLE` : le contrôle nul a gagné 0,0029 et 0,0050 BPC, au-dessus de 0,002.

Ce contrôle était mal conçu. Il ne prédisait qu'aux positions où l'expert `m` = 12 trouvait une correspondance, et avec ses longueurs. Il savait donc où le texte se répète. Ses décalages appris sont opposés deux à deux selon l'accord avec la tête : ils ne font que recalibrer la confiance de la tête à ces positions.

Sur décision du propriétaire, l'étude est refaite sur la calibration avec le même expert et le même mélangeur. Le propriétaire a demandé tous les contrôles proposés. Ils sont figés ici, avant le second run.

**Critères** (tous requis, pour les deux graines) :
1. **Gain** : au moins 0,01 BPC sur A1d, à la longueur de contexte retenue sur CAL-SEL (6, 8 ou 12).
2. **Nul aléatoire** (`null`) : un octet et une longueur au hasard à chaque position. Il doit rester à moins de 0,002 BPC de A1d.
3. **Donneur** (`donor`) : un expert de contexte 6 qui cherche ses correspondances dans un autre article de la même partie (le suivant dans la liste), jamais dans l'article codé. Son gain doit rester sous 10 % du gain retenu.
4. **Retard** (`lag`) : la prédiction de contexte 6 faite 4 096 octets plus tôt dans le même article. Son gain doit rester sous 10 % du gain retenu.
5. **Bootstrap par article** : 1 000 tirages d'articles de CAL-SEL avec remise. La borne à 2,5 % du gain doit atteindre 0,01 BPC. Sont aussi publiés la part des articles qui gagnent, la médiane et le premier décile du gain par article.
6. **Échange** : on apprend sur CAL-SEL et on mesure sur CAL-FIT. Le gain doit garder au moins la moitié du gain retenu, et le nul doit rester sous 0,002 BPC.
7. **Causalité** : pour un article de CAL-SEL sur 16, la seconde moitié est remplacée par du bruit. Toutes les prédictions jusqu'au milieu inclus, celles de chaque variante et de COPY-1, doivent rester identiques. Une seule différence donne le verdict `COPY_CAUSALITY_FAIL`.

**Publiés sans seuil** :
- **Rythme** (`timing`, l'ancien nul) : ce que vaut le seul fait de savoir où le texte se répète.
- **Décalage unique** : un seul décalage pour les 96 cases, appris sur CAL-FIT.
- **Recopie sur P1 seul**, sans la tête H-1d, pour mesurer la complémentarité.
- **COPY-1** (GPT/Grok, SML #725) : fenêtre de 256 octets, suffixe de 4 octets au plus, mélange fixe de 20 % en probabilité. Il est évalué sur les mêmes octets et avec les mêmes têtes. La probabilité H-1d est prise au 16 bits de `squash_value`, ce qui en fait une évaluation hors moteur.

Limite connue : le mélangeur distingue les cas où la tête penche vers le bit attendu ou non. Un nul aléatoire peut donc en tirer un léger recalibrage de la tête par profondeur, si elle est mal calibrée. Le critère 2 le mesurera tel quel.

## Ce qui suit

- **Si l'étude est faisable**, H-2c sera préenregistré pour EVAL4 (peS2o-0004) :
  - `m` et les décalages figés ;
  - contrôles A1d et nul ;
  - porte sur CAL-SEL ;
  - EVAL4 lu une seule fois, après accord du propriétaire.

  L'intégration au moteur natif (coût, aller-retour, Dell = Thor) viendra ensuite, comme H-3 l'a fait pour la tête H-1d.
- **Sinon**, le résultat est publié tel quel, et EVAL4 reste fermé.

## Limites déclarées

- CAL-SEL sert à la fois à choisir `m` et à mesurer le gain. Ce choix ne porte que sur trois valeurs, mais le gain reste un peu optimiste. Le verdict ne vaut que comme feu vert pour un préenregistrement.
- Les deux premiers octets d'un article ne sont pas tracés par P1, donc l'expert ne les voit pas. Le moteur natif les verra.
- Il n'y a que des articles peS2o.

Research only. DEV non lu. EVAL4 non lu. TEST fermé. NOT_PROMOTED.
