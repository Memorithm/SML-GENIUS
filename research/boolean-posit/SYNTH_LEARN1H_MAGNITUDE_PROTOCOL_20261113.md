# SYNTH-LEARN1H — Comparateur de magnitude posit8 composé de portes apprises

**Objectif :** remplacer les comparaisons de magnitude codées à la main par un sous-circuit booléen récurrent dont les **portes locales** sont sélectionnées activement. Le calcul utilise la négation complète de SYNTH-LEARN1G comme normaliseur des encodages finis négatifs. La sortie est une relation entre magnitudes, pas un ADD/MUL numérique complet.

## Préenregistrement de l'espace de recherche

L'architecture générale du comparateur est un **automate booléen de huit étages**, parcourant les bits de poids fort vers les poids faibles. Il transporte deux signaux de préfixe : `greater` et `equal`. Ce schéma de comparaison est **imposé** par notre hypothèse algorithmique. En revanche, les cinq portes locales sont apprises à partir de tables d'étiquettes élémentaires, sans leur identité dans le code du moteur :

1. `bit_greater(a_i,b_i)` apprend si un bit est strictement supérieur à l'autre.
2. `bit_equal(a_i,b_i)` apprend l'égalité des bits.
3. `qualified(equal_prefix,bit_greater)` apprend l'activation du plus grand nouveau préfixe.
4. `merge(greater_prefix,qualified)` apprend la propagation de la supériorité.
5. `merge_equal(equal_prefix,bit_equal)` apprend la poursuite de l'égalité.

Grammaire : neuf portes déjà connues (0, 1, fils, NOT, AND, OR, XOR), **AND-NOT-B** et **XNOR**, soit onze candidates. Chaque fonction étiquette **au plus quatre** couples `(a,b)` binaires, choisis en maximisant le désaccord des hypothèses encore compatibles. Audit des autres couples binaires **après le gel des portes**. Ces professeurs locaux sont définis à partir de l'algèbre élémentaire des comparaisons binaires, et non découverts à partir des 65 536 valeurs posit complètes : aucune revendication de découverte autonome d'un comparateur à partir de zéro n'est autorisée.

## Composition et qualification

Le graphe découvert est exécuté sur les 8 bits de **tous les 65 536 couples d'octets**, avec trois résultats `0=a<b`, `1=a>b`, `2=a=b`. La comparaison de magnitudes posits normalise chaque opérande fini via le **circuit de négation appris** de SYNTH-LEARN1G ; une entrée NaR produit le résultat `3=non défini`. L'audit de 65 536 couples compare la relation entière à la **valeur absolue de l'oracle exact** `Posit8.decode_units()`. Cela contrôle notamment les 65 025 couples finis et les 511 couples contenant NaR. Un zéro erreur doit être obtenu sur les deux audits.

Le générateur traduit les cinq portes effectivement apprises en **huit étapes x86-64 SysV dépliées** et produit deux symboles :
- `mem_posit8_learned_byte_cmp` : ordre non signé de deux octets bruts ;
- `mem_posit8_learned_posit_mag_cmp` : comparaison de magnitude, avec **appels réels à la fonction native `mem_posit8_active_abs8`** précédemment compilée.

La CI compile deux objets GAS, les lie à un test Rust FFI et compare les **65 536 sorties des deux fonctions natives** au comparateur Rust et à l'oracle entier. Inventaire du code machine et des objets archivé. Les noms des portes, leur nombre d'exemples, les domaines et les erreurs sont obligatoirement publiés.

Cette expérience ne prouve ni une accélération (les comparaisons natives `cmp` restent les meilleurs témoins), ni une addition posit8 complète, ni un apprentissage libre de toutes les topologies, ni un progrès de qualité de SML-GENIUS. Aucun pilote/SDK GPU, ni admission sur les machines Dell/Thor/Mac. Parent fonctionnel : SYNTH-LEARN1G ; après fusion du parent, la branche doit être **requalifiée sur main** avant fusion.
