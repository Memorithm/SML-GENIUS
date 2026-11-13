# SYNTH-LEARN1F — Induction active de prédicats sur les bits bruts posit8

**Objectif contrôlé :** supprimer les trois indicateurs d'entrée `isNaR`, `isZero` et `sign` que les classificateurs précédents recevaient comme caractéristiques construites manuellement. Découvrir désormais leurs fonctions **depuis les huit bits de l'encodage brut**, puis composer des portes apprises pour classifier la multiplication.

Cette expérience ne découvre ni les caractéristiques arithmétiques complètes du posit, ni la magnitude de MUL ou ADD. La sortie reste un drapeau à quatre classes : `1=NaR`, `2=zéro`, `4=négatif`, `0=positif`.

## Contrat d'apprentissage gelé

**Espace des hypothèses par prédicat unaire :** les 6 561 conjonctions de littéraux de huit bits, une par masque et motif `(raw & mask)==value`. Chaque bit a trois états : libre, imposé à zéro, imposé à un. Le système ne reçoit **aucun masque cible** ni règle `x==0` ou `x==0x80`.

Les cibles sont définies par les sorties de notre professeur arithmétique existant `Posit8(raw).mul(Posit8(0x40))` : résultat égal à NaR, égal à zéro, ou bit de signe. Deux requêtes initiales fixes `raw=0` et `raw=0x80` sont payées comme toutes les requêtes. À chaque étape, l'apprenant calcule **uniquement les prédictions des hypothèses restantes**, choisit le prochain octet qui maximise `votes_positifs × votes_négatifs`, puis interroge le professeur pour **ce seul octet** et élimine les hypothèses incompatibles.

Le plafond est de **12 consultations du professeur par prédicat**, puis un audit après gel sur les 256 octets. Pour pouvoir parler de sous-circuit exact, l'erreur sur les octets non interrogés doit être nulle. Les métriques requises sont : requêtes, hypothèses restantes, masque final, motif final et erreurs hors entraînement. L'algorithme doit explicitement détecter une grammaire incompatible ou un budget épuisé plutôt que d'élargir silencieusement le budget.

## Composition booléenne indépendante

Pour chacun des domaines NaR (65 536 paires), zéro fini (65 025 paires) et signe fini non nul (64 516 paires), un deuxième apprentissage sélectionne une porte parmi les neuf candidates `0,1,A,B,NOT A,NOT B,AND,OR,XOR` déjà implémentées dans `posit8_active_sign_gates.rs`. Ses deux entrées proviennent désormais **des détecteurs de bits bruts appris au premier étage**. Jusqu'à 16 étiquettes supplémentaires par porte ; bassin fixe de 16×16 encodages incluant zéro, NaR et positifs/négatifs ; choix des exemples par désaccord des portes candidates ; audit des labels non interrogés sur tout le domaine de chaque opération.

Une fois les six sous-circuits gelés, les règles de priorité **NaR > zéro > signe > positif** sont fixées par le programme. L'audit global compare les 65 536 catégories finales au professeur arithmétique. Toutes les valeurs de l'audit sont en lecture seule et **ne sont jamais réintroduites dans l'apprentissage**.

## Compilation binaire native

Le compilateur Rust produit du GNU GAS x86-64 System V à partir des **masques, motifs et portes effectivement sélectionnés**. Il génère les comparaisons sur octets bruts `and`, `cmp`, `sete`, puis les compositions `or`, `xor` et les retours de classes, sans table de réponses. Après assemblage en `.o`, un programme FFI Rust compare **tous les 65 536 résultats machine** à la fois au graphe appris et à l'oracle.

Un succès prouvera l'exactitude de la classification des **valeurs spéciales et du signe de MUL** à partir des octets bruts, mais n'autorisera aucune revendication sur l'apprentissage de la magnitude, des régimes, de l'exposant, de l'arrondi ou sur l'entraînement du modèle SML-GENIUS. L'espace conjonctif est prédéfini ; une réussite ne constitue pas une découverte libre de topologies quelconques. Aucun code externe, SDK GPU ni travail sur Dell/Thor/Mac ; les essais sont limités au runner GitHub hébergé. Fusion seulement si toutes les CI obligatoires sont vertes sur le SHA exact.

**Implémentation :** `research/boolean-posit/posit8_active_raw_mul.rs`. **ABI :** `posit8_active_raw_mul_native_test.rs`.
