# GB-6 — amortissement de la transposition de 64 directions sur une banque d'experts

**Préenregistrement gelé le 23 octobre 2026, avant toute mesure GB-6.**
Autorité scientifique : Memorithm/SML-GENIUS, référence GB-5D native sur Dell et Thor ([journal de validation RemoteOps #39962386271](https://github.com/Memorithm/RemoteOps/actions/runs/39962386271), code SML `9c18996d56f34abee7b53b73e2740f252e0972a2`). Cette étude ne modifie aucun poids, routeur productif, entraînement ou runtime GPU.

## Hypothèse et résultat attendu falsifiable

La transposition `[sector;64] -> [u64;6]` a pénalisé la méthode ANF bit-slicée lorsqu'elle n'était utilisée qu'une fois : sur GB-5D, l'arm `bitslice_including_pack` n'a gagné dans aucune des 12 cellules des deux machines. Si un lot de 64 adresses reste identique pendant **R calculs de prédicats distincts**, son coût de transposition pourrait être amorti. Hypothèse à tester, pas un avantage démontré.

Comparer exactement les mêmes `64 × R` sorties logiques de R circuits, dont la sélection est **sans entraînement** et figée avant les mesures. Ne pas mesurer le classificateur de directions géométriques : la frontière chronométrée commence avec les identifiants de secteurs déjà calculés.

## Matrice fixe

- `M∈{8,16,32,64}` secteurs, `R∈{1,2,4,8,16,32}` fonctions (banque) et toujours 64 lanes.
- `famille∈{sparse, alternating, pi}` :
  - `sparse` : bit de cellule `((7*g)%M)` pour le g-ième circuit ;
  - `alternating` : motif pair/impair `0xAA...`, XOR d'un bit `((5*g)%M)` pour faire varier les fonctions ;
  - `pi` : le g-ième masque consomme les M bits à l'offset `g*M` du préfixe binaire π qualifié GB-1, sans rebouclage. Le dernier bit à M=64 et R=32 est à l'offset 2047, inférieur à 4096.
- `queries∈{cyclic,clustered}` :
  - `cyclic` : adresse `i mod M` pour `i=0..63` ;
  - `clustered` : adresse `floor(i/4) mod M`. Les deux modes contiennent 64 directions logiques valides, mais diffèrent en diversité et localité.
- Aucun dataset d'entraînement ; les fonctions `pi` sont déterministes, pas un échantillonnage i.i.d. Le total est 4×6×3×2 = **144 configurations**, sans sélection a posteriori.

## Trois exécutants et oracle indépendant

1. **LUT_SCALAR** : construire R mots u64 de sorties pour les 64 positions via lecture directe du bit `FanMask.bits()`.
2. **BITSLICE_PREPACKED** : préparer les six bitplanes *hors chrono*, puis calculer les R sorties par `AnfCircuit64.evaluate_bitplanes()`. C'est une borne de coût si l'entrée est déjà transposée en amont.
3. **BITSLICE_ONE_PACK** : pour chaque lot logique, préparer les six bitplanes **une seule fois** puis calculer les R sorties à partir de ceux-ci. Ce bras inclut le coût réel de préparation.

L'oracle est le LUT scalaire sur **toutes** les 64×R décisions. Un résultat rapide sans correspondance exacte sur chaque sortie échoue, quelle que soit sa médiane. Vérifier chaque résultat final du passage chronométré contre l'oracle et la stabilité du checksum, et publier les échecs.

## Chronométrage

- Préparation des banques et de l'ANF hors chronométrage, dans les trois bras. Scratch `Vec<u64>` préalloué de même longueur dans tous les bras, sans allocation de sorties par opération. Les opérations mesurées ne comprennent ni classification géométrique ni compilation ANF.
- Par configuration, `iter = 4096/R`, toujours entier : **4096 applications circuit×lot par répétition**, pour comparer les différents R dans un budget de travail logique constant.
- 3 préchauffages, puis **9 mesures brutes par bras**, ordre des bras décalé modulo trois d'une répétition à l'autre ; ratio de médianes, min/max et bruts conservés.
- Temps normalisé en `ns / (64×R×iter)`, mais l'unité de service est aussi le **temps par lot 64×R**. Le coût de packing du bras 3 est ainsi payé une fois par lot de R circuits.
- Même SHA exact, Rust 1.89.0, flags Release et machine native pour tout rapport. Rapports Dell et Thor séparés, pas de moyenner leurs ns absolues.
- Conditions CPU (affinité, fréquence, concurrent-load) observées mais non supposées contrôlées. Un gain ponctuel est diagnostique ; reconfirmer avec répétitions de campagnes avant la promotion.

## Décisions

- Indiquer, pour chaque combinaison largeur×famille×pattern, le plus petit R mesuré où la méthode **BITSLICE_ONE_PACK** est plus rapide que LUT_SCALAR, s'il existe ; ne pas extrapoler le point de bascule.
- Répondre séparément si prépackée gagne et si la transposition amortie gagne. Une réussite dans une seule famille ou une seule machine ne constitue pas un gain universel.
- Si le bit-slicing gagne, étude suivante : vérifier la disponibilité de lot de R circuits sur un vrai moteur `Pvp6Bank` / `SbgAnfExpert`, avec métriques propres au modèle. Si tous les bras complets restent perdants, cesser la promotion matérielle et chercher un autre mécanisme ou une réduction de packing.
- Aucune promesse de compression de mémoire, énergie, GPU, attention replacement, langue naturelle ou modèle de 25 milliards de paramètres.

## Études complémentaires séparées, *non qualifiées par GB-6*

**GB-7 / mémoire et énergie** : captures RSS/VmHWM et allocation peak, énergie CPU uniquement via capteurs exposant unité, provenance, fenêtre et valeur de référence inactive. Si RAPL/compteurs Thor ne sont pas accessibles, marquer « unmeasured » et ne pas substituer le TDP.

**GB-8 / SIMD, NEON, GPU portable** : ablations exactes Rust scalaire et packed, vérification vectorisation réelle (code machine/compteur), NEON aarch64 et WGPU si une implémentation propriétaire facultative est autorisée. Pas de stack GPU NVIDIA obligatoire hors pilotes.

**GB-9 / qualité SML** : jeu train/dev/test figé pour les six tâches `sml_core::tasks` (delayed recall, overwrite, keyed facts, copy, conjunction, distractor rejection), plus séquences longues et tests OOD. Baselines à capacité, budget d'entraînement et protocole identiques : routeur SML existant, candidat GB, modèle sans GB et témoin attention/Transformer si disponible. Mesurer taux de réussite, généralisation, latence et mémoire ; les fixtures synthétiques seules ne prouvent pas la qualité en langue naturelle.

**GB-10 / architecture sans attention** : tests de capacité sur retrieval long contexte, copie/composition, séquences adversariales et perplexité/token accuracy avec baselines Transformer matched-token/compute. Un gain sur bits booléens n'établit pas le remplacement de l'attention.

Les gates expérimentaux sont indépendants. Pour fusionner le binaire GB-6 dans `main`, exiger sur SHA PR exact `cargo fmt --all -- --check`, `cargo check --locked --workspace --all-targets`, `cargo clippy --locked --workspace --all-targets -- -D warnings` et `cargo test --locked --workspace`.
