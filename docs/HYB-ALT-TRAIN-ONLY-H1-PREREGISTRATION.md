# H-ALT-1 — voie concurrente entraînable, sans routage ni écriture SBG

**Préenregistrement avant validation du noyau H-ALT-1 (23 octobre 2026).**
Rattaché à H-ALT-0, PR #676, et au protocole `HYB-ALT-TERNARY-READONLY-RESIDUAL-PREREGISTRATION.md`. Ne pas fusionner H-ALT-1 avant que H-ALT-0 soit validée et fusionnée sur SHA exact.

## Démonstration demandée

La voie Claude-B étudie les experts neuronaux par page MOR. Cette voie contrôle, de manière indépendante, la seule hypothèse : **un readout neuronal appris sur un état SBG congelé apporte une information prédictive supplémentaire**. Aucune mutation de MOR, `Pvp6Bank`, `rtlr_cost_gate`, poids production ou SBG.

## Algorithme gelé

- Tête 256 logits byte, poids maîtres `f64` **exclusivement à l'entraînement**, descente de gradient softmax de l'entropie croisée par token, features binaires de coordonnées SBG déclarées. Batch unitaire, ordre TRAIN déterministe. Apprentissage sur *résidu* des logits du modèle booléen gelé, divisés par l'échelle quantifiée fixe `q=8`.
- Valeurs maître `w` exportées : `+1` si `w>=0.125`, `-1` si `w<=-0.125`, sinon 0 ; biais `round(bias)` en entier, toutes les sorties multipliées par `8` dans le chemin d'inférence. Les vecteurs exportés transitent exclusivement par l'interface H-ALT-0 contrôlant masques, padding et overflow.
- Résultat H-ALT-1 : `FrozenReadout` immuable, contenant l'identité des documents TRAIN vus, sans mécanisme de réentraînement sur DEV ou TEST.
- Traçabilité : chaque exemple expose `document_id`, `position`, `split`, `state`, `baseline_logits[256]`, `target`. Refuser strictement TRAIN en évaluation DEV, DEV/TEST dans `train_step`, collision document TRAIN/DEV et doublon `document_id+position` à l'évaluation.
- BPC en `f64` de chaque bras sur les **mêmes positions** à partir de scores de sortie int8/ternaires déterministes ; ne pas réclamer une identité binaire Dell/Thor du logarithme flottant.

## Gates et limites

Les tests de ce module incluent apprentissage et amélioration de perte sur un **jeu artificiel explicitement marqué « sanity check », avec deux patterns réutilisés entre TRAIN et DEV**. Cela prouve seulement que le gradient, la quantification et le calcul DEV sont câblés ; ce n'est ni une preuve de généralisation, ni un BPC 3,66, ni une campagne de langage naturel.

Une **campagne réelle** ne pourra être effectuée que lorsque les traces SBG, les logits du même modèle booléen, labels, tokenizer, positions alignées, manifeste TRAIN/DEV `eng1c_dev` et identité des poids seront fournis/vérifiés. Il est interdit d'entraîner sur DEV, d'ouvrir TEST pour choisir les hyperparamètres ou d'imputer à ce module un gain de BPC avant mesures. Séparer coût de construction/training, inférence quantifiée et mémoire par hôte.

À comparer côte à côte avec Claude-B : même train/dev/test, budget, seeds et coût du moteur complet, sous contraintes SBG identiques. L'absence de gain d'H-ALT n'invalide pas la proposition B ; elle invalide cette tête résiduelle sur les features évaluées.

**Note d'implémentation** : le benchmark de cohérence ne remplace pas la vérification des traces de corpus octet par octet. Les labels de split seuls ne suffisent pas à prouver l'indépendance sémantique entre TRAIN et DEV.
