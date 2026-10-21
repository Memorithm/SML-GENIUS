# SML S5b — extension à 60 familles candidates, sans contamination des évaluations

Gel du protocole : 2026-10-22, **avant** la modification du registre et de son validateur. Suite du programme S5a, fusionné par SML-GENIUS #657. Aucun téléchargement, aucune modification des checkpoints et aucun accès au TEST de référence.

## Objectif

Porter le registre des familles de sources candidates de **22 à 60**, en élargissant effectivement la couverture de découverte aux mathématiques, preuves, raisonnement, instruction, code, données scientifiques, biomédicales, multilingues et francophones. La diversité annoncée porte sur des sources *inventoriées*, pas sur des octets autorisés ou réellement appris. Les collections peuvent se recouper fortement. Un jeu synthétique peut aussi dépendre des droits et conditions de ses sources, de ses générateurs et des modèles qui l'ont produit.

## Règle d'admission

Chacune des 38 entrées ajoutées reste strictement `SOURCE_CANDIDATE_ONLY` et `PIN_PENDING`. Source et page de conditions HTTPS sont des pointeurs de revue, **pas** une preuve contractuelle ou juridique. Les tags de licence affichés par Hugging Face ne constituent pas une autorisation d'exploitation des contenus tiers sous-jacents.

Une nouvelle voie `EVAL_ONLY` référence les benchmarks dont les tâches/test sets doivent être tenus à l'écart de l'entraînement. Aucun générateur de manifeste TRAIN ne doit accepter `EVAL_ONLY`, `RESEARCH_ONLY_REVIEW`, `MIXED_SOURCE_REVIEW` ou une des autres voies du registre sur la seule base de son étiquette. Les textes NC, SA, web/TDM, code multi-licences ou synthétiques restent séparés et nécessitent examen au niveau de la source et de l'usage; leur existence n'interdit pas d'étudier un chemin de recherche **si** les conditions correspondantes sont satisfaites.

## Protocole des vérifications

1. Le CLI `sml-corpus-source-plan` accepte huit catégories de revue, dont `EVAL_ONLY`. Il rejette étiquettes de droits promues, références non canoniques, URL non HTTPS et doublons d'identifiant ou d'URL. Le test de catalogue exige au moins 60 familles et un jeu de contrôle évaluations seulement.
2. Les 38 nouveaux identifiants ont été recherchés par métadonnées du connecteur Hugging Face le 22 octobre 2026. Les pages de conditions pointent les cartes des jeux publiées quand une chaîne de droits amont n'est pas encore indépendante. L'existence des jeux et leurs tags déclaratifs **ne valent pas** revue d'autorisation. Les versions en amont demeurent à épingler et à vérifier avant toute acquisition.
3. Ajouter ensuite, dans RemoteOps, une acquisition incrémentale **source par source**, sans exécution du contenu et sous quotas Dell, avec reçus SHA-256, provenance auteur/œuvre/session, licences et notices, exclusions PII, regroupement et déduplication inter-familles. Une copie complète locale de ces catalogues n'est ni nécessaire ni souhaitable.
4. Aucun corpus de benchmark `EVAL_ONLY` ne peut nourrir TRAIN/DEV; conserver séparément le TEST SML historique. Le protocole de comparaison reprendra des contrôles inchangés et des comptages d'octets uniques réellement admis. Aucun résultat de qualité ou d'accélération n'est revendiqué ici.

Références de métadonnées : https://huggingface.co/datasets, ainsi que le chemin individuel de chaque source dans `research/corpus/source_catalog_v1.tsv`. Publication des conditions originales, consentements et restrictions à contrôler avant d'autoriser l'usage; ne pas attribuer de licence universelle aux données de crawl ou à la totalité d'une collection mixte.
