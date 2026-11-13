# SYNTH-LEARN1D — Apprentissage compositionnel du statut de MUL posit8

**Périmètre du résultat** : classer les résultats de multiplication posit8 selon quatre états : NaR, zéro, négatif fini et positif fini. Le résultat du compilateur est un drapeau numérique **1=NaR, 2=zéro, 4=négatif, 0=positif**. Le calcul de la magnitude, du régime, de l'exposant, de la fraction ou de l'arrondi n'est **pas** implémenté. Ne pas confondre ce classificateur avec un multiplicateur posit8.

## Hypothèse préenregistrée

Reprendre l'espace de portes logiques déjà qualifié dans `posit8_active_sign_gates.rs` : constantes, entrées A et B, NOT A/B, AND, OR, XOR. Ne pas copier l'implémentation de ces portes dans ce nouveau module. Entraîner trois sous-fonctions **avec des entrées de caractéristiques explicitement fournies** :

- **NaR** : caractéristiques `(a==0x80, b==0x80)`, domaine complet de 65 536 couples ; le professeur fournit le booléen `mul(a,b)==0x80`.
- **Zéro** : caractéristiques `(a==0, b==0)`, uniquement quand les deux opérandes sont finis (255×255=65 025 couples) ; professeur `mul(a,b)==0`.
- **Signe négatif** : caractéristiques `(a>>7,b>>7)`, uniquement quand les deux opérandes sont finis et non nuls (254×254=64 516 couples) ; professeur `mul(a,b)&0x80!=0`.

Le bassin de requêtes est déterministe, composé de 16 encodages posit8 particuliers, dont zéro, NaR, extrêmes et valeurs positives/négatives, dans leur produit cartésien. L'algorithme n'interroge le professeur **que pour la paire sélectionnée par désaccord maximal** entre les portes candidates ; il supprime ensuite les hypothèses incompatibles. La limite est de 16 requêtes par sous-fonction (48 au total). Le choix de cet espace de caractéristiques est informé par les règles arithmétiques connues : cela ne constitue **pas** une découverte autonome de la détection de zéro ou de NaR au niveau des bits bruts.

Une fois les trois portes figées, le composant les combine avec la priorité NaR > zéro > signe > positif et audite l'intégralité des 65 536 couples. Aucun résultat de cet audit n'est réintroduit dans l'apprentissage. Chaque sous-fonction est également auditée sur son domaine, en comptant les erreurs **hors requêtes de formation**.

## Compilation et vérification natives

La compilation GAS x86-64 System V comprend `cmp`/`test`, `sete`, `movzx`, `or`/`xor`/`and`, décalages et branchements conditionnels. Un fichier `.s` est réellement assemblé en objet ELF, lié à un test Rust FFI, et toutes les 65 536 combinaisons sont comparées à la classification issue de l'oracle entier **et** au classificateur Rust appris. Le drapeau retourné n'est pas le posit8 arithmétique original.

## Conditions de succès

1. Toutes les requêtes correspondent aux étiquettes du professeur, sans duplicata, dans le budget plafonné.
2. Zéro erreur dans chaque sous-domaine non interrogé et **zéro erreur sur les 65 536 classes composées**.
3. Zéro différence entre le code machine et la référence Rust sur tout le domaine.
4. Tests Rust 1.89 et CI générale entièrement verts sur le SHA exact avant fusion. Aucune mise en production du modèle, ni revendication de conformité Posit 2022 indépendante.

Si un seul critère échoue, conserver l'échec et modifier l'hypothèse plutôt que de relâcher les tests. La voie suivante visera l'apprentissage de la magnitude et du régime, puis la normalisation et l'arrondi. Ne pas mobiliser Dell/Thor/Mac sans admission via Memorithm/RemoteOps.
