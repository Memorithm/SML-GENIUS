# P8-PACK — Évaluation booléenne Rust générique de 64 à 1024 couples

**Recherche expérimentale autonome Memorithm.** Le nouveau composant `posit8_bitplane_generic.rs` réutilise un même noyau sûr Rust pour les largeurs `W=1..16`, soit 64 à 1024 couples simultanés. `W` est un paramètre constant à la compilation : chaque variable et chaque résultat intermédiaire est représenté par `[u64; W]`.

## Travail effectivement implémenté

- Les 16 variables binaires d'entrée sont transposées en `16 × W` mots de 64 bits, les nœuds à arêtes complémentaires sont calculés en ordre topologique avec XOR/AND/NOT, puis huit masques de sortie reconstruisent chaque octet posit8.
- Le buffer de travail `PackedScratch<W>` est alloué une fois et réutilisé. Son coût mémoire est exactement `16×W×8 + (N+1)×W×8` octets pour N nœuds ; aucune table de réponses d'opérations n'est utilisée dans l'inférence.
- La fonction de streaming traite toute longueur, même zéro, et n'alloue pas de tableau de remplissage pour la dernière tranche : elle calcule directement les mots partiellement utilisés.
- Largeurs nulles, largeur supérieure à 16, tailles de sorties erronées ou buffer inadéquat sont rejetés avant l'écriture des sorties.
- Exactitude obligatoire sur les 65 536 couples ADD et MUL, pour `W=4` et `W=8`; tests complémentaires pour `W=1`, longueurs non alignées, encodages spéciaux et refus des arguments invalides.

## Protocole de performance préenregistré

Le comparateur est le **noyau spécifique 256 voies déjà qualifié** et conservé comme contrôle, face au nouveau code générique compilé pour `W=4` (256 voies) et `W=8` (512 voies). Les trois exécutent le même graphe sur les mêmes 65 536 couples dans un ordre déterministe ; la préparation des plans et la reconstruction sont comptées. Deux échauffements, sept passages mesurés, ordre tournant, résultats : trois médianes en nanosecondes par couple, deux rapports par rapport au contrôle, checksum identique des 65 536 sorties. Toute discordance rend les mesures invalides.

Ce travail ne garantit **aucun gain** par rapport au noyau 256 existant : le stockage élargi peut détériorer la localité cache. L'exécution n'exige pas AVX2 ; des instructions SIMD ne peuvent être revendiquées que sur preuve du code machine et des caractéristiques du CPU. Les mesures d'un runner hébergé ne prouvent pas la supériorité sur les processeurs Dell T430 ou au niveau SML-GENIUS complet.

Le code reste autonome et sans GPU, dépendance externe, exécution de services ou copie de logiciel tiers. Toute réutilisation par V888-BOOLEEN nécessite son propre oracle de règles de transition et une requalification.
