# GRU-v2 / SML-GENIUS — sources Hugging Face du domaine public

Date de revue : 2026-10-15. **État : SOURCE_CANDIDATE_ONLY, G0_admitted=false.**
L'objectif est d'abandonner la dépendance exclusive à la collecte de documents
de logiciels récents, en privilégiant des œuvres anciennes / du domaine public,
des données publiques et des corpus dont la provenance est documentée.

**Aucune de ces sources n'est encore un volume TRAIN/VAL/TEST admis.**
Les cartes de jeux de données sont des déclarations des déposants : le tag
`license` du dépôt n'atteste pas nécessairement la situation juridique de
chaque œuvre, de chaque transcription ou de chaque traduction.

## Six candidats vérifiés dans le catalogue Hugging Face

| Priorité | Dataset Hugging Face | Domaine | Origine / revendication | État |
| --- | --- | --- | --- | --- |
| P0 | [common-pile/project_gutenberg](https://huggingface.co/datasets/common-pile/project_gutenberg) | EN, littérature ancienne | Project Gutenberg, textes anglais désignés « Public Domain » par métadonnées, plus PG19 ; risque d'édition/traduction et portée territoriale | AUDIT_WORK_LEVEL |
| P0 | [PleIAs/US-PD-Newspapers](https://huggingface.co/datasets/PleIAs/US-PD-Newspapers) | EN, presse historique | Library of Congress, Chronicling America ; carte CC0-1.0 et déclaration de domaine public | AUDIT_WORK_LEVEL |
| P0 | [common-pile/uspto](https://huggingface.co/datasets/common-pile/uspto) | EN, science/techniques/brevets | Corpus USPTO revendiqué d'œuvres gouvernementales américaines, 20 294 152 documents annoncés ; noter l'avertissement du fournisseur sur erreurs possibles de métadonnées de licence | AUDIT_PATENT_SOURCE |
| P1 | [PleIAs/French-PD-Books](https://huggingface.co/datasets/PleIAs/French-PD-Books) | FR, monographies anciennes | Gallica/BnF, collection revendiquant 289 000 livres, auteur décédé depuis plus de 70 ans ; les conditions BnF sur la réutilisation commerciale des reproductions font l'objet d'un contrôle séparé | BNF_TERMS_REVIEW |
| P1 | [PleIAs/French-PD-Newspapers](https://huggingface.co/datasets/PleIAs/French-PD-Newspapers) | FR, presse ancienne | Gallica/BnF, collection revendiquant près de 3 millions d'éditions ; revue des œuvres collectives, contributions individuelles et CGU BnF requise | BNF_TERMS_REVIEW |
| P1 | [PleIAs/common_corpus](https://huggingface.co/datasets/PleIAs/common_corpus) | FR/EN/RS potentiel | Corpus composite à provenance documentaire et licences différenciées, revendiquant 2,27 billions de tokens ; ne retenir que des documents vérifiés public domain / licence appropriée | PER_DOCUMENT_FILTER |

Sources de comparaison **non prioritaires ou refusées comme preuve directe de domaine public** :

- [storytracer/US-PD-Books](https://huggingface.co/datasets/storytracer/US-PD-Books) :
  sa carte signale que les textes ont été retirés à la demande d'Internet
  Archive en raison de métadonnées insuffisamment fiables sur le domaine public.
  **Ne pas le réutiliser comme corpus de livres homologués.**
- [rayml/french_gutenberg](https://huggingface.co/datasets/rayml/french_gutenberg) :
  un tag Apache-2.0 du dépôt ne prouve pas à lui seul que chaque œuvre,
  traduction et édition ancienne relève de cette licence.
- [NortheasternUniversity/big_patent](https://huggingface.co/datasets/NortheasternUniversity/big_patent) :
  sa carte annonce CC-BY-4.0, ce qui n'est pas équivalent à absence de droits.
  Revue distincte possible pour un volet entraînement sous attribution.

## Pourquoi ce choix

La presse historique et la littérature améliorent la couverture des
structures syntaxiques et narratives FR/EN. Les publications de brevets
apportent descriptions techniques, relations causales et vocabulaires
d'ingénierie ; elles ne prouvent pas à elles seules une capacité logique
ou mathématique. La répartition TRAIN/VAL/TEST sera établie par œuvre,
édition, auteur/famille et source, jamais par ligne Parquet aléatoire.

Un brevet **expiré** signifie, sous réserve de son droit territorial et
de son statut réel, qu'un monopole d'exploitation peut avoir cessé.
Cela ne constitue pas une garantie générale d'absence de droit sur
l'ensemble des illustrations, textes incorporés, traductions ou fichiers
d'une archive. Pour USPTO, vérifier la source et le statut de
reproduction des textes retenus, et ne pas confondre brevet et
copyright du document.

Un auteur **sans héritier identifié** ne devient pas automatiquement
« libre de droits ». La règle générale française de droits patrimoniaux
est 70 ans après l'année du décès, avec exceptions ; les droits moraux
et certaines éditions/traductions appellent une analyse distincte.
Les œuvres orphelines ne sont pas automatiquement dans le domaine public.

**Gallica/BnF :** la BnF indique que les métadonnées descriptives sont
sous licence ouverte, mais que ses documents numérisés peuvent être soumis
à des conditions de réutilisation commerciale. Le statut de domaine
public de l'œuvre et les conditions d'accès aux reproductions sont deux
questions à documenter avant tout usage commercial de ce corpus.

Références de politique / fournisseur :
- [BnF — API et jeux de données](https://www.bnf.fr/fr/portail-bnf-api-et-jeux-de-donnees)
- [BnF — utilisation commerciale d'une reproduction](https://www.bnf.fr/fr/faire-une-utilisation-commerciale-dune-reproduction)
- [Hugging Face — dataset cards](https://huggingface.co/docs/hub/datasets-cards)

## Circuit de réalisation G0, sans contournement

1. **G0-HF0 — sonder les révisions du Hub** de chaque source avec
   `git ls-remote` ou une API officielle et conserver le commit
   immuable, le README/licence associés et leur empreinte de preuve.
   Pas de téléchargement volumineux au moment de cette sonde.
2. **G0-HF1 — sélectionner un micro-lot documentaire vérifiable**,
   avec pays de première publication, auteur/décès quand connu,
   édition/traduction, source, droits, URL, langue, période, taille,
   qualité OCR et indicateurs de texte tiers. Bloquer les inconnus.
3. **G0-HF2 — conserver les reçus** en local privé, les décisions
   `REVIEWED/EXCLUDE` et la signature de l'autorité documentaire
   indépendante du signataire des poids. Les reçus documentent
   précisément les éventuelles obligations d'attribution.
4. **G0-HF3 — figer une partition documentaire** sous identité
   ouvrage/numéro de brevet/journal/édition, avec ablations et
   décontamination des données v1. Ne jamais exposer le TEST au fit.
5. **G0-HF4 — matérialiser seulement les contenus autorisés** et
   exécuter la validation Rust G0 sur leurs SHA-256 et octets exacts.
   Recalculer les seuils après exclusions : 40 Mio TRAIN,
   2 Mio chacun FR/EN/RS, 1 Mio VAL, 1 Mio TEST,
   et deux groupes indépendants par partition.

Attention : les jeux de données historiques FR et EN ne couvrent pas
automatiquement **les 2 Mio de Rust natif** exigés. Il faut conserver
une source de programmes Rust sous licence compatible et attestée,
ou faire approuver une modification explicite du protocole scientifique
AVANT l'examen de résultats. Ne pas utiliser des textes de brevets
comme substitut arbitraire au Rust.

## Décision actuelle

`dataset_revision_pinned=false` ; `documents_selected=0` ;
`rights_approved_files=0` ; `train_val_test_frozen=false` ;
`G0_admitted=false` ; `v2_training_started=false`.

Ce recensement constitue un nouveau plan d'approvisionnement **fondé
sur des sources effectivement identifiées sur Hugging Face**, et
non une admission de données ou une annonce de supériorité du modèle.
