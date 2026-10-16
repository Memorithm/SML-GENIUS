# SML-1 — Noyau d'apprentissage par gradient à atlas parcimonieux, paginé sur disque

Date 2026-10-16. **Implementation effective en Rust, sans unsafe ni dépendance supplémentaire** dans `sml-core::sparse_gradient_atlas`. Validation demandée par l'utilisateur : cœur d'apprentissage massif réel et mesurable, pas une banque de compteurs à 8 192 contextes. Cette PR ne prétend pas avoir entraîné 25B.

## Séparation prouvable des dimensions

- **Capacité structurée** : `pages × 1024` poids *entraînables*, physiquement présents dans `master.f32`. Chaque paramètre est chargé/écrit via la page sélectionnée. Le fichier maître est représenté sur disque par des `f32` (4 octets/paramètre), pas une simple description théorique d'un atlas fictif.
- **Poids réellement modifiés au moins une fois** : `distinct_updated_parameters` compté **exactement** via un bitmap `touched.bit` (128 octets/page). `parameter_updates` comptabilise chaque mise à jour de scalaire; ne jamais l'assimiler au nombre de paramètres.
- **Mémoire vivante plafonnée** : cache LRU `max_cached_pages`, chaque page 4 096 octets master + 128 octets bitmap (+ overhead des allocations). Parcours séquentiel du texte; seules les pages routées sont resident en RAM. Sur 4 096 pages, la capacité est **4 194 304 poids** (master **16 Mio**, export Q4 **2 Mio**) ; **le nombre appris est la valeur du bitmap, pas 4 194 304 par définition**.
- **Routage initial non-entraîné** : sélection déterministe de page par contexte de deux octets et index du bit `0..7`. Chaque contexte active au plus 17 features de la page, avec biais, 8 features `a` et 8 features `b`. Ce noyau n'est pas encore une fusion avec MOR appris, ni un ANF profond, ni une récurrence interne.
- **Apprentissage** : perte logistique binaire exacte sur la cible réelle `y`, probabilité `sigmoid(sum_i (weight_i × feature_i)/4)`, gradient `d=(p-y)/4` et descente de gradient stochastique sur les poids sélectionnés. Aucune modification des poids pendant `evaluate_document`. Bornes numériques et vérifications `is_finite`.
- **Inférence 4 bits** : `export_q4` sérialise effectivement chaque poids dans un nibble signé, saturation `[-8,7]`, quantification `round(2 * w)`, sans embarquer les masters `f32`. **Aucune affirmation que cette quantification conserve la perte DEV** ; cela doit être mesuré sur la version compactée et calibré / entraîné sous contrainte QAT plus tard.
- **Checkpoint** : poids maître et bitmap persistés/vidés avec `sync_all`; manifest horodaté par schéma, dimensions, réglage d'entraînement, compteurs et **SHA-256 de chaque fichier**. Reprise stricte sur égalité de configuration et des deux SHA : corruption ou interruption pendant écriture fait échouer la reprise, non reprendre de faux poids. Les checkpoints ne sont pas des snapshots multitâches concurrents.

## Test de bout en bout sur corpus réel

Binaire `sml-sparse-gradient-fit --pilot CORPUS.utf8 provenance.tsv CORPUS_SHA256 OUT_DIR`.

- Ne part pas d'une séquence inventée : sur Dell, le corpus déjà réellement matérialisé est `PleIAs-SEC-10GiB-b09d02e`, 10 737 765 087 octets utiles, SHA-256 `c890b0cd08e9d67d4ea4ca9a88e760282c6de6c1a8f74ebfad6c0d1a2c740ed9`.
- Vérification du SHA-256 du **fichier entier** avant tout apprentissage. Le pipeline ne traite ensuite qu'un préfixe borné : au moins 2 Mio de documents pour TRAIN et 1 Mio d'autres documents pour un **diagnostic indépendant par documents du même lot**. Ce diagnostic n'est PAS le DEV chronologique 2023.
- Lecture de tailles/documents du manifeste de huit colonnes déjà utilisé; SHA-256 par document; refus doublons, licences restrictives inconnues, symlinks et ruptures de frontière; réinitialisation du contexte à chaque document.
- Un vrai `master.f32`, `touched.bit`, `snapshot.tsv`, `weights.q4` et `pilot.tsv` sont produits. Les preuves indiquent capacités, nombre exact d'indices touchés, mises à jour, perte TRAIN et sur documents non vus, poids packés, G0 NOT_ADMITTED, TEST UNOPENED. Aucun texte de rapport publié dans les artefacts GitHub.
- Précondition : droits des rapports d'émetteurs non encore homologués, donc expérimentations seulement; inclure seulement les données déjà utilisées en mode exploratoire avec provenance.

## Gates et risques avant les 25 milliards

Pour reproduire 25B poids dans ce noyau temporairement full-`f32`, il faudrait **~100 Go de masters sur disque**, **~3,125 Go de bitmaps**, et un export 4 bits **~12,5 Go**, pas l'architecture SML de 3,906 Go. Le test synthétique de capacité ne crée pas ces fichiers et ne démontre aucune qualité.

Notre architecture 25B finale demeure **21,25B booléens, 2,50B ternaires, 1,25B résiduels Q4**, soit 31,25 milliards de bits = 3 906 250 000 octets de poids, hors index. La structure d'optimisation doit à terme gérer les trois classes de représentation, les poids maîtres optionnels, le routage MOR entraîné, des circuits ANF et la récurrence, sans fausse déclaration de paramètres.

**Gates immédiats** : tests SML sur convergence de perte connue, reprise strictement identique, corruption rejetée, cache RAM plafonné, 4M poids logiquement/physiquement alloués sur un fichier réel, `distinct_updated_parameters` inférieur à la capacité ; exécution sur Dell sur préfixe d'un vrai corpus financier. **Gates suivants :** DEV 2023 chronologique hors entraînement, comparaison de Q4 et F32, débit et latence sur Thor, montée 50M → 300M sous mesures constantes, intégration réelle ANF + MOR et contrôle des dépenses SSD. Aucune performance financière ou 25B certifiée à ce stade.

Le nombre de paramètres **entraînables** n'est pas le nombre de paramètres **effectivement appris** ; les deux sont audités séparément.
