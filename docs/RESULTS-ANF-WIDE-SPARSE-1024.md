# WideSparseANF — résultats natifs 8→1 024 entrées, adaptateurs récurrents et coût CPU

**Date : 12 octobre 2026. État : première implémentation qualifiée sur GitHub-hosted, benchmark micro-noyau et tests d'oracle exacts.**

Protocole [préenregistré avant le code](ANF-WIDE-SPARSE-1024-PREREGISTRATION.md) au commit `18d233fb468e429c3d44dd559ea39121c4de0ec9`. Après la première mesure, un [addendum de performance](ANF-WIDE-SPARSE-1024-PERF-AMENDMENT.md) a explicitement gelé les valeurs initiales et les deux changements prévus **avant** toute nouvelle exécution. Aucun résultat initial n'a été effacé.

## 1. Limite supprimée, sans masquer les restrictions

Dans le code historique `anf_local.rs`, `LOCAL_ANF_MAX_FAN_IN=8` reste intact pour les appels existants et le chemin dense hérité. **Cette valeur n'est plus un plafond du nouveau moteur** `anf_wide_sparse.rs`, désormais exposé publiquement par `sml_core::anf_wide_sparse` :

- `WideAnfGate::from_monomials(inputs, monomials, source_width, limits)` accepte les **positions physiques de sources** en entrée et des monômes exprimés par index locaux, de longueur dynamique. Un monôme vide vaut 1, zéro monôme donne la fonction zéro. Tous les indices, doublons, débordements de refs et budgets sont vérifiés à la construction.
- `WideAnfLimits` fournit un budget explicite au demandeur (banc : max_fan_in=**2048**, max_monomials=256, max_term_input_refs=8192, max_stored_mask_words=8192, max_prefix_nodes=8192). Le budget n'est **pas** une limitation théorique de l'ANF ; un autre appelant peut transmettre un budget supérieur si sa mémoire est disponible. Ce banc n'a testé que jusqu'à 1024 entrées.
- Choix **automatique et fixe** : `dense` si `k<=8` et `#monômes actifs >= ceil(2^k/4)`, sinon `sparse`. Le backend dense est **l'existant `LocalAnfGate`**, inchangé. Le backend sparse n'alloue jamais les `2^k` coefficients.
- Sparse : stockage des monômes actifs par **masques `Vec<u64>` sur `ceil(k/64)` mots**, et compilation d'un **DAG de préfixes AND partagés**. La racine 0 représente la constante 1 ; chaque nœud calcule `scratch[parent] & source[global_input]` pour **64 séquences simultanées**. XOR uniquement sur les terminaux des monômes actifs ; scratch fourni et **réutilisé par l'appelant**.
- Le chemin scalaire indépendant lit les masques ANF et **ne réutilise pas** le DAG de préfixes du chemin `u64` : l'oracle peut donc détecter une erreur de compilation de préfixes.
- Optimisation après addendum : `required_source_len` est calculé **une fois** à partir du plus grand index d'entrée physique, puis vérifié en **O(1)** avant toute évaluation native ; l'ancienne validation O(k) de tous les index dans la boucle chaude était strictement équivalente mais pénalisait les portes larges aux monômes courts.
- `WideAnfNetwork` exécute réellement une transition de **N états récurrents et la lecture post-transition**, avec `WideAnfNetwork::from_local` pour convertir sans perte un `LocalAnfNetwork` existant. `WideAnfGate::from_large` raccorde aussi les monômes `u32` de `LargeAnfGate` jusqu'à 31 bits. Ce sont des interfaces natives réelles **mais aucun routeur SBG/MOR/Delta-KV existant n'a été automatiquement remplacé**.

## 2. Exactitude — toutes les portes et les deux ponts natifs validés

**Lignes du banc :** 7 largeurs [8,16,32,64,128,256,1024], trois familles `linear32`, `mixed16`, `full_and`, plus une référence k8 dense à 128 coefficients actifs = **22/22**. Chaque ligne compare la sortie native de 64 voies pour **8 patterns** (zéros, uns, alternances, signaux seedés) à un oracle indépendant littéral `XOR(AND(variables))` sur chaque bit. Soit **22 × 8 × 64 = 11 264 comparaisons** bit-à-bit, **0 désaccord**.

**Adaptateurs natifs :**
- Deux trajectoires `LocalAnfNetwork` natives distinctes, initialisées à zéro avec N128, k8, graines 11 et 29, évaluées sur **256 cycles ×64 voies chacune** : **2/2 PASS, zéro désaccord sur 32 768 bits**.
- `LargeAnfGate` existant à 31 bits d'adresse, six monômes incluant le bit 30, converti en `WideAnfGate` et confronté à 64 évaluations de l'ancien moteur : **1/1 PASS, zéro désaccord**.
- Test spécifique d'une porte à **1024 entrées à l'intérieur d'un réseau réellement récurrent à 1024 états** : `step_lanes` et `readout_lanes` exécutés avec succès. Ce test démontre une compatibilité de représentation et d'exécution, **pas** un apprentissage d'une telle porte.
- Tests de rejet de sources non contiguës, tailles de scratch insuffisantes, doublons d'entrée/monôme, termes illégaux et budgets dépassés. Une vérification échouée ne modifie pas le scratch fourni. Exécution en Rust sûr, `#![forbid(unsafe_code)]`.

## 3. Mesures CPU — premières mesures conservées, puis optimisation séparée

Runner des deux mesures : GitHub Actions hébergé, **x86_64, 4 CPU visibles, Ubuntu 24.04, Rust 1.89.0 RELEASE**, avec **8 patterns tournants, 128 appels d'échauffement, 8192 évaluations natives par répétition et 5 répétitions**. Le temps rapporté est la **médiane des 5 durées par appel**, pas une médiane de 40 960 latences indépendantes. Les deux runs ont utilisé deux runners éphémères distincts ; les différences comprennent donc aussi des variations de CPU, ordonnanceur, compilation et le coût d'une réduction de checksum modifiée.

**Médianes en nanosecondes par évaluation d'un paquet de 64 voies.** Avant : contrôle physique d'indice O(k) à chaque appel et checksum XOR annulable. Après : borne d'entrée mémorisée O(1), checksum non annulable.

| k | linear32 avant → après | mixed16 avant → après | full_and avant → après |
|---:|---:|---:|---:|
| 8 | 19,126 → **17,371** | 44,825 → **39,079** | 17,151 → **20,936** |
| 16 | 34,536 → **22,718** | 54,749 → **42,465** | 36,117 → **26,559** |
| 32 | 63,297 → **48,384** | 67,415 → **51,128** | 64,240 → **50,812** |
| 64 | 79,161 → **47,513** | 83,730 → **49,721** | 93,722 → **109,906** |
| 128 | 112,811 → **46,412** | 104,359 → **56,086** | 177,821 → **243,203** |
| 256 | 138,762 → **47,729** | 145,105 → **50,320** | 342,042 → **519,280** |
| 1 024 | 378,175 → **42,739** | 384,757 → **55,535** | 1 341,112 → **2 365,915** |

Référence k8 dense128 : **424,280 → 536,861 ns** par appel. Les nouvelles sorties checksum sont non nulles sur **22/22** lignes, alors que l'XOR périodique du premier run donnait zéro sur toutes les lignes. Chaque ligne a repassé indépendamment le même oracle littéral et les passerelles natives après la modification.

**Interprétation à ne pas extrapoler :** le k1024 avec **32 monômes linéaires** passe de 378,175 à 42,739 ns sur ces deux runs (ratio de médianes **8,85×**) et le k1024 avec **16 monômes non linéaires de degré 2–4** de 384,757 à 55,535 ns (**6,93×**). Il serait néanmoins incorrect d'affirmer que l'optimisation seule fournit ces ratios précis : les runs ne sont pas appariés sur une seule instance CPU ni une mesure A/B contrôlée. Surtout, le **monôme AND de degré 1024 devient plus lent : 1 341,112 → 2 365,915 ns**, un contre-exemple conservé ; son coût résiduel est dominé par **1024 préfixes AND actifs**, à quoi s'ajoutent les variations de mesure et de checksum. Un grand fan-in n'est pas gratuit lorsque le **degré actif** est lui-même grand.

## 4. Budget mémoire déclaré — pas de table dense 2^1024

Mesures du **second run** pour les trois représentations k1024 :

| Famille | Monômes actifs | Préfixes partagés | Masques `u64` | Payload logique de la porte | Scratch `u64` |
|---|---:|---:|---:|---:|---:|
| linear32 | 32 | 32 | 512 | **13 056 octets** | 33 mots |
| mixed16 | 16 | 47 | 256 | **11 120 octets** | 48 mots |
| full_and | 1 | 1024 | 16 | **24 712 octets** | 1025 mots |

Les « octets de payload » additionnent précisément les indices d'entrées de la porte, les mots de masques, les nœuds de préfixes et les identifiants terminaux. Ils **excluent** en-têtes/capacités `Vec`, structures de l'allocateur et RSS global. Le scratch est **fourni séparément** : 33, 48 ou 1025 mots `u64` (respectivement 264, 384 ou 8200 octets), pouvant être réutilisés entre appels. Aucune allocation de `2^{1024}` bits ou de table de vérité n'est effectuée.

La mémoire et le temps de calcul sont liés à **la largeur réellement adressée, au nombre de monômes actifs et aux préfixes nécessaires**, sous limites de sécurité explicites. Pour une ANF dense arbitraire à k1024, la complexité potentielle reste exponentielle : le moteur sparse ne supprime pas cette impossibilité générale.

## 5. Provenance CI et reproduction

**Premier run (baseline) :** `1cdf45d4d444428ea6922013c9befe7de595aa68`, [job 114704820279](https://github.com/Memorithm/SML-GENIUS/actions/runs/38217268362/job/114704820279), **SUCCESS** sur l'expérience dédiée. **Run après addendum et correction O(1) :** `1458f61798b0560e6e52c8d894c5b376a4897a01`, [job 114706563496](https://github.com/Memorithm/SML-GENIUS/actions/runs/38217812927/job/114706563496), **SUCCESS** sur l'expérience dédiée. Les premières corrections préliminaires concernaient `rustfmt`, la conversion de `LargeAnfError` du banc en erreur affichable, le dimensionnement de scratch d'un test k1 et une correction Clippy (test seul) ; elles sont entièrement traçables, sans changer les familles, les graines, le dispatch ou les monômes déclarés.

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --lib anf_wide_sparse
cargo test --locked -p sml-core --bin sml-wide-sparse-bench
cargo run --locked --release -p sml-core --bin sml-wide-sparse-bench
```

Le workflow dédié utilise un runner isolé `ubuntu-24.04`, checkout exact SHA et token non persistant, Rust 1.89 pin, permissions `contents:read`. Aucun runner self-hosted Memorithm/RemoteOps, bibliothèque GPU NVIDIA, SDK NVIDIA, `unsafe`, tâche résidente ni allocation dense k1024.

## 6. Décision et étapes non qualifiées

**H1 : PASS 22/22. H2 : PASS 2/2 réseaux récurrents +1/1 pont k31, plus un test k1024 en réseau. H3 : PASS pour le respect automatique du seuil dense/sparse, la comptabilité mémoire et la mesure honnête des contre-exemples.**

La limite fan-in 8 **cesse d'être architecturale pour ce nouveau backend** mais demeure l'optimisation historique de `LocalAnfGate`. Pour la production SML il reste à :
- raccorder explicitement WideSparseANF aux pages ANF sélectionnées via SBG/MOR et Delta-KV, et comparer les vrais flux autonomes ;
- **apprendre** les monômes utiles au-delà de k8, et mesurer la qualité de généralisation, la complexité de sélection et les budgets de coefficients ;
- obtenir des mesures AVX/NEON/SIMD puis GPU natif sur Thor, avec vérifications physiques et sans SDK NVIDIA ;
- valider un véritable `K=1 048 576` de **fonctions** en banque, les budgets d'activation/scratch et les garanties de mémoire élastique, sans confondre fonctions adressables, portes physiques, monômes actifs et activations/token.

Les résultats ne prouvent ni l'inférence SML-25B, ni une qualité linguistique, ni 100 millions de portes actives, ni un gain sous-cubique de recherche de structures complexes. Il s'agit d'un **noyau natif fonctionnel, récurrent et vérifié** permettant d'étudier ces objectifs séparément.
