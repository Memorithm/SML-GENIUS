# SYNTH-LEARN1I — Synthèse active d'un additionneur booléen avec retenue

**But :** découvrir les portes logiques d'un additionneur binaire élémentaire à trois entrées (a, b, retenue d'entrée), puis réutiliser son circuit appris pour calculer une **addition entière de deux octets et d'une retenue**, avec retenue finale sur le neuvième bit. Cette primitive a vocation à servir au calcul de significandes posit8 **après** alignement des exposants, normalisation et traitement des régimes. **Ce n'est pas une opération ADD posit8** : les deux octets bruts ne sont pas des nombres posit8 additionnables par `a+b`.

## Hypothèse et grammaire enregistrées avant validation

L'architecture du sous-circuit local est un réseau de cinq portes :
`p=G0(a,b)`, `g=G1(a,b)`, `sum=G2(p,carry_in)`, `t=G3(p,carry_in)`, `carry_out=G4(g,t)`.

Chaque `G0...G4` peut être XOR, AND, OR, NAND, NOR ou XNOR. **Aucune identité de porte n'est fixée** avant les exemples. La recherche énumère exactement `6^5=7 776` programmes de cinq portes, calcule leurs prédictions pour les **huit combinaisons de trois entrées binaires**, puis déduplique ces programmes par fonction de sortie (deux bits somme/retenue). Cette déduplication est effectuée **sans lire l'oracle**. Le candidat choisi est un représentant reproductible de son comportement ; nous ne prouvons pas qu'il s'agit du circuit minimal parmi toutes les topologies possibles.

L'apprentissage choisit ensuite une combinaison d'entrée qui maximise la **diversité des prédictions** des fonctions encore possibles (`N² − Σ n_classe²`). Il demande la paire `(sum,carry_out)` à l'oracle arithmétique `a+b+carry_in` **uniquement pour l'exemple sélectionné**, élimine les circuits incompatibles, puis répète avec un plafond strict de **huit requêtes au professeur**. Les huit sorties théoriques de l'oracle ne sont pas mises en cache par le système d'apprentissage. Les exemples non demandés sont audités **après sélection définitive** des cinq portes.

Le graphe local appris est réemployé sur huit étages de propagation de retenue. Les calculs `u8+u8+carry` sont contrôlés pour **tous les 131 072 triplets (256×256×2)**. Résultat : `u16` contenant les huit bits de somme et le neuvième bit de retenue. L'audit est postérieur à l'apprentissage ; il n'entraîne pas le circuit.

## Code binaire et critères obligatoires

Le générateur transforme les cinq portes effectivement apprises en **huit réseaux de portes x86-64 SysV totalement dépliés**, avec `and`, `or`, `xor`, décalages, copies de registres ; il ne doit utiliser ni `add` ni `adc`, ni table de correspondance des 65 536 couples d'octets. Le symbole natif est `mem_posit8_learned_ripple_add_u8(a:u8,b:u8,carry:u8)->u16`.

Le pipeline CI assemble GNU GAS en un objet ELF, produit `objdump` et le SHA256 de l'objet, lie un test Rust FFI, puis compare les **131 072** résultats machine à l'oracle entier **et** au programme Rust appris. Le budget de requêtes, les portes retenues, l'erreur sur les entrées locales non interrogées et les différences natives doivent être reportés.

Échec si une seule sortie locale, une somme d'octets ou une instruction native diverge. Le compilateur est un *proof of concept* de synthèse compositionnelle sur un **réseau de retenue imposé**, pas une preuve d'induction libre de toutes les architectures, ni un gain de vitesse par rapport à `add`/ `adc`, ni un additionneur posit complet. Pas de projet tiers copié, pas de SDK GPU, pas de service résident ni de charge sur machines Dell/Thor/Mac. Fusion seulement après qualification spécialisée et CI générale sur le HEAD exact.

Code : `research/boolean-posit/posit8_active_ripple_add.rs`. ABI : `research/boolean-posit/posit8_active_ripple_add_native_test.rs`.
