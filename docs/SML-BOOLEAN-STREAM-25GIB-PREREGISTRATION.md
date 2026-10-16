# SML-GENIUS Boolean Context Stream V1 — 25 Gio cible

Date : 16 octobre 2026. Prérequis : les PR SML-GENIUS #579/#580 (apprentissages 11,78 Mio / 19,86 Mio), RemoteOps #454/#456 (PleIAs Common Corpus, 500 Mio de texte source).

## Objectif et unité

**Capacité architecturale : au minimum 25 Gio de texte, avec une limite de sécurité explicite de 32 Gio** (`34 359 738 368` octets TRAIN, hors métadonnées). Il s'agit de **volume de données**, pas de 25 milliards de paramètres. Le modèle demeure une banque de 8 192 contextes booléens de fréquence conditionnelle, `[u64; 2]`, lissage Laplace `alpha=1`. Aucune revendication d'ANF récurrente ou de GRU-v2.

Mode supplémentaire de l'exécutable actuel : 

```sh
sml-boolean-large-corpus-fit --stream   CORPUS.utf8 provenance.tsv CORPUS_SHA256_HEX   FROZEN_DEV_REPO NEW_OR_RESUMABLE_OUTPUT_DIR
```

Les modes antérieurs `SCIRUST TDI FLAT CCOS DEV OUTPUT` et `--multi REPO ROOT ... DEV OUTPUT` restent intacts. Ils ont leurs propres limites et preuves : la nouvelle capacité 32 Gio concerne **uniquement `--stream`**.

## Schéma de données

- Le texte source est un fichier `corpus.utf8` constitué de documents concaténés, avec **exactement un séparateur `\n` par document**. Le compteur de taille TRAIN **exclut** ces séparateurs.
- Le manifeste `provenance.tsv` suit les huit colonnes du constructeur RemoteOps HF : `shard,row,sha256,bytes,license_class,license_source,identifier,creator_title`.
- Chaque document doit comporter 128 à 8 388 608 octets et un hash SHA-256. Les classes d'accès possibles pour l'expérimentation sont `PD_CLAIM`, `CC0`, `CC_BY_ATTRIBUTION`, avec origine et licence initiale non vides. Les revendications de domaine public sont **non homologuées juridiquement**.
- Une lecture préalable valide le manifeste, son SHA, les bornes et la taille physique exacte ; une lecture complète valide le SHA-256 du texte contre la valeur **imposée par l'appelant**. Le hash de chaque document est également contrôlé pendant le fit. Les entrées malformées, symlinks d'entrée et dépassements provoquent un échec.
- La mémoire dédiée à la source est bornée par une fenêtre de lecture d'1 Mio + 2 octets de contexte, **sans charger le corpus complet**. Le manifeste est parcouru ligne par ligne avec un plafond de 256 Kio par enregistrement. Les structures persistantes du modèle occupent 8 192 × 16 octets, plus DEV, sans tableau de tous les documents TRAIN en mémoire. Les plus de 25 Gio ne sont pas une allocation RAM.
- Chaque frontière documentaire réinitialise les deux octets de contexte ; les séparateurs ne sont ni appris ni évalués. Les fenêtres qui traversent une frontière de lecture *dans un document* restent continues.

## Reprise et sûreté

- Sauvegarde du modèle, offsets documentaires, octets et événements toutes les **256 pièces**, via fichier temporaire puis renommage atomique `checkpoint.v1`. `u64` avec addition vérifiée pour compteurs, événements, offsets.
- Une reprise ne reprend qu'avec le **même SHA corpus** et **même SHA manifeste** et recalcule l'intégrité du corpus, la longueur physique et les offsets/événements du préfixe ; elle ne rejoue jamais des documents déjà enregistrés au checkpoint. Si le checkpoint est incompatible, l'exécution échoue.
- Le corpus TRAIN est comparé aux SHA-256 des fichiers d'un DEV séparé : le chevauchement **exact** rejette le run. Cette vérification n'élimine pas tout chevauchement sémantique ; pas de publication de score TEST.
- Seuls `weights.tsv`, `summary.tsv`, `qualification.tsv` et le checkpoint sont générés localement ; jamais les octets bruts sur GitHub. Le score est une perte de prédiction du bit en bits par octet, calculée avec la banque figée sur un DEV différent.
- Une fois le `summary.tsv` complet présent, relancer le fit sur le même répertoire est refusé.

## Expérience initiale prévue

- Source TRAIN : `PleIAs/common_corpus@307910e4c5d040d6f318e6edf2a2b97849155771`, corpus persistant sur Dell `/home/remoteops/sml-corpora/PleIAs-CommonCorpus-500MiB-307910e4`.
- SHA du corpus : `684aa9f2bd96055ea4c95ad450533a4d4da888a30b3e6e4e49f3afe70e68eed8`.
- Volume revendiqué et physiquement validé : 524 332 346 octets textuels, 28 294 documents (18 655 déclarés domaine public, 6 974 CC0, 2 665 CC-BY avec attribution) ; 59 identifiants longs réparés dans `full_identifiers.tsv` par RemoteOps #456.
- DEV : `Memorithm/SML-GENIUS@566faf5feffb8dcabb0f0955ef390cf73a29a939`, gelé, séparé et jamais utilisé pour mettre à jour la banque.
- Vérifier le cas 500 Mio, les tests de continuité entre chunks, la reprise sur corruption/coupure, et le dépassement de capacité simulé sans allouer 25 Gio. Le run physique 25 Gio réel reste un **jalon futur** dépendant d'un corpus plus volumineux autorisé, de l'espace disque et du temps de calcul sur machine.

**Frontière** : `G0_admitted=false`, `TEST=UNOPENED`, modèle probabiliste booléen de référence. 25 Gio d'entraînement ne signifie ni un modèle 25B ni une garantie de performances supérieures. Les obligations d'attribution CC-BY demeurent applicables.

## Résultat du premier fit en streaming (non homologué)

L'exécution Dell [RemoteOps Actions #38913477655](https://github.com/Memorithm/RemoteOps/actions/runs/38913477655) a validé le binaire au SHA `355b52ceda1b71e869b3641810157c61c1e762cb` (avant ajout du test de reprise). Résultats physiques :

- TRAIN : **524 332 346 octets** utiles, **28 294 documents**, **524 275 758** positions de prédiction (`sum(max(0, len-2))`).
- Banque : **3 249 / 8 192 cellules** apprises ; DEV gelé : **5 862 247 octets**, **5 861 191 positions**, **5.995993233 bits/octet**.
- Capacité annoncée par le nouveau binaire : **34 359 738 368 octets (32 Gio)** de payload TRAIN, suffisante pour une cible de 25 Gio. Ce plafond est le résultat de bornes, types et tests ; il **n'est pas la preuve d'un run de 25 Gio**.
- Sorties : `checkpoint.v1` atomique, `summary.tsv`, `weights.tsv`, `qualification.tsv` et empreintes dans l'artefact GitHub Actions, sans texte brut.
- `TEST=UNOPENED` ; `G0=NOT_ADMITTED`.

Attention : la perte DEV de 5.99599 bpc est mesurée après un entraînement sur des textes Hugging Face hétérogènes, évalué sur le code/documentation de SML. Elle n'est **pas directement comparable en validité externe** aux campagnes à 4/14 dépôts Memorithm, et ne constitue ni une preuve d'ANF récurrente, ni un gain universel.

La qualification finale exige la CI verte sur la dernière révision de la PR, une requalification Dell sur ce même commit et la vérification du checkpoint. Les longues campagnes 25 Gio requièrent un vrai corpus éligible de 25 Gio, des évaluations indépendantes par source et une protection de crash/reprise démontrée.
