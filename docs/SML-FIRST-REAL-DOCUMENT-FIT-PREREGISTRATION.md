# SML First Document Fit — préenregistrement du pilote natif, 16 octobre 2026

Statut : **pilote expérimental, pas GRU-v2, pas G0**.
Autorité scientifique : SML-GENIUS. Exécution Dell : RemoteOps.
La décision du propriétaire permet de lancer un essai exploratoire sur
les textes du dépôt Memorithm ; elle ne crée pas de droits patrimoniaux
sur des œuvres tierces ni de signature indépendante de droits.
Les corpus Gutenberg, Gallica et USPTO restent hors de cet essai.

## Objet

Effectuer un vrai fitting avec mise à jour de paramètres, en Rust natif,
sur des documents réels du projet, via les huit LatentGate2 existants.
Pour chaque bit du prochain octet, apprendre une fonction booléenne à
deux entrées : les bits homologues des deux octets précédents.
La sortie est huit tables de vérité et 8 × 16 logits latents :
**ce n'est pas un GRU, ni un modèle de langage, ni 25 milliards de paramètres**.

## Corpus exact et frontière

Le checkout du commit SML-GENIUS exact par RemoteOps garantit les sources.
TRAIN (ordre figé) : README.md ; BOOTSTRAP.md ; docs/ROADMAP.md.
DEV (jamais optimisé) : docs/SML-0-SPEC.md ;
docs/SML-ARCHITECTURE-DECISION-GATES.md.
Aucun TEST, aucune source externe, réseau ou donnée privée.
Fichiers réguliers non symlink, 128–262144 octets chacun. Taille
et SHA-256 bruts par document dans provenance.tsv.

## Recette préenregistrée

- Modèle : 8 × LatentGate2 ; logits initialisés à zéro.
- Entrées pour le bit b : bit b des octets aux positions t−2 et t−1.
  Cible : bit b à t.
- Jusqu'à 2048 échantillons déterministes par document TRAIN et bit,
  répartis via stride entier.
- 24 époques par bit, learning rate 0.25, epsilon 1e-6, aucun réglage DEV.
- Score avant et après : somme des log-loss binaires des huit bits
  rapportée aux octets ; TRAIN et DEV distincts ; pas d'accès TEST.
- Preuves : provenance.tsv, first_fit.tsv et weights.tsv ; SHA-256,
  aucun texte brut. Les métriques DEV restent publiées même si dégradées.
- Dell non-root, CPU uniquement, workflow borné, concurrence exclusive,
  aucun téléchargement de dépendance, pas de secrets.
- Réussite : job réellement terminé success, perte TRAIN diminuée,
  poids observables et preuves datées du commit exact.
- Pas de généralisation de ce score aux capacités GRU-v2 et 25B.
  Le circuit de pilotage est candidat à la réutilisation scientifique
  seulement après qualification, sans contournement des gardes G0.

G0_admitted=false ; gru_v2_training_started=false ;
rights_reviewed_files=0 ; test=UNOPENED.
