# P8-ORACLE2 — Validation croisée de l'arithmétique posit8

**Objectif** : empêcher que des circuits booléens entièrement exacts relativement à un oracle incorrect soient présentés comme des circuits arithmétiques corrects.

Deux implémentations Rust sont maintenant comparées, développées séparément dans Memorithm :

- Oracle principal (posit8_circuit_synth.rs) : décodage par décalages de bits puis sélection du mot positif le plus proche par balayage des 127 codes.
- Oracle secondaire (posit8_independent_oracle.rs) : lecture explicite séquentielle des bits du régime, de l'exposant et de la fraction ; calcul de la valeur entière à échelle 2^-24 ; sélection du mot le plus proche par recherche dichotomique encadrante et égalité départagée par la parité.

Le second oracle ne réutilise ni le décodeur ni l'arrondisseur du premier. Les deux partagent **la même spécification mathématique déclarée** du format posit8 (régime, exposant 2 bits, NaR, arrondi, saturation non nulle) et peuvent donc partager une erreur de compréhension de cette spécification.

**Critères de CI** : égalité des 256 décodages individuels, ordre strict des 127 codes positifs, égalité de 65 536 additions et de 65 536 multiplications, propagation de NaR, zéro, unité, extrêmes. Toute divergence bloque la qualification. Compilation Rust 1.89 sans code dangereux, tests et traces publiés pour le SHA exact.

**Conclusion autorisée après CI verte** : deux méthodes indépendantes de décodage et d'arrondi sont cohérentes sur le domaine fini. Cela ne démontre pas à soi seul la conformité au standard Posit 2022 ; un oracle normatif externe sous licence claire, ou une preuve formelle basée directement sur la spécification, restent nécessaires avant une revendication normative. Aucun code externe n'a été copié.

**Intégration** : n'altère pas l'architecture SML-GENIUS ni la production ; pourra servir à V888-BOOLEEN comme procédure de qualification de circuits arithmétiques, indépendamment du chemin de calcul.
