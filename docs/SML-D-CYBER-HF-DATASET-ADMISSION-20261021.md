# SML-D CYBER — addendum d'admission des datasets cybersécurité Hugging Face, figé le 2026-10-21

Propriétaire scientifique : SML-GENIUS. Exécution hôte : RemoteOps.
Additif à `docs/SML-50M-S1A-OPEN-LICENSE-CORPUS-ADMISSION.md` et à la règle 1 de
`docs/SML-50M-S3B-OPEN-LICENSE-SOURCE-EXPANSION-20261021.md` ; aucune modification
rétroactive des résultats S0/S1b/S2a.

**Ce document est un protocole d'admission figé, pas l'affirmation qu'un téléchargement
ou un entraînement a eu lieu.** Statut initial de chaque source : `SOURCE_CANDIDATE_ONLY`.
Ajouter une étiquette ici n'atteste pas une clearance juridique ; la revue indépendante
reste obligatoire et l'échec est fermé.

## Objet

Autoriser l'exploitation **massive** (dizaines à centaines de millions d'enregistrements)
de datasets cybersécurité étiquetés binaires pour les têtes de décision `sml-decision`
(`InputRecord` : 64 features booléennes `values` + masque `known`, tête `Binary`), sans
affaiblir la politique de droits du dépôt.

## Constat de licence (revue du 2026-10-21)

Le tag `license` d'un dépôt Hugging Face n'attestant pas les droits amont (règle S3b n°1),
chaque source a été revérifiée chez son éditeur d'origine :

| Source HF | Tag HF | Termes amont constatés | Classe retenue |
| --- | --- | --- | --- |
| [pirocheto/phishing-url](https://huggingface.co/datasets/pirocheto/phishing-url) | CC-BY-4.0 | [Mendeley Data c2gw7fy2j4 v3](https://data.mendeley.com/datasets/c2gw7fy2j4/3), DOI 10.17632/c2gw7fy2j4.3, **CC BY 4.0** (Hannousse & Yahiouche) | `CC_BY_ATTRIBUTION_REVIEWED` |
| [CIRCL/vulnerability-scores](https://huggingface.co/datasets/CIRCL/vulnerability-scores) | CC-BY-4.0 | Publié par CIRCL lui-même, DOI 10.57967/hf/5927 ; agrège CVE List v5, GitHub Advisories, CSAF CISA/Red Hat/Cisco | `CC_BY_ATTRIBUTION_REVIEWED` **par sous-source** après vérification ligne `source` |
| [shivamjaisingh/SIMARGL2021-Intrusion-Detection-Systems](https://huggingface.co/datasets/shivamjaisingh/SIMARGL2021-Intrusion-Detection-Systems) | CC-BY-4.0 | Miroir non officiel, carte vide ; source amont RoEduNet-SIMARGL2021 (Sensors 2021) non encore vérifiée | `PENDING_UPSTREAM_REVIEW` (non admis) |
| [rdpahalavan/UNSW-NB15](https://huggingface.co/datasets/rdpahalavan/UNSW-NB15) | Apache-2.0 | [UNSW](https://research.unsw.edu.au/node/134656) / [UNSWorks](https://unsworks.unsw.edu.au/items/4dc0e35c-6196-4c9d-945a-c50b981e5955) : « Free use … for academic research purposes … in perpetuity » ; « Use for commercial purposes is strictly prohibited » ; citation des cinq articles obligatoire. **Le tag Apache-2.0 du miroir est incompatible avec l'amont.** | `ACADEMIC_RESEARCH_ONLY_REVIEWED` |
| [auliraff/CIC-IDS-Collection](https://huggingface.co/datasets/auliraff/CIC-IDS-Collection) | Apache-2.0 | [CIC/UNB IDS-2017](https://www.unb.ca/cic/datasets/ids-2017.html) : « publicly available for researchers », citation obligatoire, aucune licence nommée ; même régime pour CSE-CIC-IDS2018, CIC-DoS2017, CIC-DDoS2019 | `ACADEMIC_RESEARCH_ONLY_REVIEWED` |
| [init5iv3/network-traffic-detection](https://huggingface.co/datasets/init5iv3/network-traffic-detection) | MIT | Dérivé de BCCC-CSE-CIC-IDS2018 (York University) ; termes amont non vérifiés | `PENDING_UPSTREAM_REVIEW` (non admis) |

Restent **refusés** : jeux sans licence ni carte (Asad1616, MachineEX007, cybersectony,
rr4433, bstee615/diversevul tant que l'amont n'est pas revu), CC-BY-SA
(Mitake/PhishingURLsANDBenignURLs), LGPL (zefang-liu), et tous les jeux SFT/chat cyber.

## Nouvelle classe : `ACADEMIC_RESEARCH_ONLY_REVIEWED`

Les deux plus gros gisements (UNSW-NB15 ≈ 406 M lignes, CIC-IDS-Collection ≈ 9,17 M lignes)
ne sont ni CC0 ni CC-BY. Leur exploitation massive est autorisée **uniquement** sous cette
classe, avec les conséquences suivantes, non négociables :

1. **Contamination des artefacts.** Tout checkpoint, banque, tête, Q4, statistique de
   seuil ou manifeste dérivé, même partiellement, d'une source de cette classe porte
   `rights_class = ACADEMIC_RESEARCH_ONLY` et hérite de la restriction. Elle ne se
   « lave » ni par quantification, ni par distillation, ni par fusion de banques.
2. **Pas de publication de poids ni d'usage commercial**, y compris intégration dans un
   produit, une API ou un service, sans accord écrit des auteurs (UNSW : Moustafa & Slay ;
   CIC : Canadian Institute for Cybersecurity), archivé hors dépôt public avant toute levée.
3. **Ségrégation physique.** Ces données et leurs dérivés vivent dans un répertoire hôte
   distinct (`/home/remoteops/sml-corpora/cyber-research-only/`), jamais mélangé avec les
   corpus CC0/CC-BY ; aucun checkpoint de cette classe ne sert d'initialisation à un
   modèle de classe ouverte.
4. **Citation.** Les articles exigés par chaque éditeur sont listés dans le manifeste de
   run et reproduits dans tout rapport de résultats.
5. **Recherche seulement.** Conforme au statut actuel du dépôt : modèle research-only,
   jamais auto-promu.

La classe CC-BY (`pirocheto/phishing-url`, CIRCL) n'a pas ces restrictions mais conserve
les obligations S1a : `identifier`, `creator`, `title`, `license_source` et attribution
à la diffusion.

## Préflight par source (avant tout téléchargement massif)

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

1. Pilote `pirocheto/phishing-url` (≈ 11 k lignes, 2,8 Mo) : valide le convertisseur et
   le manifeste, aucune revendication de qualité.
2. Pilote 1 M lignes CIC-IDS-Collection, puis 9,17 M complets.
3. UNSW-NB15 Network-Flows, puis seulement après mesure de capacité, les sous-ensembles
   paquets.
4. Chaque étape publie : lignes TRAIN/DEV/TEST effectives, lignes rejetées par motif,
   octets lus, temps mur, débit, SHA source et SHA SML/RemoteOps épinglés.

En cas d'échec de vérification de droits, de hash ou de capacité : arrêt **avant
entraînement** avec audit machine-lisible, pas de succès synthétique.
