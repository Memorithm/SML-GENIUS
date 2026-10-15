# BMX-GRU-v2 — inventaire de sources épinglées G0 (2026-10-15)

**Statut : candidats vérifiés au niveau du dépôt ; aucun document n'est encore admis comme donnée TRAIN, VAL ou TEST.** Le présent inventaire précède le premier fit v2, conformément à
[`BMX-GRU-V2-PRETRAINING-PREREGISTRATION.md`](BMX-GRU-V2-PRETRAINING-PREREGISTRATION.md).
RemoteOps ne doit pas interpréter ce document comme un manifeste de corpus complet.

## Dépôts candidats immuables

Toutes les références ci-dessous sont des commits de 40 hexadécimaux observés directement via GitHub le 15 octobre 2026 (et non des noms de branches réutilisables). Le SHA du blob de licence est un **objet Git**, pas le SHA-256 des données. Le volume total annoncé par GitHub est la taille du dépôt avec historique, **pas** le nombre d'octets exploitables pour l'entraînement.

| Domaine envisagé | Groupe source | Commit exact | Licence observée et blob Git |
| --- | --- | --- | --- |
| FR | `github:mdn/translated-content` | `2bafc41365c7e8d539fe9dee3f49d8a680070de1` | `LICENSE.md`, prose CC-BY-SA-2.5, blob `0a30987c4a6176a2079080fec34561e5f9e306dd` |
| FR | `github:Jimskapt/rust-book-fr` | `42bfcc8ccea02a1272e5fda936dd593b88361b80` | `LICENSE-MIT` et `LICENSE-APACHE` dans le dépôt ; MIT blob `25597d5838fa4cd7ff5c3c2bb1d1b4c3731eda7f` ; attribution traduction encore à auditer |
| EN | `github:mdn/content` | `370f1912b54d24bd7da541f8f2dd98c26d377121` | `LICENSE.md`, prose CC-BY-SA-2.5, blob `6563e36c4864195a3144b532b216f4155f5d0cba` |
| EN | `github:rust-lang/book` | `1500248d8f230566e4ec9f27fcbb8fe9e2898ab1` | `LICENSE-MIT`, blob `25597d5838fa4cd7ff5c3c2bb1d1b4c3731eda7f`; vérifier la portée de la licence et les contributions |
| RS | `github:rust-lang/rust` | `91e9c2688eb23af9a5b77f056156e756550c7c99` | `LICENSE-APACHE`, blob `1b5ec8b78e237b5c3b3d812a7c0a6589d0f7161d`; vérifier licences de fichiers et sous-répertoires |
| RS | `github:rust-lang/cargo` | `01b026f0a6119442b1104b7eec298da835b22d5b` | `LICENSE-APACHE`, blob `c98d27d4f32d56e56f13de73a08ab44b2947ccf5` |
| RS | `github:rust-lang/rustfmt` | `c69af7786cdfef5da968f53eebd6c9b1af242b5f` | `LICENSE-APACHE`, blob `212ba1f3184810909ef2186137ec0d9a6aea4a6b` |
| EN | `github:rust-lang/mdBook` | `ae0bc6996bf45cd9e93b9222fd4cd264ee69e6d4` | `LICENSE` MPL-2.0, blob `be2cc4dfb609fb6c38f6365ec345bded3350dd63` |

Les licences MDN s'appliquent différemment à la prose et aux exemples de code ; les conditions d'attribution, de copyleft et les exceptions doivent être revues fichier par fichier. Les dépôts Rust peuvent contenir des éléments tiers. Les mentions ci-dessus sont des **observations**, pas une autorisation générale d'exploiter tous les fichiers ni un avis juridique.

## Affectation et protocole anti-fuite

1. Pour chaque commit candidat, n'extraire que les fichiers textuels explicitement sélectionnés et compatibles avec l'usage autorisé : `.md` pour prose FR/EN et `.rs` pour RS. Toute autre extension, fichier généré, fixture, secret, grand binaire ou fichier sous licence différente est exclu.
2. Établir un catalogue **par document** avec groupe, auteur, URI immutable de type `https://raw.githubusercontent.com/<owner>/<repo>/<commit>/<path>`, chemin Git, licence applicable, attribution et bytes originaux. La normalisation est interdite sans transformation épinglée distincte.
3. Affecter une œuvre/dépôt complet à **un seul** TRAIN, VAL ou TEST. Ne pas considérer `rust-book-fr` et `rust-lang/book` comme indépendants : la traduction et l'original partagent un lignage documentaire. Les corpus MDN traduits et anglais peuvent aussi être des traductions des mêmes pages. Les conserver ensemble dans un split, ou établir et vérifier explicitement des groupes de contamination communs.
4. Les groupes de l'ancien TEST GRU-v1 (Sand, Daudet, Huysmans, Loti, Dolbear, Tyndall, Lodge, Maxwell, tokio, rayon, serde, regex), ainsi que les panels déjà consultés ENG-1c/ENG-1e, **ne sont pas des TEST v2 indépendants**. Les sources du GRU-v1 historique étant imparfaitement répertoriées, aucune absence de chevauchement ne peut être affirmée rétroactivement.
5. Construire les données dans un espace privé, enregistrer le hash SHA-256 et la taille de **chaque fichier brut et de chaque payload**, puis seulement sceller un TSV v1 avec le vérificateur Rust `RemoteOps/ops/bmx-gru/v2_manifest.rs --strict`. Ajouter audit des duplications proches et conformité licence, qui ne sont pas prouvés par le simple succès du vérificateur.
6. Les contraintes chiffrées enregistrées restent TRAIN ≥40 Mio, chaque FR/EN/RS TRAIN ≥2 Mio, VAL et TEST ≥1 Mio, au moins deux groupes par split. Le présent inventaire ne démontre **aucun** de ces volumes.

## Statuts et conditions de sortie

- **Candidate commit IDs and license blobs** : `OBSERVED` (ce document).
- **File-level provenance, immutable URI, sha256, split-role allocation** : `NOT_PREPARED`.
- **Full source download, fingerprint manifest and exact payloads** : `NOT_PREPARED`.
- **Independent legal/licence attribution audit and near-duplicate exclusion** : `NOT_QUALIFIED`.
- **G0 strict admission** : `BLOCKED`.
- **GRU-v2 GPU training** : `NOT_STARTED`.

Le seul progrès revendiqué ici est l'inventaire des objets sources possibles.
Ne pas simuler des empreintes SHA-256 de corpus, des volumes ou des scores
et ne pas solliciter la lecture du nouveau TEST avant la publication des règles de décision.
