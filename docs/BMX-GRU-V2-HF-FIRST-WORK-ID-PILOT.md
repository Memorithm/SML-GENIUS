# GRU-v2 / G0-HF2 — premier micro-lot de 7 œuvres identifiées (métadonnées uniquement)

Date : 2026-10-15. **État : `WORK_ID_CANDIDATES_ONLY` ; zéro texte admis.**

Ce micro-lot fait progresser le chantier depuis les *fichiers conteneurs*
Hugging Face vers des **identifiants d'œuvres** vérifiables. Aucune
partition TRAIN/VAL/TEST n'est attribuée et aucun texte n'a été
téléchargé, haché comme payload ni présenté comme licence autorisée.
Les œuvres restent exclues du dataset entraînable.

## Inventaire contrôlé des conteneurs

Le workflow Dell
[RemoteOps #38798878562](https://github.com/Memorithm/RemoteOps/actions/runs/38798878562)
a constaté **30 fichiers de données Gutenberg**, **100 fichiers USPTO**
et **145 fichiers français Gallica**, soit 275 conteneurs de données
et 6 fichiers de métadonnées Git. L'inventaire conserve les chemins
et blobs de pointeurs Git, mais **ne lit aucun payload Xet/LFS**.

SHA-256 du TSV des 281 entrées :
`39ce85eda83ff1cde165e8507a8595d27486f3fe70e996c417e05e5d775b5771`.
[Artefact #12002618666](https://github.com/Memorithm/RemoteOps/actions/runs/38798878562/artifacts/12002618666).
RemoteOps [PR #435](https://github.com/Memorithm/RemoteOps/pull/435)
fusionnée sur le commit `495197c59bfc6a5f3a42edaf817ebb6f96cb6a0a`.

## Sept exemples d'œuvres / documents à examiner

Les références proviennent de fiches Project Gutenberg, ou de
**l'aperçu public** du dataset Hugging Face correspondant. Une ligne
d'aperçu ne suffit pas pour prouver que le même enregistrement
est présent dans les octets de la révision Git épinglée : **cette
correspondance reste à effectuer avant toute admission.**

| Origine/ID d'œuvre | Auteur et date | Catégorie | Risque documentaire |
| --- | --- | --- | --- |
| Project Gutenberg [ebook #1342](https://www.gutenberg.org/ebooks/1342), *Pride and Prejudice* | Jane Austen (1775–1817), œuvre 1813 | EN littérature | Vérifier l'édition/transcription de la version effectivement extraite du dataset HF |
| Project Gutenberg [ebook #2701](https://www.gutenberg.org/ebooks/2701), *Moby Dick* | Herman Melville (1819–1891), œuvre 1851 | EN littérature | Plusieurs versions Gutenberg : séparer édition et lignage |
| Project Gutenberg [ebook #84](https://www.gutenberg.org/ebooks/84), *Frankenstein* | Mary Shelley (1797–1851), œuvre 1818 | EN littérature | Les éditions de 1818 et 1831 sont distinctes ; préciser le témoin textuel |
| Gallica `bpt6k55771383`, *Des idées napoléoniennes* | Napoléon III (1808–1873), exemplaire 1860 | FR livre ancien | Reproduction numérisée BnF, conditions commerciales à examiner |
| Gallica `bpt6k6138136q`, *Rêves et devoirs* | Théodore Froment (1839–1901), exemplaire 1873 | FR livre ancien | Vérifier contribution exacte, qualité OCR (indicateur affiché 96), conditions BnF |
| USPTO `US-205091D-A`, *Improvement in lap-links* | Matthew Howard, publication 18 juin 1878 | EN brevet historique | L'enregistrement HF indique **CC-BY-4.0**, à vérifier : ne pas le traiter comme CC0 |
| USPTO `US-1901060788-A`, *Apparatus for the manufacture of artificial stone* | Friedrich Marx, publication 17 juin 1902 | EN brevet historique | Idem : licence affichée CC-BY-4.0 et métadonnées d'origine Google Patents |

Pages des aperçus et colonnes observées :
[Common Pile Gutenberg](https://huggingface.co/datasets/common-pile/project_gutenberg),
[PleIAs French-PD-Books](https://huggingface.co/datasets/PleIAs/French-PD-Books),
[Common Pile USPTO](https://huggingface.co/datasets/common-pile/uspto).

Les colonnes visibles varient selon la source :

- Gutenberg : `id`, `text`, `source`, `added`, `metadata` ;
  cette dernière peut contenir `license`, `language`, `url`, `title`.
- Gallica : `file_id`, `ocr`, `title`, `date`, `author`,
  `page_count`, `word_count`, `character_count`, `complete_text`.
- USPTO : `id`, `text`, `added`, `created`, `source`,
  `metadata` ; cette dernière peut contenir la date de publication,
  la langue et des conditions d'attribution **CC-BY-4.0**.

Les trois sources sont épinglées respectivement aux révisions :

- `common-pile/project_gutenberg` :
  `01dc90a5002f8977c7fb03a372c14bca29c65cf1`
- `PleIAs/French-PD-Books` :
  `21abfb3f646a0f1846175c63709913a51abc9bcb`
- `common-pile/uspto` :
  `e8b975381547f026e2ccfe721f785dcb6a2fa9a0`

## Conditions de passage de « candidat » à « accepté »

1. **Rechercher les ID au sein des payloads de la révision épinglée**,
   en extraction bornée et privée ; aucun téléchargement de corpus
   entier et aucun texte TEST ouvert à l'entraînement.
2. Vérifier l'identité **ouvrage–édition–traduction–source**,
   les éléments ajoutés par l'éditeur/transcripteur, les droits dans
   le pays d'usage et les CGU du fonds numérisé. Des auteurs décédés
   avant 1956 ne suffisent pas à eux seuls à qualifier une édition.
3. Vérifier l'existence du contenu, sa qualité OCR, son SHA-256 réel,
   puis obtenir les décisions `REVIEWED` documentées ou `EXCLUDE`,
   les reçus privés et une signature par une autorité de confiance.
4. Former les familles indépendantes, figer TRAIN/VAL/TEST **avant les
   mesures de performance** et refaire les contrôles G0. Ne jamais
   splitter au hasard les lignes Parquet : plusieurs éditions d'une
   même œuvre restent dans la même famille de provenance.

L'absence d'ayant droit connu ne prouve pas le domaine public.
L'expiration d'un brevet ne prouve pas non plus l'absence d'autres
droits ou l'équivalence avec CC0. Les deux exemples USPTO ne doivent
pas être marqués « libres de droits sans conditions » tant que leur
licence déclarée CC-BY-4.0 n'a pas été évaluée.

**Bilan : 7 identifiants prospectifs, 0 document admis, 0 octet
TRAIN/VAL/TEST, `G0_admitted=false`.** Le volet Rust natif
(au moins 2 Mio dans TRAIN selon le protocole) reste nécessaire.
