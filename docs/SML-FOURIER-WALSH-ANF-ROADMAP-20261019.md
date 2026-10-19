# Feuille de route SML-GENIUS — Fourier–Walsh × ANF
Date : 2026-10-19. Statut : plan scientifique, sans nouveau résultat expérimental.

## Décision

Faire de l'analyse spectrale des fonctions booléennes (FABF) un **moteur parcimonieux de découverte et de qualification** des fonctions pertinentes. Conserver l'ANF / GF(2) comme représentation exécutable et les banques MOR/ANF comme infrastructure du modèle. Ne pas ajouter un décodeur spectral permanent ni prétendre que la FABF remplace RTRL, MOR, MSB-first ou la mémoire gouvernée.

Construire cette voie à partir des travaux **déjà implémentés et mesurés**, sans dupliquer les modules ni changer les seuils A6-E5 déjà préenregistrés.

## Fondement et pièges à éviter

Une fonction booléenne f de n bits se transforme en signe g(x)=(-1)^f(x). La base Walsh est chi_S(x)=(-1)^{XOR des bits x_i pour i∈S}. Sous distribution uniforme indépendante, le coefficient ghat(S) = E[g(X) chi_S(X)] est une coordonnée orthogonale ; une parité pure n'a qu'un coefficient non nul. La forme ANF, sur GF(2), est f(x)=XOR_S a_S produit_{i∈S} x_i. AND a un monôme ANF, mais un spectre Walsh généralement dense. L'intérêt est de choisir une représentation de coût inférieur suivant le signal ; pas de supériorité universelle.

Si la distribution des observations est non uniforme, une corrélation empirique sur TRAIN est **pondérée par cette distribution**, pas automatiquement un coefficient orthogonal uniforme. Ne pas confondre score spectral, influence, attribution causale et avantage prédictif. Une FWHT complète coûte O(n 2^n) en opérations et O(2^n) en mémoire : seulement oracle pour petites largeurs n≤16. Une banque K=1 048 576 fonctions ANF n'est pas une FWHT sur K entrées ; distinguer nombre de fonctions K, largeur des features n et fan-in d d'une porte.

La récupération de parité avec bruit d'ordre quelconque (LPN) n'est **pas** résolue par la méthode W3 exhaustive de petite dimension.

## Acquis vérifiés dans le dépôt : NE PAS recréer

| Référence | Constat | Limite |
|---|---|---|
| docs/RESULTS-A6-C12A-SPECTRAL-ADVERSARIAL.md | Marginal Walsh faible ordre et peeling XOR : échec top24 sur deux familles adversariales représentables, dont XOR3 et AND+XOR+XOR (0/20 chacune) | Agrandir le faisceau APRES filtrage ne réintroduit pas une source supprimée |
| docs/RESULTS-A6-C12B-GF2-WALSH23.md | Gauss GF(2) trouve les parités propres ; W2 récupère des composites mixtes ; W3 retrouve la vérité sur grilles n≤128, bruit fixé | W3 balaie 317 440 structures admissibles à n128 ; >21 M pour 508 sources : **la mesure à 508 est effectuée dans C12c**, ne pas dire qu'elle manque |
| docs/RESULTS-A6-C12C-SKETCH-MITM-508.md | **Hybrid W2 + Sketch/MITM** déjà implémenté : 24/24 récupérations à 508 sources sur quatre familles, ~35,06× plus rapide que W3 exhaustive sur huit comparaisons natives appariées | pas de borne pire cas sous-cubique, trois portes et topologie fixe ; Sketch seul échoue sur les compositions AND |
| docs/RESULTS-A6-C12D-CORRELATED-IDENTIFIABILITY.md | sous distracteurs fortement corrélés : 12/12 récupérations hybrides ; alias exacts démontrés non identifiables structurellement | équivalence fonctionnelle ≠ récupération des noms des sources |
| docs/RESULTS-A6-C12E-TEMPORAL-STATE.md et docs/RESULTS-A6-C12F-STATE-NOISE-OBSERVABILITY.md | alias temporels, bruit latent d'état et égalité native des sorties déjà testés, y compris 72/72 récupérations déterministes F | pas de prédiction des réalisations aléatoires ni de graphes libres |
| docs/RESULTS-A6-C12GH-NOISE-PHASE-QUALIFIED.md | à bruit élevé la W3 retrouve davantage de parités pures que W2+MITM : parité avec bruit 40 %, W3 16/16 à 32768 bits et hybride 0/16 au même budget | **goulet d'étranglement du filtrage pairwise**, et non une preuve LPN générale |
| docs/RESULTS-A6-I5-TRIPLE-COVERAGE.md | garde exact de support d'ordre 3 ; **échec obligatoire d'une interaction cachée d'ordre 4** conservé | aucun algorithme d'ordre ≤3 ne garantit cette détection par simple observation |
| crates/sml-core/src/anf_e5_spectral_beam.rs | Projection Walsh streaming ≤96 masques, ≤8192 observations ; composition AND de parités compilée par Möbius GF(2), faisceau/MDL borné | référence de noyau, pas preuve de gain linguistique |
| crates/sml-core/src/anf_e5_logloss_arbiter.rs | arbitre contextuel à cinq paramètres, loss propre et gardes temporelles OOF | l'appelant reste responsable de la provenance réelle des folds |
| crates/sml-core/src/anf_e5_materialization.rs et anf_wide_sparse.rs | planificateur DIRECT vs table compilée selon budget RAM/L3 et coûts mesurés | ne pas allouer 2^n ni confondre K et d |
| docs/A6-E5-NUMERIC-FROZEN-BEFORE-CODE.md | corpus, masques, splits, seuils représentation/modèle et budget gelés | aucun ajustement rétroactif sur TEST autorisé |

Les travaux A6-E5 existants incluent aussi un corpus français Flaubert #14155 déjà fingerprinté et un protocole de validation 256 classes. Vérifier son état TEST et les droits dans les artefacts avant toute exécution, **sans y lire des métriques pour modifier cette feuille de route**.

## Tranches et ordre d'exécution (une PR contrôlée par étape)

| Phase | Priorité | Code/document à produire | Gate objectif |
|---|---|---|---|
| F0 — inventaire et gels | immédiat | matrice des fichiers/CI/code C12/E5, sources et empreintes, oracles de représentabilité, paramètres, contrôles négatifs, budgets et tests disjoints | préenregistrement enregistré dans Git AVANT chaque nouvel essai ; aucun TEST ouvert |
| F1 — oracles mathématiques | P0 | FWHT exacte limitée à n≤16 ; Walsh naïf indépendant, Parseval, orthogonalité et spectres de XOR/AND/majorité ; conversion Möbius/ANF testée exhaustivement | égalité oracle à précision déclarée ; signaler mesures non uniformes ; refuser n>16, overflow et allocation hors budget ; Rustfmt/Clippy/tests |
| F2 — sélection adaptative des moteurs existants | P0 | **réutiliser W2+Sketch/MITM de C12c, W3, Gauss** ; construire un méta-sélecteur TRAIN-only (statistiques d'énergie/corrélation/support + budget CPU) choisissant recherche rapide ou contrôle W3 borné selon la difficulté de la parité | reproduire C12c/d/e/f/g/h à splits figés ; surtout parité bruitée 20/30/40 % où MITM échoue ; étudier faux négatifs/positifs, rappel, CPU total et coûts de sélection ; aucun oracle de la vraie structure dans le méta-sélecteur |
| F3 — au-delà du domaine C12 | P1 | **ne pas refaire** les alias corrélés/temporels C12d/e, bruits d'état C12f et échelles C12g/h déjà mesurés ; étendre vers entrées partiellement masquées, perturbations simultanées/corrélées et grammaires apprenables de k4/5 ; ajouter contrôle de couverture triple I5 et quadruple aveugle | séparer équivalence fonctionnelle de paramètres non identifiables ; oracle représentable, ablations, témoin I5 d'ordre4 non récupérable, TEST vraiment indépendant ; FAIL publiés sans réglage a posteriori |
| F4 — spectral next-byte | P1 | connecter le mécanisme F2 aux modules E5 déjà natifs : propositions TRAIN_probe OOF, sélection TRAIN_inner_VAL, backoff multinomial, arbitre log-loss sur prédictions hors fold, compilation ANF bornée | contrôles appariés NG4, RAW32 greedy/Beam, ANF greedy/JOINT, spectre sans backoff, mélange fixe, arbitre et composites annulés ; 256 classes et mêmes indices par bras |
| F5 — qualification de modèle | P1 conditionnelle | banc A6-E5 gelé, journaux sources/poids/masques/hyperparamètres SHA avant première lecture de TEST | REPRESENTATION_PASS uniquement avec ≥0,01 bit/octet face à RAW Beam ET ANF greedy plus véritable composite non équivalent RAW ; MODEL_PASS exige cela et ≥0,02 vs NG4 ainsi que les avantages prescrits face à ANF-JOINT et spectre sans backoff/arbitre ; sinon STOP |
| F6 — coût natif et intégration | P2 conditionnelle | Dell/Thor source SHA identique via RemoteOps ; suivi RSS réel, allocations, latence p50/p95, CPU, working set, Q4, pages et coûts de construction ; routage par gain qualité/coût | seuil A6-E5 existant : mémoire additionnelle de la banque ≤8 MiB et coût moyen décodage ≤2× RAW Beam ; pas de récurrence admise sans gain contre banque booléenne également informée |
| F7 — frontière de modèle | P2 | contrat versionné de candidats spectraux vers fonctions WideAnfGate/MOR, avec masque, signe, support, gain OOF, coût, empreinte source, route fixe | aucune lecture d'historique complet ni matrice 2^n ; intégrer seulement un composant qui surpasse contrôles de même budget sur qualité ET ressources ; promotion SML-1/SML-2 par gates indépendants |

P0/P1/P2 sont des dépendances techniques et non des garanties de date. La première PR de code après cette feuille de route est **F1, oracles exacts avec tests synthétiques**. **F2 n'implémente pas un second Sketch/MITM** : il préenregistre une stratégie de sélection TRAIN-only **entre** l'hybride existant et W3, avec validation sur le contre-exemple à bruit élevé C12g/h. Ne pas commencer F5 tant que F4 et tous ses contrôles ne sont pas figés.

## Discipline expérimentale obligatoire

1. Scission TRAIN_fit → TRAIN_probe OOF → TRAIN_inner_VAL → outer_VAL → TEST strictement inaccessible jusqu'au gel final. L'API locale ne prouve pas une provenance externe correcte : enregistrer la provenance SHA et les bornes.
2. Conserver les mêmes tâches, cibles et budget pour les méthodes comparées. La sélection spectrale est un **classement de candidats** et non une preuve de causalité. Tests des faux positifs sur null, XOR3 sans corrélations marginales, AND mixte, bruit LPN et IID.
3. Score Walsh partiel avec budget explicitement borné en nombre de masques, observations, ordre de parité et fan-in. Le rang du véritable triplet est une **mesure après coup**, jamais une information fournie au découvreur.
4. Mesurer temps de proposition + tri + complétion + calcul Möbius/ANF + calibration + inférence et mémoire effectivement allouée. Ne pas confondre tableau de vérité stocké en bits, taille du processus et nombre de paramètres appris.
5. Tester un corpus **externe d'auteur distinct et droits admis** après résultats synthétiques, sans réutiliser les mêmes livres exploratoires comme de nouveaux TEST indépendants. Documenter les effets de distribution et les différences par blocs/document.
6. Résultat négatif = conclusion scientifiquement valide. Aucun contournement par modification silencieuse de seuil, agrandissement post hoc du top-k, fuite TEST, ou affirmation qu'une référence synthétique est un SML25B entraîné.
7. Corpus SEC : **G0 NOT_ADMITTED** ; TEST 2024 **UNOPENED**. Pas de déplacement sur Thor, pas de charge GPU ni pile logicielle NVIDIA hors pilotes. Les expérimentations restent en Rust.

## Gouvernance, parallélisation et livrables

- SML-GENIUS est propriétaire du code algébrique, des oracles, baselines, validations et du modèle ; RemoteOps est le **plan de contrôle matériel et la provenance des artefacts** Dell/Thor. Ne pas recopier cette logique dans Forge/ProofLab/GOT, ni créer SML-HARNESS avant le jalon architectural défini dans docs/ROADMAP.md.
- Chaque F1..F7 : (a) protocole en PR, (b) code/tests Rust, (c) CI source verte, (d) campagne indépendante/contrôles, (e) décision PASS/FAIL/STOP avec artefacts SHAs, (f) fusion si protections requises vertes. Ne pas attribuer un gain de mémoire à une simple taille logique de bits.
- La piste MSB-first, les pilotes 4M/50M et RTLR conservent leurs gates et évoluent indépendamment ; aucune conclusion spectrale ne justifie de fusionner leurs expérimentations ni de modifier leur choix de défaut.

**Critère directeur : la découverte spectrale doit fournir une fonction booléenne que l'ANF/RAW à budget égal ne trouve pas aussi efficacement, puis améliorer la perte de prédiction indépendamment de la complexité ajoutée.**
