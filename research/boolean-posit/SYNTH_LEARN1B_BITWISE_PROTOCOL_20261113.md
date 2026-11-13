# SYNTH-LEARN1B — Apprentissage actif bit à bit et sous-circuits booléens partagés

L'expérience SYNTH-LEARN1 a livré une découverte de circuits sur 320 étiquettes par opération et leur compilation native, mais l'audit a révélé une **mauvaise généralisation** : addition 3 821 erreurs sur 4 096 exemples réservés ; multiplication 4 014 erreurs sur 4 096. Un arbre discriminant les 256 valeurs de sortie a tendance à mémoriser les entrées observées.

## Hypothèse gelée avant exécution

Décomposer ADD et MUL en **huit fonctions booléennes de sortie**, chacune entraînée comme un signal binaire (0 ou 1), peut réduire la complexité de l'hypothèse. Les huit arbres interrogent les mêmes 16 variables `a_i,b_i`. Les sous-circuits équivalents sont canoniquement mutualisés grâce aux tables de hachage de nœuds `(variable,low,high)`, et les constantes 0/1 sont partagées. Les sorties sont recombinées par opérations OU et décalages.

Le budget d'apprentissage reste **64 requêtes initiales et 8 × 32 requêtes actives = 320** par opération. Chaque round compare deux hypothèses bit à bit, avec deux préférences opposées pour les séparations équivalentes ; 75 % des prochains exemples sont choisis parmi les désaccords et le reste relève d'une exploration déterministe. Les membres du jeu de validation réservée (fixée par `is_holdout`) ne sont jamais étiquetés à l'entraînement.

Comparaison : les métriques du modèle multiclasse initial et du modèle bitwise constituent **deux expériences de même budget**, mais pas nécessairement un appariement à échantillons identiques ; n'attribuer les écarts qu'à l'architecture serait incorrect. Les audits après entraînement ne modifient pas les circuits.

**Critères** : erreurs sur les 320 entrées apprises, erreurs sur exactement 4 096 entrées réservées (le nombre doit être confirmé par le workflow), erreurs sur 65 536 entrées complètes, nœuds, nombre de branches et profondeur. Ne déclarer une opération exacte que pour zéro erreur exhaustive.

## Abaissement natif

Le compilateur indépendant produit une vraie fonction x86-64 SysV par opération : `xor eax,eax`, tests `dil/sil`, branchements `jnz/jmp`, assemblage de huit sorties logiques par `or eax,imm`, `ret`. Il n'intègre aucune table de réponses arithmétiques ni l'oracle dans l'exécutable obtenu. La CI assemble chaque `.s` en objet ELF, le lie à Rust et compare ses 65 536 prédictions au graphe appris. **Identité native du candidat ≠ exactitude arithmétique normative.**

## Décision

- Si la généralisation s'améliore, continuer vers les monômes AND, XOR et NOT et la recherche de sous-circuits de retenue, avec partage inter-opérations.
- Si elle ne s'améliore pas, conserver le résultat négatif et modifier l'espace de recherche au lieu de présenter le prototype comme industriel.
- Mesures CPU hébergées uniquement ; pas de charge Dell/Thor/Mac, pas de dépendance externe, et pas d'activation des poids d'un modèle SML-GENIUS.
