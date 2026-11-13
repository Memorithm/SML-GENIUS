# BPB-P8-ISA0 — Compilation assembleur x86-64 native de circuits booléens

**Recherche autonome Memorithm en Rust et assembleur GNU x86-64, 12 novembre 2026.** Aucun composant externe ne fournit la sémantique, l'oracle arithmétique, la synthèse ni la génération de code. Le CPU ne reçoit aucun programme opaque à exécuter et aucun accès machine n'est requis sur Dell/Thor/Mac.

## Périmètre

Le synthétiseur `posit8_bdd_synth.rs` produit un DAG de décisions booléennes (ROBDD) calculant les huit bits de `posit8_add` et `posit8_mul` selon notre oracle. Le nouveau `posit8_x86_64_emit.rs` convertit **chaque nœud de ce graphe en de véritables instructions x86-64**, sans table de correspondance en inférence.

Le protocole de compilation cible **Linux x86-64 System V ABI** :

- Arguments `a` et `b` : octets bas des registres `rdi` et `rsi`.
- Résultat : octet bas de `rax`.
- Registres temporaires uniquement *caller-saved* : `rax`, `rcx`, `rdx`, `r8`.
- Mémoire temporaire : une adresse de pile statique par nœud, au maximum **16 384 octets** par invocation. Aucune adresse calculée par les opérandes, aucun pointeur externe, aucun saut vers du code généré en mémoire.
- Chaque MUX calculé sans branche : `lo XOR ((lo XOR hi) AND bit)`. Les deux entrées terminales sont initialisées à 0 et 1. Les dépendances sont ordonnées et contrôlées (chaque nœud dépend seulement d'identifiants antérieurs).
- Le registre `rsp` est restauré intégralement avant `ret` ; aucune fonction n'est appelée depuis le noyau. La section `.note.GNU-stack` interdit de réclamer une pile exécutable.
- Le chemin reste celui du CPU hôte et de GNU assembler, sans code dépendant d'un fabricant de GPU.

## Qualification exigeante

1. Les sources Rust sont formatées et testées avec Rust **1.89**, y compris les invariants d'indices et les restrictions de pile de l'émetteur.
2. Synthétiser les deux circuits **à partir de toutes les 65 536 paires** selon l'oracle du projet, sans prétendre qu'il existe un jeu TEST indépendant.
3. Générer deux fichiers `.s` **réels**, assembler par `gcc -c -x assembler` et conserver les objets natifs `.o`, désassemblages `objdump -d -M intel`, tailles et symboles.
4. Compiler deux harness Rust liés aux objets assembleur par FFI selon l'ABI System V. Comparer chaque résultat ADD/MUL sur les **65 536 couples** avec l'oracle Rust : zéro erreur tolérée.
5. Rapporter l'empreinte du code généré et les comptages de nœuds. Cela ne prouve **ni** la minimisation des portes, **ni** le caractère optimal de l'assembleur, **ni** un gain de performance par rapport aux ALU natives, **ni** la conformité de l'oracle au standard posit8.
6. L'inférence sur les modèles SML-GENIUS et V888-BOOLEEN n'est pas modifiée. La qualification physique du Dell reste exclusivement sous admission de `Memorithm/RemoteOps`, sans perturbation des services.

## Suite technique

La baisse du nombre de nœuds, du trafic pile, des instructions et des latences doit faire l'objet de recherches prospectives : élimination des portes mortes, propagation des constantes, mutualisation des plans d'activations, allocation des registres, comparateurs sur taille **réelle** du code machine, déroulage et vectorisation SIMD AVX2. Le remplacement par une version plus compacte sera autorisé seulement après exactitude intégrale, benchmark CPU apparié et préservation de l'oracle indépendant.

Référence normative : [Posit Standard 2022](https://posithub.org/docs/posit_standard-2.pdf), qui ne constitue **pas** à ce stade une preuve indépendante de conformité de notre décodeur.
