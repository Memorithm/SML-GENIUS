# GB-9 — circuits bit-slicés dans les banques Pvp6Bank réelles

**Préenregistrement gelé le 23 octobre 2026, avant toute mesure native GB-9.**
Référence : GB-6 natif Dell/Thor ([RemoteOps PR #799](https://github.com/Memorithm/RemoteOps/pull/799), workflow #39977860338, `docs/GB6-DELL-THOR-NATIVE-EVIDENCE-20261023.md`) et GB-7 (capteurs, RemoteOps PR #800). Cette étude ne modifie ni poids, ni routeur productif, ni politique de sélection PVP6, ni entraînement.

Numérotation : le préenregistrement GB-6 nommait « GB-9 » l'étude de qualité SML. À la demande du propriétaire, GB-9 désigne désormais **l'intégration au moteur réel** que GB-6 annonçait comme étape suivante ; l'étude de qualité (apprentissage, généralisation, témoins) reste à faire et n'est pas qualifiée ici.

## Ce qui est intégré

- `Pvp6ExecutionPath { Selected, Scalar(arch), Bitsliced }` et `Pvp6Bank::evaluate_on`. `evaluate` reste strictement `evaluate_on(Selected, …)`. Le bras `Bitsliced` transpose chaque bloc de 64 requêtes logiques en `log2(D)` bitplanes `u64`, partagés par **toutes les portes de la banque** ; un monôme est l'AND de ses plans, une porte le XOR de ses monômes. La sortie gate-major est écrite directement, sans réorganisation.
- `SbgAnfExpert::execute_on` et `SbgExpertAtlas::execute_on` propagent le bras ; projection SBG, contrôle des écritures disjointes, réécriture d'état et mots query-major sont inchangés.
- Le libellé `selection` du résultat reste celui de la politique dans tous les bras. Forcer un bras est un contrôle diagnostique : **aucune règle de routage automatique n'est ajoutée**.

## Exactitude (obligatoire avant toute mesure)

- Tests unitaires : égalité **structurelle complète** de `Pvp6BatchResult` entre `Selected`, les trois bras scalaires forcés et `Bitsliced`, pour D ∈ {64, 128, 256, 1024, 4096}, 1/3/64/65 portes, densités 1/2/7/D/2, 1 à 129 requêtes, reuse 1 et 3, plus un oracle littéral indépendant. Égalité complète de `SbgExpertBatch` au niveau atlas, y compris l'erreur sur sélection en double.
- Banc : avant chronométrage, sur chacun des 16 états figés de chaque cellule, égalité complète des lots moteur, égalité des mots gate-major des quatre bras noyau, cohérence noyau ↔ moteur (`to_query_major_words`) et oracle littéral par porte et par lane. Chaque passage chronométré vérifie à nouveau son checksum.

## Matrice fixe

- Domaine `D ∈ {64, 256, 1024}` (soit 6, 8, 10 variables ; D=64 correspond aux « 64 secteurs » de GB-6). Les largeurs 8/16/32 de GB-6 n'existent pas dans `Pvp6Bank`, dont le minimum est 64.
- Densité ANF uniforme par porte `k ∈ {2, 8, 32}` monômes distincts, tirés par LCG déterministe. k=32 à D=64 approche la densité des familles π/alternance de GB-6.
- Portes par banque `R ∈ {1, 2, 4, 8, 16, 32, 64}` : ce sont les « experts » de GB-6, qui partagent les mêmes entrées.
- Lanes par page `Q ∈ {8, 64}` : 64 remplit un mot ; 8 mesure le cas de pages étroites.
- Atlas de 4 pages, routeur MOR natif (`SemanticMorBridge::route`, top-1, `max_experts = 1`), signatures aléatoires figées ; `SemanticState<1024>`. 16 états figés par cellule, parcourus cycliquement. Le routage varie réellement d'une page à l'autre (test).
- Toutes les cellules restent sur le bras de politique `PASCAL_ANF_MATERIALIZE` (vérifié), donc `engine_selected` est le chemin de production actuel.
- 3 × 3 × 7 × 2 = **126 cellules**, 8 bras, soit 1 008 cellules de validation et 9 072 mesures par machine, sans sélection a posteriori.

## Bras

0. `route_only` — décision MOR seule, pour séparer le routage du reste du surcoût moteur.

Moteur complet, une itération = routage MOR + sélection atlas + projection des requêtes + banque + réécriture d'état + sorties query-major :

1. `engine_selected` — chemin de production (Pascal matérialisé à chaque appel puis lecture).
2. `engine_sparse_anf` — ANF scalaire forcé.
3. `engine_bitslice` — transposition incluse.

Noyau seul (requêtes déjà projetées, ni routage ni réécriture) :

4. `kernel_lut_resident` — table de vérité résidente précalculée hors chrono : contrôle équivalent au `scalar_lut` de GB-6, sans chemin de production correspondant (`SbgAnfExpert` ne conserve aucun cache).
5. `kernel_selected`, 6. `kernel_sparse_anf`, 7. `kernel_bitslice`.

## Chronométrage et décisions

- `iter = 4096/R` appels par répétition ; 3 préchauffages puis 9 mesures, ordre des bras décalé à chaque répétition. Release, Rust 1.89.0, même SHA sur Dell et Thor, rapports séparés.
- Seuil : pour chaque (D, k, Q) et chaque couple (référence, bit-slice) du même niveau, le **premier R testé** où le ratio des médianes dépasse 1. Pas d'extrapolation entre points.
- Si `engine_bitslice` gagne contre `engine_selected`, le gain survit au coût complet dans cette cellule. S'il ne gagne qu'au niveau noyau, les surcoûts de routage et de matérialisation l'absorbent. Un gain sur une seule machine reste diagnostique.
- Énergie : **non mesurée** (GB-7 : RAPL illisible sur Dell, INA238 de Thor non attribuable au programme). Aucune mémoire, SIMD/NEON, GPU ni qualité de modèle n'est revendiquée.
