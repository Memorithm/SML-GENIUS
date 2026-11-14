# SYNTH-LEARN1J — Multiplication entière 8×8 composée de portes apprises

**Objet :** transformer la primitive arithmétique SYNTH-LEARN1I (addition binaire de deux octets avec retenue, cinq portes sélectionnées activement) en un **multiplicateur entier complet de 8×8 bits vers 16 bits**. L'algorithme de produits partiels et décalages est **prédéfini**, tandis que la logique de chaque addition et retenue provient des cinq portes découvertes et gelées dans le parent #990. Il ne s'agit **pas** d'une multiplication numérique posit8 : les règles de régime/exposant/fraction et de normalisation ne sont pas encore traitées.

## Protocole préenregistré

Pour chacun des huit bits du multiplicateur B, sélectionner `partial = (A << bit)` si ce bit vaut 1, sinon zéro, puis accumuler dans un mot de 16 bits. Chaque addition de 16 bits utilise le même additionneur appris **deux fois** : l'octet bas produit la retenue d'entrée de l'octet haut. La composition totale comporte **huit produits partiels et seize appels au même circuit ADD8 appris**. Aucun apprentissage supplémentaire ne consulte l'oracle de multiplication ; le budget du programme est celui des portes apprises par SYNTH-LEARN1I, indiqué explicitement dans le rapport pour éviter le double comptage.

L'audit exhaustif, après synthèse de la primitive, compare les **65 536 paires d'octets** à l'oracle arithmétique indépendant `u16::from(a) * u16::from(b)`. Les tests unitaires vérifient aussi la retenue entre octets et des cas limites (255×255, 128×128, 0×255).

## Compilation réelle

Le compilateur génère un objet GNU assembleur x86-64 SysV autonome pour `mem_posit8_learned_u8_mul_u16(a:u8,b:u8)->u16`, relié à l'objet déjà généré par le compilateur de #990 `mem_posit8_learned_ripple_add_u8`. Le code implémente des produits partiels par masques, puis **16 appels à l'additionneur booléen compilé**, avec une zone locale de pile et un alignement SysV explicite. Il ne fait appel à aucune table de 65 536 produits ni aux instructions de multiplication machine `mul/imul`. Les opérations arithmétiques sur les données doivent provenir des portes de l'additionneur appris ; les décalages et la sélection de produits partiels restent des opérations structurelles prescrites.

La CI génère les deux fichiers GAS, assemble les deux objets ELF, les relie à Rust FFI, compare les **65 536 produits natifs** à la référence entière et au moteur Rust composé, et publie les désassemblages, tailles, sommes de contrôle, nombre de consultations de l'oracle et erreurs. Une erreur quelconque interdit la qualification. La réussite du programme ne prouve ni une meilleure latence face à l'instruction matérielle `mul`, ni un modèle posit8 complet, ni un apprentissage de topologie sans biais architectural.

Dépendance directe : PR #990. La présente PR doit être intégrée **après** son parent, puis requalifiée sur le SHA exact de `main` avec les contrôles obligatoires. Aucune admission Dell/Thor/Mac ; aucun SDK GPU ni changement du runtime de production.
