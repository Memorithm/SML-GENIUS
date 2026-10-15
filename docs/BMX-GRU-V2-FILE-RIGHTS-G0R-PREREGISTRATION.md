# BMX-GRU-v2 — gel des décisions sur les droits et partitions (G0-R)

**Statut au 15 octobre 2026 : `G0-R_BLOCKED`.**
Le présent document complète le
[préenregistrement GRU-v2](BMX-GRU-V2-PRETRAINING-PREREGISTRATION.md)
et [l'inventaire à neuf sources](BMX-GRU-V2-NINE-SOURCE-FEASIBILITY-EVIDENCE.md).
Les critères scientifiques et l'autorisation d'ouverture TEST sont sous
l'autorité de SML-GENIUS. RemoteOps n'assure que l'exécution et les contrôles
mécaniques ; il ne statue pas sur les droits ni les résultats scientifiques.

## État réel, sans équivalence abusive

- Neuf dépôts candidats, huit familles potentiellement indépendantes.
- Dernier inventaire de fichiers avec contrôle **physique** des objets Git :
  workflow Dell
  [#38735699543](https://github.com/Memorithm/RemoteOps/actions/runs/38735699543),
  19 578 fichiers vérifiés, 95 078 247 octets.
- Un fichier `rust-analyzer` de 513 octets est explicitement écarté
  du roster de fichiers vérifiés, car son checkout (LF) ne correspond
  pas aux octets d'objet Git source (CRLF). Réintégration éventuelle
  seulement via l'objet Git brut, avec preuve distincte.
- La PR [RemoteOps #417](https://github.com/Memorithm/RemoteOps/pull/417)
  a livré `ops/bmx-gru/v2_review_gate.rs`, compilé et testé sur Dell
  Rust 1.89. Ce contrôle structurel **n'a approuvé aucun fichier réel**.
- Aucun document n'a encore un droit de réutilisation vérifié fichier
  par fichier dans une décision scientifique authentifiée et gelée.
  Aucun vrai manifeste TRAIN/VAL/TEST n'est homologué.
  `v2_training_started=false`.

La simple réussite de la publication signée de `main` concerne encore
des preuves de publication, pas des poids GRU-v2 entraînés.

## Contrat des décisions G0-R

Avant de commencer G0 matérialisé, créer un jeu de décisions
**une ligne par fichier** reprenant exactement les identités
`github:owner/repo`, commit SHA40, chemin Git et Git blob, selon
[le schéma RemoteOps](https://github.com/Memorithm/RemoteOps/blob/main/docs/BMX_GRU_V2_FILE_RIGHTS_REVIEW.md).

Les exclusions sont explicites. Une ligne prétendant une revue positive
doit porter une licence identifiée, un identifiant de personne chargée
de la revue, l'empreinte SHA-256 d'un justificatif légal conservé hors
dépôt public et le split prévu. Le programme doit refuser les lignes
manquantes, inattendues, dupliquées ou dont la provenance Git diverge.

**Une chaîne SHA-256 syntaxiquement correcte n'est pas une preuve de
droit**. Le justificatif doit exister, l'identité/compétence du réviseur
doit être confirmée par un dispositif de confiance distinct, et
l'analyse doit respecter les conditions des contenus (notamment prose
MDN en CC-BY-SA, code sous licence potentiellement différente, MPL-2.0,
traductions, contenu tiers, obligations d'attribution et redistribution).
En l'absence de ces preuves, les fichiers concernés restent exclus
de tout jeu autorisé.

## Gel indépendant des partitions

Ne pas ajuster les partitions pour atteindre artificiellement le quota
de 40 Mio : les seuils s'appliquent **après** exclusions et conversion
des octets effectivement consommés.

Réserver avant toute mesure v2 une sélection satisfaisant simultanément :

- un dépôt/une œuvre/un lignage connu affecté à un seul split, même
  s'il fournit plusieurs langues ;
- au minimum deux groupes indépendants par TRAIN, VAL et TEST ;
- TRAIN ≥40 Mio, dont chaque FR/EN/RS ≥2 Mio ;
  VAL et TEST ≥1 Mio chacun ;
- nouvelles sources TEST indépendantes des panels v1/ENG et audit de
  chevauchement rétroactif autant que possible ;
- aucun accès aux octets TEST par l'environnement, les arguments, les
  logs ou le moteur d'entraînement.

La politique de split, l'empreinte du registre, les décisions de
droits, les empreintes des vrais documents et la règle d'acceptation
doivent être gelées **avant l'observation des performances**. Une
allocation arithmétiquement possible des neuf dépôts n'est pas une
allocation scientifique décidée et autorisée.

## Chaîne d'admission suivante, non encore exécutée sur un vrai corpus

1. Vérifier les justificatifs réels de droits, arrêter les inclusions,
   exclusions et splits ; geler leurs empreintes et l'autorité signataire.
2. Matérialiser en privé uniquement les objets Git admissibles
   à révision exacte ; refuser les transformations non déclarées.
3. Contrôler les tailles et SHA-256 bruts et payloads réels avec
   `v2_seal.rs`.
4. Exécuter `v2_manifest.rs --strict`, `v2_lineage.rs` et
   `v2_overlap.rs` ; conserver les résultats et les cas négatifs.
5. Après succès complet de G0 et qualification du moteur/recette,
   seulement alors autoriser G1 best-VAL vs final sur Dell, sous lock
   d'exécution et budgets préenregistrés.

**Verdict documentaire actuel : `NOT_READY_FOR_TRAINING`.**
Cette PR ne change ni le modèle ni les seuils de sélection, et ne
déclare aucune licence autorisée automatiquement.


## G0-R2 — preuve d'octets des reçus de droits (RemoteOps #420)

Une validation de la seule syntaxe de `legal_evidence_sha256` ne suffisait
pas : une déclaration `REVIEWED` pouvait faire référence à un document
inexistant. Ce chemin est désormais bloqué par
[RemoteOps PR #420](https://github.com/Memorithm/RemoteOps/pull/420),
fusionnée sur le commit `a7ed6780535b24446a224ec3e76a24a99fdbc97d`.

Le binaire Rust `v2_review_gate.rs` exige désormais
`--evidence-dir PRIVATE_ROOT` si une seule décision est `REVIEWED`.
Il lit un document privé `<legal_evidence_sha256>.txt` par entrée,
recalcule le SHA-256 sur **les octets lus**, valide un format borné à
16 Kio et vérifie l'identité du dépôt, commit, chemin, objet Git, licence,
réviseur et split. Les reçus altérés, absents, symboliques, d'un autre
document ou en doublon sont rejetés. Les trois champs déclaratifs
`rights_basis`, `attribution_plan` et `third_party_scope` doivent
être renseignés.

Validation réelle de la PR sur Dell, au SHA head
`8b2dfa17f47abe2b8ca885dbd50b8d5c746ca3e0` :

- [admission Rust et tests adversariaux #38743299101](https://github.com/Memorithm/RemoteOps/actions/runs/38743299101) : SUCCESS ;
- [reconstruction du registre candidat #38743299084](https://github.com/Memorithm/RemoteOps/actions/runs/38743299084) : SUCCESS.

**Cette preuve est technique, pas juridique.** Elle confirme que le
justificatif existe et correspond aux déclarations, pas que son auteur
est autorisé ni que ses conclusions sont exactes. Aucun `REVIEWED`
du vrai registre de 19 578 documents n'a été authentifié ni signé
scientifiquement. Un processus d'authentification séparé devra lier
l'identité habilitée, le lot figé de décisions et les reçus validés,
avec conservation privée des originaux et examen des conditions de
licence spécifiques aux fichiers.

Dans l'état courant, le registre de décisions est intégralement
`EXCLUDE`, les octets admis valent **zéro** pour chaque split,
et `G0_admitted=false`. Le protocole interdit de qualifier
le corpus au seul motif que la CI documentaire est verte.

Prochaine condition mesurable : obtenir **un sous-ensemble de reçus
authentifiés et valides**, puis répéter les calculs des tailles et
familles sur ces seuls fichiers sans changer les seuils. Le modèle
GRU-v2 n'a pas reçu l'autorisation d'entraînement.
