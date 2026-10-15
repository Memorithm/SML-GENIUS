# BMX-GRU-v2 — G0-R3 : triage documentaire mesuré et signatures de lots

Date d'observation : **15 octobre 2026**. État scientifique :
**`G0_admitted=false`**, aucun corpus TRAIN/VAL/TEST réellement
homologué, aucun entraînement GRU-v2 lancé.

L'autorité scientifique reste SML-GENIUS. RemoteOps exécute et
contrôle les preuves sur Dell mais ne décide ni des droits d'usage ni
de l'indépendance scientifique des partitions.

## 1. Résultat de la sonde documentaire sur de vrais fichiers Git

Le workflow RemoteOps
[`BMX GRU-v2 pinned source volume audit` #38744723608](https://github.com/Memorithm/RemoteOps/actions/runs/38744723608)
a terminé **SUCCESS** sur la PR
[RemoteOps #422](https://github.com/Memorithm/RemoteOps/pull/422).
Il a parcouru les **19 578 documents candidats** déjà vérifiés
contre les objets Git de neuf dépôts publics à commits immuables.

| Indice recherché dans les premiers 4 096 octets | Fichiers signalés |
| --- | ---: |
| Marqueur `SPDX-License-Identifier:` | 0 |
| Mention `copyright` ou symbole © | 23 |
| Terme `license` ou `licence` | 51 |
| Mention `third party`, `third-party`, `third_party` ou `vendored` | 110 |
| Mention `CC-BY-SA` ou `Creative Commons Attribution-ShareAlike` | 2 |
| Fichiers non UTF-8 | 0 |

Les indicateurs se chevauchent et ne sont donc **pas** additionnables
pour produire un nombre de documents concernés par le droit d'auteur.
L'absence de marqueurs dans les premiers 4 096 octets ne signifie pas
absence de licence, de copyright ni de tiers. Les notices dans
`LICENSE`, `README` ou dans les sous-projets peuvent régir des
fichiers ne contenant aucune mention intrinsèque ; les extraits
intégrés peuvent aussi avoir un régime distinct.

SHA-256 réel du TSV de triage :
`00d904305f6ea52227709d84bcd28e21b2c1a0da605a69e283b4f0430da90b72`.
Artefact :
[`bmx-gru-v2-license-notice-triage`, ID 11978106109](https://github.com/Memorithm/RemoteOps/actions/runs/38744723608/artifacts/11978106109).
Il contient les métadonnées par fichier et marqueurs binaires,
**pas les contenus des œuvres**.

## 2. Contrôle mécanique de la décision d'usage

RemoteOps [PR #417](https://github.com/Memorithm/RemoteOps/pull/417)
exige une décision explicite par fichier du registre, et
[PR #419](https://github.com/Memorithm/RemoteOps/pull/419)
a généré sur Dell un TSV déterministe initial contenant
**19 578 lignes `EXCLUDE` et zéro `REVIEWED`**.

SHA-256 du registre initial des décisions :
`5a86b3693a906afd5d3d98c67cd36587c4cb9b7323ce4a7d29b21e25056ddb0a`.
Il ne représente **aucune autorisation** ; les volumes utilisables
par TRAIN/VAL/TEST restent tous nuls.

RemoteOps [PR #420](https://github.com/Memorithm/RemoteOps/pull/420)
oblige une décision `REVIEWED` éventuelle à pointer vers les
**octets réels** d'un reçu de droits privé, dont le SHA-256 est recalculé
et dont l'identité Git, la licence, le réviseur et le split doivent
correspondre exactement au TSV.

## 3. Nouvelle protection contre les modifications et l'usurpation de lot

RemoteOps [PR #424](https://github.com/Memorithm/RemoteOps/pull/424),
fusionnée sur le commit
`9b1ab5b44b5b0caa373dbf8a8efd9654da9f7306`, ajoute
`ops/bmx-gru/v2_review_authority.rs`.

Le message signé comporte :

- un identifiant de protocole de revue G0 ;
- le SHA-256 réel des **octets du registre** de provenance ;
- le SHA-256 réel des **octets des décisions** ;
- le principal exact du réviseur, déclaré pour chaque `REVIEWED` ;
- le nombre de décisions positives et la mention
  `G0_admitted=false`.

La vérification utilise OpenSSH `ssh-keygen -Y verify` avec un
namespace spécifique
`memorithm-gru-v2-rights-v1` et une liste de
clés publiques de revue établie **indépendamment** de la signature
des poids en production. Elle refuse les signatures altérées,
les octets supplémentaires après l'armure, une clé non autorisée,
un principal différent ou un TSV modifié.

Les tests positifs/négatifs ont réussi sur Dell, workflow
[#38753284116](https://github.com/Memorithm/RemoteOps/actions/runs/38753284116),
au commit exact
`1c2b203d600812207bc242c2dc4bc01638764aa9`.
Les clés de tests sont éphémères, synthétiques et détruites ;
**aucune clé d'authentification réelle n'a été provisionnée**.

## 4. Frontière de confiance avant G0

La possession d'une clé acceptée prouve seulement la validité d'une
signature **par rapport à la liste de confiance fournie**. La
compétence de son titulaire et sa délégation juridique doivent être
vérifiées indépendamment. La signature est liée aux fichiers de
décisions mais ne démontre pas que le contenu des justificatifs
décrit honnêtement les conditions de réutilisation.

La séquence préalable à toute déclaration G0 positive est :

1. Vérifier les conditions de licence par œuvre et document,
   avec revue des contributions tierces, traductions, code et prose.
   Conserver les justificatifs privés et vérifier les titulaires.
2. Désigner et authentifier l'autorité habilitée à approuver,
   en configurant une liste de clés publiques **distincte de la clé
   de signature des poids**. Ne jamais publier la clé privée.
3. Finaliser les décisions `REVIEWED` ou `EXCLUDE` par fichier,
   vérifier chaque reçu au niveau des octets, et signer le **lot exact**
   de décisions et le registre source gelé.
4. Auditer les groupes de lignage et définir les partitions
   indépendantes avant tout score TEST. Vérifier volumes réels
   retenus après exclusions : TRAIN ≥40 Mio, ≥2 Mio FR/EN/RS,
   VAL et TEST ≥1 Mio, et ≥2 groupes indépendants par split.
5. Matérialiser les octets dans un stockage privé, contrôler leurs
   blobs Git, SHA-256, intersections et tailles avec les quatre
   validateurs Rust de corpus G0. Autoriser G1 seulement après
   preuve complète d'admission.

**Aucune de ces étapes de droits réels et d'authentification de
l'autorité n'est actuellement considérée comme accomplie.**
Le succès technique des CI et l'existence de 95 Mio d'octets
candidats ne constituent pas un corpus licite ni une preuve de
supériorité du GRU-v2.

`rights_reviewed_files=0`, `G0_admitted=false`,
`gru_v2_training_started=false`.


## G0-R4 — contrôle atomique d'exhaustivité avant une signature de revue

L'analyse de la procédure G0-R3 a identifié un défaut d'articulation :
le programme d'attestation pouvait produire un message signable même
si le fichier des décisions comportait une ligne de source **absente**
du registre Git, ou s'il omettait un candidat enregistré. Une
signature SSH correcte ne démontre pas à elle seule la concordance
des documents déclarés et des sources réellement inventoriées.

Cette lacune est corrigée et validée par
[RemoteOps PR #432](https://github.com/Memorithm/RemoteOps/pull/432),
fusionnée sur le commit
`bc2fbaac09c33f97c1726419dfc04c453db1d767`.
Le
[workflow Dell #38782362739](https://github.com/Memorithm/RemoteOps/actions/runs/38782362739)
est **SUCCESS** sur le commit exact
`f835fb53b89b5b79d49be49a34b6a3a1d10c5698`.

Le contrôleur Rust `v2_review_authority.rs` refuse maintenant de
créer ou confirmer une attestation si :

- le registre Git contient une provenance erronée, URI mutable,
  chemin invalide, identité en double ou taille hors contrat ;
- les décisions omettent un fichier, comportent une identité
  inventée, ou possèdent des doublons ;
- les documents d'un même dépôt ou d'une famille de traductions
  connues traversent les splits, ou la décision TEST recouvre
  les sources historiquement exposées connues ;
- un justificatif de droits est réutilisé entre plusieurs
  fichiers ou n'existe pas physiquement sous son SHA-256 attendu.

**Changement d'interface non rétrocompatible :**
`--message` ET `--verify` exigent désormais le répertoire local
des reçus privés `PRIVATE_RECEIPTS_DIR`. Ce répertoire est contrôlé
pendant la même invocation Rust qui calcule le message canonique
et vérifie la signature. L'ancien appel sans les reçus est rejeté.
Les fixtures Dell comportent un reçu synthétique et une clé éphémère
détruits après les tests ; aucun véritable droit n'a été accordé.

Le même contrôleur maintient `G0_admitted=false`, même après
validation des signatures et des octets des reçus. Il reste obligatoire
de faire vérifier l'autorité légale réelle, la politique de
réutilisation des œuvres, les exclusions, et l'absence de
contamination des nouvelles partitions TEST avant l'apprentissage.
**Les 19 578 décisions réelles restent EXCLUDE, les volumes
TRAIN/VAL/TEST autorisés restent à zéro.**
