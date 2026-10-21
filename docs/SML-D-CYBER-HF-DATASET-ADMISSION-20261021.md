# SML-D CYBER — addendum de pré-admission des datasets cybersécurité Hugging Face, figé le 2026-10-21

Propriétaire scientifique : SML-GENIUS. Exécution hôte : RemoteOps.
Additif à `docs/SML-50M-S1A-OPEN-LICENSE-CORPUS-ADMISSION.md` et à la règle 1 de
`docs/SML-50M-S3B-OPEN-LICENSE-SOURCE-EXPANSION-20261021.md` ; aucune modification
rétroactive des résultats S0/S1b/S2a. Complémentaire de
`docs/SML-50M-S4A-CYBER-DATASET-RIGHTS-PILOT-20261021.md` (corpus texte cyber) et de
`docs/SML-50M-S4C-APACHE-MIT-OPERATOR-REVIEWED-LABELS.md` : le présent addendum couvre
les jeux **tabulaires** étiquetés binaires destinés à `sml-decision`, pas le flux texte.

Les étiquettes S4c `APACHE_2_0_ATTRIBUTION_REVIEWED` et `MIT_ATTRIBUTION_REVIEWED`
**ne s'appliquent pas** à UNSW-NB15 ni aux jeux CIC ci-dessous : leur tag Apache-2.0/MIT
Hugging Face n'est pas une licence accordée par l'éditeur amont (voir le constat ci-dessous).

**Ce document est un protocole figé, pas une admission de données.** Il n'autorise par
lui-même ni téléchargement massif, ni entraînement, ni redistribution, ni publication de
dérivés, et n'atteste aucune clearance juridique. **Toutes les sources ci-dessous restent
`SOURCE_CANDIDATE_ONLY`** tant qu'un reçu de revue opérateur indépendant, propre à
l'éditeur amont et signé, n'a pas été enregistré. Aucune étiquette introduite ici n'est
acceptée par un exécutable du dépôt ; l'échec est fermé.

## Objet

Préparer l'exploitation **massive** (dizaines à centaines de millions d'enregistrements)
de datasets cybersécurité étiquetés binaires pour les têtes de décision `sml-decision`
(`InputRecord` : 64 features booléennes `values` + masque `known`, tête `Binary`), en
fixant à l'avance les conditions d'admission, sans affaiblir la politique de droits du
dépôt.

## Trois niveaux distincts

Pour chaque source, ce document sépare strictement :

1. **Termes de l'éditeur** : ce que l'éditeur amont publie, cité mot pour mot avec sa page.
   Le tag `license` d'un miroir Hugging Face n'en fait pas partie (règle S3b n°1).
2. **Restriction d'usage inférée** : la lecture conservatrice que SML-GENIUS en tire. C'est
   une proposition de ce document, pas un avis juridique.
3. **Admission opérateur signée** : reçu de revue indépendant, propre à l'éditeur, hashé et
   signé. **Aucune source n'a ce reçu à la date de ce document.**

## Niveau 1 — termes de l'éditeur (constat du 2026-10-21)

| Source HF (miroir) | Tag HF | Termes publiés par l'éditeur amont |
| --- | --- | --- |
| [pirocheto/phishing-url](https://huggingface.co/datasets/pirocheto/phishing-url) | CC-BY-4.0 | [Mendeley Data c2gw7fy2j4 v3](https://data.mendeley.com/datasets/c2gw7fy2j4/3) (DOI 10.17632/c2gw7fy2j4.3, Hannousse & Yahiouche) : CC BY 4.0. La conformité du miroir HF à la v3 et les mentions aval restent à auditer. |
| [CIRCL/vulnerability-scores](https://huggingface.co/datasets/CIRCL/vulnerability-scores) | CC-BY-4.0 | Publié par CIRCL lui-même (DOI 10.57967/hf/5927) ; agrège CVE List v5, GitHub Advisories, CSAF CISA / Red Hat / Cisco, dont les termes propres restent à vérifier par sous-source. |
| [rdpahalavan/UNSW-NB15](https://huggingface.co/datasets/rdpahalavan/UNSW-NB15) | Apache-2.0 | [Page projet UNSW](https://research.unsw.edu.au/projects/unsw-nb15-dataset) : « Free use of the UNSW-NB15 dataset for academic research purposes is hereby granted in perpetuity. Use for commercial purposes should be agreed by the authors. » ; citation des articles listés obligatoire. La notice [UNSWorks](https://unsworks.unsw.edu.au/items/4dc0e35c-6196-4c9d-945a-c50b981e5955) porte une formulation plus stricte (« Use for commercial purposes is strictly prohibited ») : les deux formulations coexistent chez l'éditeur, la revue opérateur doit trancher. Le tag Apache-2.0 du miroir n'émane pas de l'éditeur. |
| [auliraff/CIC-IDS-Collection](https://huggingface.co/datasets/auliraff/CIC-IDS-Collection) | Apache-2.0 | [CIC/UNB IDS-2017](https://www.unb.ca/cic/datasets/ids-2017.html) : données « publicly available for researchers », citation de l'article ICISSP 2018 exigée ; aucune licence nommée ni licence générale lisible par machine confirmée. CSE-CIC-IDS2018, CIC-DoS2017 et CIC-DDoS2019 non revus individuellement. Le tag Apache-2.0 du miroir n'émane pas de l'éditeur. |
| [shivamjaisingh/SIMARGL2021-Intrusion-Detection-Systems](https://huggingface.co/datasets/shivamjaisingh/SIMARGL2021-Intrusion-Detection-Systems) | CC-BY-4.0 | Miroir non officiel, carte vide ; termes de l'éditeur RoEduNet-SIMARGL2021 non vérifiés. |
| [init5iv3/network-traffic-detection](https://huggingface.co/datasets/init5iv3/network-traffic-detection) | MIT | Dérivé de BCCC-CSE-CIC-IDS2018 (York University) ; termes de l'éditeur non vérifiés. |

Les étiquettes S4c `APACHE_2_0_ATTRIBUTION_REVIEWED` et `MIT_ATTRIBUTION_REVIEWED` ne
peuvent pas s'appuyer sur ces tags de miroir.

## Niveau 2 — restriction d'usage inférée (proposition, non admise)

| Source | Lecture conservatrice proposée | Statut |
| --- | --- | --- |
| phishing-url | CC BY 4.0 avec attribution, sous réserve d'audit du miroir | `SOURCE_CANDIDATE_ONLY` |
| CIRCL/vulnerability-scores | CC BY 4.0 avec attribution, sous-source par sous-source | `SOURCE_CANDIDATE_ONLY` |
| UNSW-NB15 | Recherche académique seulement ; aucun usage commercial sans accord écrit des auteurs | `SOURCE_CANDIDATE_ONLY` |
| CIC-IDS-Collection | Recherche seulement, par prudence, faute de licence explicite ; **pas** de classe `…_REVIEWED` sur cette base | `SOURCE_CANDIDATE_ONLY` |
| SIMARGL2021, BCCC-CIC-IDS2018 | Aucune tant que l'éditeur n'est pas revu | `PENDING_UPSTREAM_REVIEW` |

Restent **refusés** : jeux sans licence ni carte (Asad1616, MachineEX007, cybersectony,
rr4433, bstee615/diversevul tant que l'amont n'est pas revu), CC-BY-SA
(Mitake/PhishingURLsANDBenignURLs), LGPL (zefang-liu), et tous les jeux SFT/chat cyber.

## Niveau 3 — conditions d'admission opérateur

Une source ne quitte `SOURCE_CANDIDATE_ONLY` que lorsqu'un **reçu de revue signé** existe,
avec : éditeur et page cités, termes copiés verbatim avec date, révision HF et SHA-256 des
fichiers, sous-sources couvertes, obligations d'attribution et restriction retenue. Une
déclaration d'éditeur ou un tag de miroir n'est jamais un reçu.

Pour les sources dont la lecture est « recherche seulement » (UNSW-NB15, CIC), il faut
**en plus**, avant tout entraînement, dans une PR séparée testée :

1. **Chemin d'exécution recherche-seulement distinct** : un mode d'exécution dédié, séparé
   des chemins CC0/CC-BY, avec répertoire hôte séparé
   (`/home/remoteops/sml-corpora/cyber-research-only/`), jamais mélangé avec les corpus
   ouverts.
2. **Propagation de contamination (taint) appliquée par le code** : tout checkpoint,
   banque, tête, Q4, seuil de quantile ou manifeste dérivé, même partiellement, porte une
   classe de droits « recherche seulement ». Elle ne se perd ni par quantification, ni par
   distillation, ni par fusion de banques ; un artefact contaminé ne peut pas initialiser
   un modèle de classe ouverte, et toute publication ou export échoue.
3. **Tests positifs et négatifs** de cette propagation, verts en CI sur le SHA exact.
4. **Citations** exigées par chaque éditeur reproduites dans le manifeste de run et dans
   tout rapport.

Tant que ces conditions ne sont pas remplies : pas de publication de poids, pas d'usage
commercial, pas de redistribution, pas d'entraînement. Le modèle reste research-only et
n'est jamais auto-promu. Les données cyber brutes restent inertes ; aucun accès modèle ou
TEST n'est ouvert par ce document.

## Préflight par source (après reçu de niveau 3, avant tout téléchargement massif)

- Épingler la révision Hub immuable (hash de commit, jamais `main`) et calculer le SHA-256
  de chaque shard Parquet/CSV ; reçu hôte local, statut `SOURCE_VERIFIED_NOT_TRAINED`.
- Budget borné et vérifié avant transfert : UNSW-NB15 se télécharge d'abord par le seul
  sous-ensemble *Network-Flows* ; les sous-ensembles Packet-Fields / Packet-Bytes /
  Payload-Bytes (≈ 419 Go au total) exigent une seconde validation de capacité disque.
- Les payloads de paquets et les scripts/URL malveillants sont des **données**, jamais
  exécutés, jamais résolus (pas de requête DNS/HTTP vers une URL du corpus).
- Données brutes et dérivés ne quittent pas l'hôte ; aucune IP ni payload dans les logs,
  PR ou artefacts publics.

## Conversion vers `InputRecord`

- Lecteur Rust natif en streaming, mémoire bornée, sans charger un jeu entier.
- `inf`, `NaN`, « null », sentinelles `-1` et colonnes mal typées → bit `known = 0`,
  jamais faux implicite.
- IP, ports source, identifiants de flux et horodatages **exclus des features** ; conservés
  uniquement comme clés de groupe pour le split.
- Flags et indicateurs de protocole → 1 bit chacun ; features continues → 2 à 4 bits
  par seuils quantiles **calculés sur TRAIN seulement** ; total ≤ 64 bits, `schema_id`
  figé et haché dans le manifeste.
- Label : `binary_label` (UNSW), `ClassLabel != Benign` (CIC), `status == phishing`
  (phishing-url). Familles d'attaque → tête `Choice` optionnelle, jamais requise.

## Split, déduplication et contrôles

- Split TRAIN/DEV/TEST **par groupe** (capture, jour, hôte, jeu source amont), assigné
  par SHA déterministe ; jamais ligne à ligne.
- Déduplication exacte du vecteur 64 bits + label avant split ; les doublons inter-rôles
  rejettent le run.
- TEST figé et non ouvert avant la fin du protocole ; inclut un test croisé
  entraînement CIC → évaluation UNSW (et inverse) pour mesurer la généralisation hors
  capture.
- Baselines obligatoires sur les mêmes 64 bits : classe majoritaire, régression logistique,
  arbre de décision de profondeur bornée. Métriques : rappel attaque, taux de faux
  positifs, MCC, taux d'abstention, toutes par jeu source.
- DEV/TEST gardent la distribution naturelle ; le rééquilibrage n'affecte que TRAIN.

## Montée en charge

1. Pilote `pirocheto/phishing-url`, une fois son reçu enregistré (≈ 11 k lignes, 2,8 Mo) : valide le convertisseur et
   le manifeste, aucune revendication de qualité.
2. Pilote 1 M lignes CIC-IDS-Collection, puis 9,17 M complets.
3. UNSW-NB15 Network-Flows, puis seulement après mesure de capacité, les sous-ensembles
   paquets.
4. Chaque étape publie : lignes TRAIN/DEV/TEST effectives, lignes rejetées par motif,
   octets lus, temps mur, débit, SHA source et SHA SML/RemoteOps épinglés.

En cas d'échec de vérification de droits, de hash ou de capacité : arrêt **avant
entraînement** avec audit machine-lisible, pas de succès synthétique.
