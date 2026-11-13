# SYNTH-LEARN1C — Recherche active de portes arithmétiques élémentaires

**Périmètre strict : sous-fonctions de signe, pas ADD/MUL complets.** Notre première tentative de généralisation de mots posit8 complets a échoué malgré une compilation native réussie. Nous testons ici l'hypothèse selon laquelle un **espace restreint de portes XOR/OR/AND** peut apprendre sans surajustement des règles arithmétiques locales réellement réutilisables.

Les seules variables observables sont les bits de signe des deux posits. L'ensemble de candidats est gelé avant étiquetage : constantes 0/1, signe A, signe B, NOT A, NOT B, AND, OR, XOR. Le professeur est notre oracle posit8 en nombres entiers exacts. Le moteur suit l'ensemble des hypothèses compatibles avec les labels vus, choisit une nouvelle entrée valide maximisant la séparation des prédictions, interroge le professeur **uniquement pour cette entrée**, puis retire les candidats incompatibles. La recherche s'arrête lorsque les candidats survivants sont indiscernables sur les seize entrées de proposition gelées.

Domaines de validité définis avant apprentissage :

- **MUL, signe** : deux opérandes finis, non nuls, non-NaR. Le domaine contient exactement `254 × 254 = 64 516` couples. L'hypothèse de signe est découverte sur au plus 16 exemples.
- **ADD, signe** : deux opérandes finis non-NaR, et soit leurs bits de signe sont identiques, soit au moins un opérande vaut zéro. Ce domaine est un sous-ensemble strict de ADD ; l'apprentissage interroge au plus 16 exemples.

Aucune étiquette extérieure aux entrées sélectionnées n'est utilisée pendant la recherche. Un audit **après gel** sur chacun des domaines complets valide la formule découverte, publie le nom de la porte, le nombre de requêtes, la taille du domaine et les erreurs sur les entrées non interrogées. Zéro erreur est obligatoire pour qualifier un sous-circuit.

Le compilateur de portes émet des instructions GNU assembleur x86-64 indépendantes `movzx`, `shr`, `and`/`or`/`xor`, `shl`, `ret`. La fonction native renvoie **uniquement le masque du signe (0x00 ou 0x80)**, jamais le résultat arithmétique complet. La CI assemble en objets ELF, lie Rust FFI, vérifie les fonctions sur tous les membres du domaine, et archive les sources et désassemblages.

Ce jalon explore la **découverte et l'exactitude d'une primitive structurelle**. Il ne démontre ni un algorithme exact complet d'addition/multiplication posit8, ni une accélération CPU, ni un apprentissage de modèle SML-GENIUS, ni une conformité normative indépendante. Les PR d'exploration basées sur arbres restent des témoins négatifs. La suite consiste à apprendre des portes pour les retenues, exposants, fractions et l'arrondi, puis leur assemblage avec vérification par contre-exemple.
