# BMX-GRU-v2 — capacité multi-domaine et familles indépendantes (2026-10-15)

**Statut : preuve de capacité candidate, PAS admission du corpus G0.**
Ce constat complète le préenregistrement
[`BMX-GRU-V2-PRETRAINING-PREREGISTRATION.md`](BMX-GRU-V2-PRETRAINING-PREREGISTRATION.md).
RemoteOps reste le plan de contrôle de la mesure sur Dell, SML-GENIUS
possède les règles scientifiques et la décision de promotion des poids.

## Résultat observé sur Dell

Le workflow RemoteOps
[`BMX GRU-v2 pinned source volume audit` #38734309182](https://github.com/Memorithm/RemoteOps/actions/runs/38734309182)
a été exécuté et terminé **SUCCESS** sur le SHA
`7ffc9ab59736513a49289ee529856e3eb44be5f9`
(PR RemoteOps [#415](https://github.com/Memorithm/RemoteOps/pull/415)).

| Domaine | Dépôt | Octets candidats réels |
| --- | --- | ---: |
| FR | `mdn/translated-content` | 30 440 038 |
| EN | `mdn/content` | 47 063 854 |
| RS | `rust-lang/cargo` | 753 688 |
| RS | `rust-lang/rustfmt` | 1 222 212 |
| RS | `rust-lang/rustlings` | 87 916 |
| RS | `rust-lang/mdBook` | 408 444 |
| EN | `rust-lang/rust-by-example` | 391 468 |
| FR | `Jimskapt/rust-book-fr` | 2 312 876 |
| RS | `rust-lang/rust-analyzer` | 13 373 139 |
| **Total** | **9 dépôts** | **96 053 635** |

Comptage par domaine : FR **32 752 914** ; EN **47 455 322** ;
Rust **15 845 399** octets. L'inventaire des métadonnées de chaque
fichier a un SHA-256 de
`de3d5ea3a09f6f6b2abfd8fa0181b1249997296e1ff0c1510f233b7fd9cc253b`.
Le TSV de volumes a un SHA-256 de
`13fa8505c1dada373d1e9060a62c64768a85d7d32662db59be3bdaf5c93a3ea2`.

Les archives de provenance sont :
[volume ID 11971484887](https://github.com/Memorithm/RemoteOps/actions/runs/38734309182/artifacts/11971484887)
et [roster de fichiers ID 11971229713](https://github.com/Memorithm/RemoteOps/actions/runs/38734309182/artifacts/11971229713).
Le registre n'expose pas les octets des documents, seulement les
chemins, Git blobs, commits et tailles.

## Un problème structurel corrigé, et ce qu'il ne démontre pas

Le premier relevé à six dépôts comportait une paire MDN FR/EN
liée par traduction. Il offrait donc **cinq familles indépendantes
connues**, pas six, alors que le protocole exige deux groupes par
split sur TRAIN/VAL/TEST. Les 79 Mo candidats étaient donc une
preuve de *volume*, mais pas même de nombre suffisant de familles.

Le précontrôle Rust fusionné par RemoteOps PR #415
(`ops/bmx-gru/v2_family_preflight.rs`) refuse ce cas
malgré les volumes globaux suffisants. Avec les trois nouveaux
dépôts, le workflow a mesuré **huit familles potentiellement
indépendantes** en regroupant les relations de traduction connues.
Cette valeur est une **borne de conception**, pas une preuve
de non-chevauchement des contenus ou des lignages inconnus.

À titre de faisabilité arithmétique uniquement, et **sans affecter
effectivement aucun document**, les volumes ci-dessus admettent
une distribution hypothétique :

- Un ensemble TRAIN composé des deux MDN liés (une famille), cargo,
  rust-analyzer et mdBook dépasserait les 40 Mio totaux et
  les 2 Mio dans chacun des trois domaines.
- Deux groupes distincts rustfmt et rustlings totaliseraient
  **1 310 128 octets candidats**, assez pour une partition VAL
  de 1 Mio avant exclusions.
- Deux groupes distincts `rust-book-fr` et `rust-by-example`
  totaliseraient **2 704 344 octets candidats**, assez pour une
  partition TEST de 1 Mio avant exclusions.

**Cet exemple n'est NI un plan de split approuvé, NI un
préenregistrement TEST, NI un catalogue autorisé**. Les exclusions
de licences, chevauchements et transformations peuvent diminuer
ces tailles jusqu'à rendre ce schéma inutilisable. De plus,
un ensemble de sources déjà publié ou potentiellement vu dans
les données de GRU-v1 ne suffit pas à constituer un TEST v2
historiquement indépendant.

## Verrous G0 à lever avant le GPU

1. Revue de licence *au niveau de chaque fichier* et décisions
   d'usage vérifiables (prose MDN CC-BY-SA, éventuel code sous
   autre licence, documentation Rust, traductions et contributions
   externes). Une licence racine observée n'est pas un accord
   implicite pour chaque élément.
2. Gel de la sélection documentaire exacte, des groupes de lignage
   et du choix TRAIN/VAL/TEST sur un commit scientifique,
   AVANT scores de performance. Vérifier que les documents du
   TEST n'étaient pas présents dans l'historique de préentraînement
   connu, ce qui demeure impossible à certifier rétroactivement
   pour certaines sources v1.
3. Matérialisation privée des octets approuvés, correspondance
   des objets Git source et SHA-256 brut/payload, déduplication
   par document, voisinage et passages communs (dont traductions).
4. Exécution et archivage des contrôles Rust G0 :
   `v2_seal.rs`, `v2_manifest.rs --strict`,
   `v2_lineage.rs` et `v2_overlap.rs`.
   Vérifier les seuils sur les octets définitivement retenus,
   **pas** sur les volumes des dépôts.

Le reste du protocole ne change pas : GRU-v1 figé comme contrôle,
première ablation best-VAL contre final au même entraînement,
aucune consultation du TEST à des fins de réglage, aucune
publication des véritables poids sans preuve G3 distincte.

**Conclusion : `source_capacity_potential=true`,
`independent_family_count=8`, mais `G0_admitted=false`,
`v2_training_started=false`, `model_superiority_unproven=true`.**
