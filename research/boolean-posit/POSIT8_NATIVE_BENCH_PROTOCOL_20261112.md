# P8-ISA3 — Comparaison appariée des noyaux assembleur natifs

Recherche CPU-only Memorithm. Cette campagne mesure les fonctions x86-64 System V générées par nos trois compilateurs Rust : direct, cached selectors, arêtes complémentaires pour ADD et MUL posit8.

**Protocole préenregistré** : Rust 1.89 sur GitHub Actions ubuntu-24.04, 8192 paires déterministes distinctes, 2 passages d'échauffement, 7 passages chronométrés par variante et opération ; ordre de passage tournant entre variantes, même séquence de paires à toutes les variantes. Temps de Instant incluant appel ABI, lecture d'entrée et accumulation d'un checksum empêchant l'élimination des appels par l'optimiseur. Rapport : médiane des ns par appel, rapport à la médiane de la variante directe et checksum.

**Garde-fou exactitude** : la CI compile et lie les six fichiers objets assembleur à un même programme Rust ; chacune des six fonctions a déjà un contrôle exhaustif de ses 65 536 couples par le workflow d'assemblage. Le benchmark vérifie à nouveau un checksum identique sur son jeu fixe. Zéro résultat de temps n'est admissible en cas de divergence, échec de compilation ou CI rouge.

**Limites** : le processeur GitHub hébergé n'est pas fixe d'une exécution à l'autre ; la fréquence, l'affinité, les voisins, les caches et la politique d'ordonnancement ne sont pas maîtrisés. Ces latences exploratoires ne sont pas des preuves de supériorité industrielle. Les temps ne constituent ni une métrique de modèle SML complet ni une comparaison à une ALU posit native optimisée. La taille d'objet ELF peut comprendre des métadonnées non exécutées et ne mesure pas directement le trafic mémoire pendant l'inférence.

**Utilisation future** : campagnes contrôlées et appariées sur le Dell T430 uniquement sous admission via Memorithm/RemoteOps, avec préservation des workloads existants ; sélection de la variante sur temps réel, non seulement sur taille d'objet. Aucun code externe n'est importé.

Artefacts : code Rust du benchmark, SHA exact, fichiers assembleur et objets, inventaire CPU, résultats de tests, chronométrages et empreintes SHA-256 des objets.
