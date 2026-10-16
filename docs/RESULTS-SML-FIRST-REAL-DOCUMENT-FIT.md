# Premier fitting documentaire SML — résultat physique Dell

Date du run observé : 16 octobre 2026 (UTC). Classe :
**entraînement réel exploratoire de huit portes booléennes SML,
pas GRU-v2, pas démonstration d'un modèle 25B**.

## Exécution et identité

- Plan de contrôle : [RemoteOps PR #450](https://github.com/Memorithm/RemoteOps/pull/450).
- Workflow Dell : [#38866421536](https://github.com/Memorithm/RemoteOps/actions/runs/38866421536), job 116860196640 **SUCCESS**.
- Code SML réellement exécuté : commit exact
  bc75690a5fe89404ca093f9c65e62935a4a1931a
  (révision de la PR SML #578 avant corrections rustfmt ultérieures).
- Programme : crates/sml-core/src/bin/sml-first-document-fit.rs.
- Paramètres préenregistrés : 8 × LatentGate2, 24 époques, LR 0.25,
  2048 positions par document TRAIN et par bit au maximum.
- Vérification réelle : compilé et testé avec Cargo Rust sur Dell non-root,
  exécuté avec enveloppe CPU et 1 GiB d'espace virtuel.
- Artefact de preuve : [#12031158199](https://github.com/Memorithm/RemoteOps/actions/runs/38866421536/artifacts/12031158199)
  (trois TSV et qualification, pas d'octets corpus).

## Mesures brutes tirées du job

| Mesure | Avant | Après |
| --- | ---: | ---: |
| TRAIN bits/octet, somme des huit log-loss binaires | 8.000000000 | 7.238829643 |
| DEV bits/octet, fichiers séparés de TRAIN | 8.000000000 | 7.212278522 |

- DEV : 8650 positions d'octets cibles observées.
- TRAIN : 49 152 observations supervisées *bit-par-bit* au total.
- Poids entraînés : 8 sélecteurs × 16 logits = 128 paramètres,
  et huit codes de tables de vérité.
- Ce score DEV ne provient pas d'un TEST indépendant et ne mesure pas
  la qualité d'un GRU-v2. Pas d'ajustement selon DEV dans le protocole.

Empreintes SHA-256 publiées par le job sur les fichiers réellement générés :

| Artefact | SHA-256 |
| --- | --- |
| first_fit.tsv | 0e1b0af2ac2de5e74e9abadd260ec203ebd97f9625c9b970420a2325d9dd1c7f |
| provenance.tsv | 1544c96c736b336ff89836264dd0de8460166537a4aba3fe133bba02c55bd2f6 |
| weights.tsv | 81749ece05f61a9706f5e28055dd5ef2297af77580d0924a05c95112be3b27b1 |

La baisse TRAIN est observable. La baisse DEV est descriptive et peut
provenir de régularités communes entre les documents du même projet ;
on ne peut pas la convertir en revendication de généralisation.

## Droits et suite

L'utilisateur a demandé de considérer les droits acquis pour démarrer
les essais. **Cette directive est une hypothèse d'autorisation pour
l'expérience, pas une pièce de preuve légale indépendante et encore
moins une approbation fictive de sources tierces.**
Ce fit a utilisé uniquement les documents du dépôt SML-GENIUS,
pas les sept documents historiques candidats de G0-HF2.
Le protocole GRU-v2 reste inchangé :

G0_admitted=false ; gru_v2_training_started=false ;
rights_reviewed_files=0 ; TEST=UNOPENED ;
production_weight_promotion=false.

Prochaine évolution souhaitée : véritable entraînement récurrent
sur un corpus textuel préconstitué autorisé, sous provenance figée
et avec DEV séparé ; ce run de 128 paramètres n'en est pas le substitut.
