# ENG-1d — politique de repli du moteur ElasticNGram (référence exploratoire)

Statut : **outil et exploration, aucune confirmation**. Ce document ne
modifie ni ne réinterprète le verdict TESTTEST
(`PANEL_SUPERIORITY_NOT_ESTABLISHED`, PR #546). Les douze TEST TESTTEST
restent exposés et ne doivent être utilisés par aucune étape ci-dessous.

## Diagnostic

Dans TESTTEST, les quatre bras classiques se dégradent avec la profondeur
pour **chacun des douze auteurs** (`docs/evidence/eng1c-testtest-first-run/*-test.tsv`) :
en moyenne environ 2,09 / 2,34 / 2,74 / 2,86 bit/octet aux profondeurs
4 / 7 / 12 / 32. DEV choisit donc 4 partout et la porte adaptative ne
choisit qu'entre des instantanés dégradés.

Cause lue dans le code (`PagedNGram::depth_ladder_readout`) : chaque niveau
observé de 0 à la profondeur demandée applique
`p ← (n_y + U·p_court) / (N + U)`. Un contexte long vu une seule fois cède
seulement la moitié de la masse au niveau inférieur, et les niveaux emboîtés
répètent souvent la même évidence. Le moteur fait trop confiance aux contextes
longs peu observés. Ce n'est pas un bug d'implémentation : c'est le
comportement documenté de Witten–Bell brut sur octets.

## Exploration hors panel (non probante)

Script : `scripts/eng1d/escape_diagnostic.py` (Python pur, ré-implémentation
indépendante, même découpage 60/80 % et 64 lanes). Texte : *Opticks* de
Newton (copie Gutenberg distribuée dans les sources Go,
`src/testdata/Isaac.Newton-Opticks.txt`), auteur absent de TESTTEST.

| Politique | DEV meilleure profondeur | DEV bit/octet | TEST bit/octet (même profondeur) |
|---|---:|---:|---:|
| `wb` (ENG-1c) | 4 | 2,1144 | 2,3616 |
| `dir:τ=64` | 12 | 2,0282 | 2,2432 |
| `swb:β=16` | 12 | 1,8978 | 2,0916 |

Avec TRAIN tronqué à 134 036 octets (taille Faraday), `wb` profondeur 4 donne
2,7111 sur TEST et `swb:β=16` profondeur 7 donne 2,3468. Un seul livre, une
seule langue : c'est une direction à vérifier, pas un résultat. Le meilleur
seuil de la porte FR129/EN64, choisi après coup sur ce TEST, ne gagne rien
(0,0000) ; sur DEV, +0,001.

## Ce que cette PR ajoute

- `EscapePolicy` dans `elastic_ngram_paged` : `WittenBell` (e = U),
  `ScaledWittenBell { beta }` (e = β·U), `Dirichlet { tau }` (e = τ, forme
  parent de `anf_byte_backoff::distribution_dirichlet_with_parent`).
  Paramètres non finis ou ≤ 0 refusés.
- `depth_ladder_readout_with(history, policy)` ; `depth_ladder_readout`
  délègue avec `WittenBell` et reste bit à bit identique (test).
- `target_ladders(history, target, policies)` : probabilité de l'octet cible
  seulement, à toutes les profondeurs 0..=32 et pour plusieurs politiques,
  en lisant chaque histogramme une fois. Valeurs bit à bit égales à la
  lecture complète ; environ 256 fois moins d'arithmétique par niveau.
- Commande `sml-eng1c-testtest dev-grid MANIFEST RAW_DIR WORK_DIR ID NEW_GRID` :
  mêmes contrôles de manifeste/source que `seal`, même store TRAIN, score
  **DEV uniquement** sur la grille figée (19 politiques × profondeurs
  2,3,4,5,6,7,8,10,12,16,24,32). Sortie nouvelle obligatoire, empreinte
  `.sha256`, champ `test_scored NO`. La ligne `wb` aux profondeurs 4/7/12/32
  reproduit bit à bit les `dev_bits*` d'un sceau (test).

Tests : parité `WittenBell` bit à bit avec la lecture figée et avec le chemin
rapide ; égalité chemin rapide / lecture complète pour toutes les politiques ;
oracle indépendant par comptage brut sur lanes ; normalisation ; refus des
paramètres invalides ; limite e → ∞ ; `dev-grid` refuse l'écrasement,
reproduit les bits DEV scellés et ne change pas quand TEST est brouillé.

## Règles pour la suite

1. `dev-grid` sur les DEV TESTTEST est exploratoire. Le DEV ayant déjà servi
   à la sélection classique, cela ne crée aucune preuve indépendante.
2. Toute politique choisie ainsi (β ou τ, profondeur, éventuel mélange) est
   une **nouvelle politique** : confirmation seulement sur un nouveau panel
   préenregistré d'œuvres jamais exposées, avec sélection sur DEV, seuil
   d'effet et puissance justifiés avant mesure, et plusieurs volumes TRAIN
   contrôlés par œuvre.
3. Le comparateur reste le classique `wb` avec profondeur choisie sur DEV.
