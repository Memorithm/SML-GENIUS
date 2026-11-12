# P8-BS64 — Calcul booléen sur 64 posit8 en parallèle

**Programme Memorithm indépendant, Rust CPU-only.** Les circuits ADD/MUL posit8 générés par les graphes à arêtes complémentaires sont évalués sur 64 couples simultanément à l'aide de masques d'entiers `u64`. Chaque bit de chaque mot représente la même variable logique pour un exemple indépendant ; XOR, AND et inversion agissent sur les 64 exemples en une instruction arithmétique native à largeur 64 bits.

## Contrat de calcul

1. Entrées : exactement 64 paires `(u8,u8)`, sans table de résultats. Les 16 variables d'entrée donnent 16 plans de bits `u64`.
2. Mémoire de travail réutilisable et propre au graphe : `(nodes + 1) * sizeof(u64) + 16 * sizeof(u64)` octets, sans allocation dans la boucle de calcul. Les entrées d'un appel ne sont pas conservées pour le suivant.
3. Chaque nœud est évalué une seule fois pour les 64 entrées : `low XOR ((low XOR high) AND selector)`; une arête complémentée est appliquée par XOR avec `u64::MAX`. Le terminal faux vaut zéro et le terminal vrai vaut `u64::MAX`.
4. Les huit plans de sortie sont transposés vers 64 encodages `u8`. Une taille de workspace incohérente est rejetée sans écrire dans le tableau de résultats.
5. Les 65 536 paires d'opérandes sont vérifiées par **1 024 lots consécutifs de 64 couples** pour chaque opération, contre l'oracle exact en nombres entiers existant. Les cas NaR, zéro et saturation sont couverts ; les tests supplémentaires les concentrent dans un lot.
6. Aucune matrice flottante, table d'inférence, modèle tiers, dépendance GPU ou service réseau n'intervient.

## Adaptateur de taille arbitraire

Un second point d'entrée accepte un nombre quelconque de couples, y compris 0, 1, 63, 65, 129 ou 1 024 entrées. Il découpe les entrées en tranches de 64, complète uniquement le dernier lot par des zéros dans la pile locale, puis copie seulement les sorties réellement demandées. La mémoire de calcul est conservée entre les lots. Un désaccord entre les tailles des tableaux d'entrée et de sortie, ou une mémoire de travail incompatible avec le graphe, provoque un rejet **avant toute modification des sorties**. La qualification comprend une campagne d'exactitude sur des longueurs non alignées et les situations d'erreur.

## Limitations et comparateurs

Le chemin `u64` est **portable** et ne contient pas de code SIMD AVX2 explicite. Un débit supérieur à 64 fois le noyau scalaire n'est pas garanti et ne doit pas être revendiqué. Une comparaison contre l'assembleur natif scalaire et un benchmark sur le même matériel restent nécessaires ; le coût de transposition entrée/sortie fait partie du travail réel et doit être comptabilisé. Les qualités numériques du modèle complet ne sont pas mesurées. L'équivalence obtenue est relative à notre oracle et non à une qualification normative externe du Posit Standard.

La réutilisation future dans V888-BOOLEEN est conditionnée à un contrat de largeur, une preuve d'équivalence des règles de transition, et des comparaisons sur données neuronales réelles ; ne pas assimiler une accélération de fonctions logiques à une amélioration de l'apprentissage.

Qualification physique éventuelle sur Dell uniquement sous Memorithm/RemoteOps, avec admission et conservation des charges existantes.
