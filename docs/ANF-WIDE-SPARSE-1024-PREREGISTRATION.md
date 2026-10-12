# WideSparseANF — préenregistrement du noyau Rust et du banc fan-in 8→1 024

**Gel : 12 octobre 2026, avant tout code, essai ou résultat de cette nouvelle tranche.** Le dépôt courant est basé sur la branche `main` après la PR #497. La demande est d'éliminer **la limite architecturale de huit entrées** des nouvelles portes ANF, sans changer les contrats des moteurs denses existants.

## Base de référence et hypothèses falsifiables

`crates/sml-core/src/anf_local.rs` a `LOCAL_ANF_MAX_FAN_IN=8` et évalue **tous** les `2^k` produits via la méthode dense `LocalAnfGate::evaluate_lanes`, même si peu de monômes sont actifs. `anf_large.rs` propose une représentation sparse `u32` pour **1..31 bits d'adresse** ; elle n'est pas câblée à `LocalAnfNetwork`.

**H1 — exactitude :** un **nouveau** noyau public `WideAnfGate` doit évaluer les polynômes ANF GF(2) sur des fan-in **8, 16, 32, 64, 128, 256, 1024**. Pour les entrées scalaire et u64 bit-slicé, tous les résultats d'un panel d'oracle littéral indépendant et des flux seedés doivent être **bit exacts (zéro désaccord)**, y compris `AND` sur **toutes** les entrées et les constantes. Tous les monômes ne doivent être stockés qu'en représentation compressée, jamais comme `2^k` coefficients au-delà du seuil dense.

**H2 — intégration :** le noyau doit adapter **sans changement des résultats** un `LocalAnfGate` dense existant, le `LargeAnfGate` sparse 31 bits existant, et un `LocalAnfNetwork` complet avec récurrence (états, entrée, readout). Comparer les sorties **bit à bit sur des trajectoires nativement récurrentes**, pas uniquement des fonctions isolées.

**H3 — densité, coût et sécurité :** le backend choisi automatiquement doit être `dense` si `k<=8` et si `active_terms >= ceil(2^k/4)` ; sinon `sparse`. Le dense doit réutiliser exactement `LocalAnfGate` (rétrocompatibilité), tandis que le sparse stocke **des masques dynamiques multi-mots `Vec<u64>`** et compile un **préfixe partagé** pour réutiliser les produits intermédiaires entre les monômes actifs. Pour chaque porte publier type de backend, densité, nombre de monômes/masques, nombre de préfixes, octets de charge utile mesurés par contrat, mots de scratch, checksum des résultats et débit CPU réel. Ne pas confondre **octets de charge utile définis** avec la mémoire résidente RSS et ne pas prétendre à une accélération GPU. Les candidats particulièrement denses ou énormes doivent être **refusés avant une allocation prohibitive** par des limites de ressources explicites et propagées.

## Sémantique native requise

La porte `WideAnfGate` reçoit des indices de sources globales non dupliqués et des monômes actifs composés d'indices **locaux** de variables, sans doublons, indexés sur `0..k`. Un monôme vide signifie la constante 1 ; **zéro monôme** représente la fonction identiquement nulle ; la composition XOR des conjonctions suit la GF(2) ANF. Les monômes duplicatifs sont rejetés (pas annulés implicitement). Les entrées dupliquées ou hors domaine et les budgets dépassés échouent fermés. Les variantes vecteur dense/sparse utilisent des mots `u64` en **64 lanes simultanées** sans `unsafe` ni dépendances externes ; le cache partagé est **un scratch réutilisé par l'appelant** et non un service résident.

Le graphe de préfixes partagés est compilé avec ordre de variables canonique (croissant), un nœud = `scratch[parent] & source[input]`; plusieurs monômes partagent les préfixes. Une constante 1 utilise `scratch[0]=u64::MAX`. Le XOR ne parcourt que les terminaux des monômes effectivement présents. Les masques de monômes sont encodés en `ceil(k/64)` mots et les limites de métadonnées/refs/masques sont explicites. Toutes les allocations d'essais passent par `try_reserve` lorsque cela est pertinent ; aucune table de `2^k` bits n'est allouée en backend sparse.

L'adaptateur `WideAnfNetwork::from_local` copie **sans altérer** la logique des `LocalAnfGate`, leurs entrées physiques et le readout. `WideAnfNetwork` réexécute réellement la transition/récurrence native sur un état bit-slicé, avec `state[0..N]` puis `inputs[0..W]`, et scratch réutilisé par le même flux. Elle constitue un **pont natif testé**, pas le remplacement automatique de toutes les pages SBG/MOR/Delta-KV existantes.

## Panel / budgets figés

- Largeurs `[8,16,32,64,128,256,1024]` : **trois fonctions représentables** par largeur, soit **21 lignes**, plus un témoin dense à k8 avec **128 coefficients actifs** (**22 lignes**).
- Famille `linear32` : `min(k,32)` monômes de degré 1 aux variables espacées selon k, uniquement ceux présents.
- Famille `mixed16` : **16** monômes distincts de degré 2..4 (les k8 étant assez larges), construits de façon déterministe ; aucun oracle qui modifie la fonction après lecture des résultats.
- Famille `full_and` : **1** monôme de degré `k`, c'est-à-dire une porte AND exacte de toutes les entrées (1024 inclus).
- Témoin dense `dense128` : k8, 128 monômes choisis par l'index binaire des coefficients actifs `m=0..127`. Représentation dense à densité 50% du support possible.
- Contrôle fonctionnel indépendant : pour **chaque ligne**, comparer la sortie native `u64` aux 64 évaluations littérales `AND`/XOR par voie, avec plusieurs motifs déterministes et seedés, dont entrées tout-à-zéro et tout-à-un. `WideAnfNetwork` contre `LocalAnfNetwork` sur **N128, 256 pas, 64 voies**, deux graines natives, et conversion `LargeAnfGate` k31 avec quelques masques d'indices élevés.
- Bench de débit : par ligne, **8192 évaluations natives×5 répétitions** avec 8 jeux d'inputs `u64` figés mais différents (séquence tournante), échauffement 128 évaluations avant mesure ; utiliser le **temps médian** des cinq répétitions en nanosecondes par appel, et les extrêmes min/max. Toutes les comparaisons de débits sont descriptives ; différentes fonctions ont une complexité différente, les mesures incluent les allocations de scratch de l'appelant **uniquement avant** la chronométrie.
- Contrat de budget passé à chaque construction : `max_fan_in=2048`, `max_monomials=256`, `max_term_input_refs=8192`, `max_stored_mask_words=8192`, `max_prefix_nodes=8192`. Refuser explicitement les allocations qui dépassent ces limites ; des budgets plus grands peuvent être passés explicitement, la valeur 2048 n'est **pas** une limite intrinsèque de l'algèbre.
- Plan de validation : compilation, tests unitaires, clippy strict et `cargo run --release` sur runner GitHub-hosted Ubuntu 24.04, Rust 1.89. Vérifier **exactement 22 lignes** `WIDE_ANF_ROW`, **22 PASS** d'oracle, **2 PASS** de récurrence native et **1 PASS** du pont `LargeAnfGate` 31 bits.

## Critères de succès sans réécriture a posteriori

H1 **PASS** uniquement si **22/22 résultats bit-exacts** avec les 64 voies et les motifs adversariaux. H2 **PASS** si **2/2** trajectoires récurrentes adaptées sont identiques à `LocalAnfNetwork`, plus **1/1** pont k31 exact, avec états native zéro et les mêmes inputs. H3 **PASS** si toutes les lignes déclarées sont mesurées et si le backend dense/sparse est **conforme au seuil figé** avec les bornes de mémoire/scratch respectées et les erreurs invalides rejetées. Une vitesse inférieure au dense à k8 n'est pas cachée ; ce n'est pas un critère éliminatoire tant que la correction est assurée, mais elle doit apparaître dans le rapport.

**Limites :** 1024 entrées admissibles ne signifient ni 1024 monômes nécessaires, ni la découverte automatique de leurs monômes, ni une banque de 1 048 576 fonctions ANF effectivement entraînées, ni 25B, ni 1024 entrées à exécuter simultanément dans tous les états. Aucun backend GPU/SIMD explicite n'est validé ici (les u64 lane words permettent déjà le bit-slicing CPU). Les routines restent purement Rust sans SDK NVIDIA ; les futures voies d'apprentissage, routage et SBG seront qualifiées séparément.

**Fusion :** aucune fusion avant résultat complet documenté, passage vert de la CI dédiée et des contrôles généraux sur le **dernier SHA** ; préenregistrement maintenu sans révision rétroactive.
