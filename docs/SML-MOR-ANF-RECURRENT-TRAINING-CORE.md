# SML — MOR apprenant, ANF booléenne et récurrence différentiable

**Date de pré-enregistrement : 17 octobre 2026.** Étape de recherche **avant** intégration dans l'atlas 50M/25B. Ce module est un noyau de validation mathématique de **21 coefficients F64**, pas un modèle de langage complet.

## Hypothèse testée

Le noyau `sml-core::learned_mor_anf_recurrent::RecurrentMorAnf` possède deux experts logistiques, un routeur MOR souple et un état scalaire récurrent. Les entrées restent des bits ; elles sont projetées en six features `[1, a, b, a XOR b, a AND b, h_t]`. Les deux expressions XOR et AND sont des portes GF(2)/booléennes **fixes** ; l'implémentation ne prétend pas découvrir une banque de polynômes ANF de grande dimension.

Au pas `t`, la récurrence est `h_t = tanh(r0*h_(t-1) + r1*(a XOR b) + r2*(a AND b))`; le gate `g_t=sigmoid(dot(router,x))` mélange les deux probabilités expertes `p_t=g_t*p0+(1-g_t)*p1`. Optimisation de l'entropie croisée sur une séquence bornée de 1 à 4 096 positions avec **BPTT intégral** (`dLoss/dg`, `dLoss/dExpert`, `dLoss/dh` rétropopagé à `t-1`), normalisation par nombre de positions, clipping global de norme 1, et tests d'absence de NaN.

La source d'initialisation brise intentionnellement la symétrie des experts : un routeur dont les experts seraient identiques ne recevrait aucun gradient.

## Conditions expérimentales

- Test de récupération sur une séquence XOR : la perte observée doit baisser significativement.
- Le vecteur de paramètres du **routeur** et les poids **récurrents** doivent effectivement changer, et non simplement la mémoire des experts.
- Vérification indépendante des dérivées par différences finies avec perturbation symétrique du paramètre récurrent ; étendre au routeur et aux experts.
- Séquences vides/incohérentes ou taux d'apprentissage invalides rejetés sans écritures.
- Construire un banc financier en lecture seule à partir de textes 10-K déjà vérifiés : groupes de bits formés des deux octets précédents, cible de l'octet courant, validation sur d'autres documents **sans entraînement**. Rapporter pertes TRAIN et holdout et nombre de tokens, **pas** de généralisation crypto ou d'amélioration 25B sans mesures.
- **Ablations obligatoires avant promotion** : désactiver/figer le routage appris, désactiver la récurrence, retirer XOR/AND, comparer pertes, taux de mise à jour et coût. Comparer à la banque de comptage 8 192 et au noyau sparse gradient 50M sur les **mêmes données**.
- Migration : conserver la dérivée du routage souple mais remplacer les 21 poids d'expert par un sélecteur de **pages** de l'atlas 50M avec contrôle de coûts ; la récurrence doit être segmentée correctement aux frontières de documents et sauvegardée dans des checkpoints vérifiés.

## Ce qui n'est pas encore établi

Ce noyau n'a que **21 paramètres** (`2×6+6+3`). Il ne vérifie pas la répartition cible 25B (booléen 21,25B, ternaire 2,5B, résidu Q4 1,25B), la capacité de mémoire à 8 Gio, la performance token/s ni une compétence de dialogue. Il ne démontre pas non plus l'entraînement en précision booléenne : **les poids d'apprentissage sont en F64** ici. Tous ces objectifs nécessitent des expériences et des ablations supplémentaires.

Étape de décision : **ne jamais promouvoir SML-2 puis SML-25B par nombre de cases allouées**. Exiger amélioration de généralisation à budget équivalent, proportion de paramètres effectivement mis à jour, temps et bande passante mesurés sur Dell/Thor, stabilité des checkpoints, comparaison de perte après quantification et absence de contamination TEST. G0 des rapports financiers reste `NOT_ADMITTED` tant que les droits sous-jacents ne sont pas qualifiés.
