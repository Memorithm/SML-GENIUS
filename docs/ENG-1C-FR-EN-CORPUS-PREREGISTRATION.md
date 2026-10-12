# ENG-1c/0 — corpus français/anglais indépendants, provenance AVANT modèle

**Préenregistré le 12 octobre 2026 avant tout code de téléchargement,
calcul d'empreinte, prédiction ou entraînement ENG-1c.**
Issue scientifique : [#530](https://github.com/Memorithm/SML-GENIUS/issues/530).
Cette tranche est **strictement une campagne de provenance en lecture seule**
et non un résultat de qualité, de mémoire, de CTW ou d'adaptation de profondeur.

## Corpus et provenance prédéclarés

Deux œuvres distinctes et non utilisées dans les précédents bancs
Jules Verne/A6-E3, Voltaire/A6-E4, Flaubert/A6-E5 :

1. Français : Alexandre Dumas et Auguste Maquet, *Le comte de Monte-Cristo,
   Tome I*, Project Gutenberg **#17989**,
   `https://www.gutenberg.org/cache/epub/17989/pg17989.txt`.
   Catalogue : https://www.gutenberg.org/ebooks/17989
2. Anglais : Jane Austen, *Pride and Prejudice*, Project Gutenberg
   **#1342**, `https://www.gutenberg.org/cache/epub/1342/pg1342.txt`.
   Catalogue : https://www.gutenberg.org/ebooks/1342

Les pages indiquent une mise à jour 2026 : **les URL seules ne sont pas
des identités immuables**. La campagne doit publier la SHA256 de chaque
source brute (octets téléchargés) et celle du corps normalisé; elle ne doit
jamais considérer une URL ou un titre comme une preuve de stabilité.
Tous les usages futurs devront refuser une divergence d'empreinte.

## Prétraitement figé avant première exécution

- `curl` public HTTPS uniquement, TLS 1.2 minimum, redirections contrôlées,
  `--fail`, cinq tentatives bornées, timeout. Aucun identifiant,
  cookie, session ou service local n'entre en jeu.
- Conserver les octets source bruts pour calculer `raw_sha256`. Supprimer
  tous les octets CR (`\r`) pour construire la version à LF.
- Contrôler exactement un marqueur de début
  `^*** START OF THE PROJECT GUTENBERG EBOOK` et un marqueur de fin
  `^*** END OF THE PROJECT GUTENBERG EBOOK` (astérisques littéraux).
  Rejeter l'absence, la multiplicité, l'inversion ou les marqueurs malformés.
  Le texte retenu exclut les lignes de marqueur et le préambule/postambule.
  Chaque ligne du corps retenu conserve ses octets et termine par LF
  via AWK sous `LC_ALL=C`. Pas de nettoyage linguistique,
  suppression des espaces/accents, NFC/NFKC ou reformatage des phrases.
- Vérifier titre et langue dans les en-têtes publics,
  `French` pour #17989, `English` pour #1342.
- Exiger `100000 <= body_bytes <= 10000000`.
  Toute erreur HTTPS, d'intégrité, de marqueur ou de taille est un échec.
- Empreinte et longueur sont calculées **sans utilisation de modèle**.
  Le partage du corps brut/normalisé par artefact GitHub est interdit :
  archiver seulement une petite preuve texte du SHA/longueur.
  Les fichiers sont temporaires sur le runner GitHub-hosted.

## Dénominateurs structurels sans lire TEST avec un modèle

Utiliser les mêmes séparations que
`sml_core::anf_char::CharSplits::new` :
`a = floor(8*N/10)`, `b = floor(9*N/10)`;
TRAIN `[0,a)`, VAL `[a,b)`, TEST `[b,N)`.
Pour chaque séparation de longueur S, le nombre de symboles susceptibles
d'être scorés pour le N-gramme de référence à t>=1,
sur 64 tranches contiguës, est
`64 * (floor(S/64) - 1)`; résidu non utilisé à déclarer.
Ce n'est pas un score ni un accès au contenu étiqueté TEST.
Si le nombre ne satisfait pas les minima, **STOP avant expérimentation**.

**Règle majeure :** cette étape produit les seules empreintes. Après
premier fingerprint en lecture seule, un commit séparé peut figer les
SHA256 et les tailles **avant tout modèle ENG-1c**. Une nouvelle
exécution doit vérifier la cohérence de ces valeurs. Ne pas lancer
de modèle dans le workflow de provenance.

## Séquence pour la suite scientifique (autre PR)

- Préenregistrer ensuite taille/population de travail et limites
  Ngram (actuellement 500000 événements, 250000 nœuds,
  500000 couples) avant de consulter les pertes VAL/TEST.
  Le corps complet peut dépasser ces limites : une erreur de capacité
  ne justifie pas un redimensionnement piloté par la perte TEST.
- Même TRAIN et mêmes symboles pour Witten–Bell NGram4/NGram7,
  ENG-1a/ENG-1b aux profondeurs 7/12/16/24/32.
- ENG-1a vs ENG-1b à profondeur identique = **distributions
  strictement égales**, parité avant chronométrage.
- Choix éventuel de profondeur sur VAL uniquement, puis TEST gelé
  évalué une fois. L'évaluation ne déduit pas de gain linguistique
  du seul avantage de mémoire du trie. Aucun harnais SML-25B
  ni transition réelle ElasticXxx n'est autorisé.
- CTW/PPM demandent de nouvelles références explicites (binaire
  vs 256 classes), de nouveaux budgets et un protocole autonome.

## Qualification CI

GitHub-hosted `ubuntu-24.04` uniquement, token lecture seule,
aucun runner Thor privé, pas de code de recherche exécutant une
fonction prédictive. La CI dédiée capture uniquement un manifeste
`ENG1C_FINGERPRINT` par texte et un SHA du manifeste lui-même.
La CI générale SML-GENIUS doit rester verte sur le commit final.
Le passage de l'étape 0 signifie **provenance gelée**, pas
`RESEARCH_PASS` ni `MODEL_PASS`.
