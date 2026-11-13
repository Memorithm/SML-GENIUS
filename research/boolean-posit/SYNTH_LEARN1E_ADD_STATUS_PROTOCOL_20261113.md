# SYNTH-LEARN1E — Classification compositionnelle de ADD posit8

**Résultat ciblé** : classer la somme posit8 parmi quatre classes `NaR=1`, `zéro=2`, `négatif fini=4`, `positif fini=0`. La sortie est un drapeau, pas le résultat posit8 ; le régime, l'exposant, la fraction et l'arrondi restent absents du calcul appris.

## Hypothèse, espace de recherche et accès au professeur

Réutiliser les neuf portes booléennes (constantes, fils A/B, NOT A/B, AND, OR, XOR) implémentées dans `posit8_active_sign_gates.rs`. Les variables caractéristiques sont fournies par le concepteur, et non apprises à partir des bits bruts. Chaque tâche a un domaine explicite :

- **NaR** (65 536 couples) : caractéristiques `a==0x80` et `b==0x80` ; professeur `add(a,b)==0x80`.
- **Zéro** (65 025 couples finis) : `b==(-a)` en arithmétique modulo 256 et `a==0 && b==0` ; professeur `add(a,b)==0`.
- **Négatif** (64 770 couples finis dont la somme n'est pas zéro) : `sign(a) && |a|_code>=|b|_code` et `sign(b) && |b|_code>|a|_code`, où la magnitude brute négative est `(-raw) mod 256` et l'encodage positif est ordonné en valeur croissante ; professeur `add(a,b)&0x80!=0`.

Le bassin déterministe de requêtes est le produit cartésien de 16 valeurs posit8 représentatives dont zéro, NaR, valeurs positives et négatives. À chaque pas, le programme choisit **le point de désaccord maximal entre portes candidates sans lire le professeur**, interroge l'oracle uniquement pour ce point, puis élimine les portes incompatibles. Maximum : 16 requêtes par tâche, 48 au total. Le départage de portes fonctionnellement indiscernables sur le domaine accessible est fixé par l'ordre du tableau de candidats ; **une porte sélectionnée n'est pas nécessairement une formule identifiable de manière unique**.

Les sous-règles apprises sont composées avec priorité NaR > zéro > négatif > positif. Les audits exhaustive des trois sous-domaines et des 65 536 classes finales se déroulent après gel et ne réentraînent rien. La vérification normative externe du posit8 n'est pas réalisée.

## Code machine et contraintes scientifiques

Le compilateur émet le véritable assembleur x86-64 SysV des portes retenues et des caractéristiques fournies (`cmp`, `sete`, `neg`, `jb/jbe`, OR/AND/XOR, ret). Il exploite explicitement l'ordre monotone des magnitudes posit8 sans appeler de bibliothèque arithmétique lors de l'inférence. La CI assemble un objet ELF, le lie à Rust et compare **chaque paire** au classificateur Rust et à l'oracle arithmétique entier.

Un succès prouverait l'**apprentissage actif et la composition de portes exactes pour la classification de signe et valeurs spéciales**, mais non un additionneur posit8 complet, un apprentissage autonome de l'ordre de magnitude, une réduction du coût du modèle SML, ni un avantage mesuré sur CPU. Aucun service ni machine physique n'est sollicité.
