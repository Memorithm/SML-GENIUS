# TESTTEST — première confirmation multi-auteurs : supériorité non établie

Auteur : Memorithm / Tarek Zekriti. Mesures du 13 octobre 2026, Linux x86_64,
Rust 1.89.0. Campagne complète : **12 œuvres, 12 auteurs, 745 088 cibles TEST**.

**L’adaptatif figé FR129/EN64 gagne sur 3 auteurs et perd sur 9.**
Son gain moyen par auteur est **−0,001379351145 bit/octet** face au classique
choisi sur DEV. Il perd sur les six œuvres anglaises. Le verdict enregistré
est `PANEL_SUPERIORITY_NOT_ESTABLISHED`. Aucune promotion du modèle.

Le banc est fonctionnel ; le résultat scientifique est négatif. Le gain
observé auparavant sur la VAL Dumas/Austen ne constitue donc pas une
supériorité générale confirmée par ce nouveau panel.

## Comparaison prospective et provenance

- Enregistrement initial : `73e73110ccc628435909cdf393a0330ed17993d3`.
- Amendement d’éligibilité et empreintes finales, avant tout score réel :
  `0bf47640eb769f6b6ed7a4efeff7a98269880f3f`.
- Première implémentation publiée et utilisée :
  `65d76ad7f229a544054f79da161cf43e49b907cd`.
- SHA256 du manifeste immuable :
  `ffd768bfe236eb1c7e9cee689d84c50702d551315dfe3327ed9dced10074e990`.
- SHA256 du binaire local :
  `c6b5a94f190c82fbe1ecff9f61e085faf66583b8ab04596fcb92c0f53bde5c6f`.

Le roster initial a été corrigé pour exclure Verne déjà exposé et Flaubert
réservé à A6-E5. Aucun score réel n’avait été calculé lors de cette correction.
Le roster final exclut aussi Dumas, Austen et Candide. Les dix autres œuvres
et tous les critères sont restés inchangés. Les sources ont été vérifiées
auprès du catalogue Project Gutenberg et les octets ont été fingerprintés
sans prédicteur avant leur enregistrement. Tous les hashes sont recontrôlés
à chaque phase ; aucun corpus ni index appris n’est archivé dans Git.

[Protocole](ENG-1C-TESTTEST-PREREGISTRATION.md),
[amendement avant mesure](ENG-1C-TESTTEST-ROSTER-AMENDMENT.md),
[commandes et limites](ENG-1C-TESTTEST-RUNBOOK.md).

Pour chaque œuvre : TRAIN chronologique 60 % plafonné à 600 000 octets ;
DEV commence à 60 % et couvre au plus 32 768 octets ; TEST commence à 80 %
et couvre au plus 65 536 octets. Les intervalles sont disjoints. Les trois
splits utilisent 64 lanes avec remise à zéro du contexte. Ni DEV ni TEST
ne modifie les comptes TRAIN. Les caps, gaps et résidus sont déclarés,
ainsi le score ne porte pas sur tous les octets du livre.

La profondeur classique est choisie parmi 4/7/12/32 exclusivement sur DEV,
puis scellée avec les hashes du corpus, de l’exécutable et de l’index TRAIN.
**DEV a choisi profondeur 4 pour chacune des douze œuvres.** Cela résulte
de la sélection indépendante, pas d’un choix effectué après TEST. Les
quatre pertes classiques TEST restent disponibles comme diagnostics,
sans nouvelle sélection du gagnant sur TEST.

L’adaptatif choisit profondeur 4 sur 719 077 cibles, profondeur 7 sur
25 025, profondeur 12 sur 986 et profondeur 32 sur zéro cible. La présence
du store profondeur 32 ne signifie donc pas que cette profondeur a été
utilisée par le candidat dans ce panel.

## Résultats par auteur

Bits/octet : plus faible est meilleur. Gain = perte classique sélectionnée
sur DEV − perte adaptative. Les décisions utilisent les f64 non arrondis ;
les valeurs ci-dessous sont affichées à douze décimales.

| Langue | Auteur | Genre | Profondeur classique DEV | Classique TEST | Adaptatif TEST | Gain adaptatif |
|---|---|---|---:|---:|---:|---:|
| FR | Victor Hugo | novel | 4 | 2.107694840840 | 2.107983634971 | -0.000288794132 |
| FR | Émile Zola | novel | 4 | 2.003345253156 | 2.003053872385 | +0.000291380772 |
| FR | René Descartes | essay | 4 | 1.885339299132 | 1.883385416740 | +0.001953882392 |
| FR | Jean-Jacques Rousseau | essay | 4 | 2.268850972019 | 2.269099129410 | -0.000248157391 |
| FR | Claude Bernard | science | 4 | 1.794900671813 | 1.793000975945 | +0.001899695868 |
| FR | Jean-Henri Fabre | science | 4 | 2.195710705596 | 2.196844225249 | -0.001133519652 |
| EN | Charles Dickens | novel | 4 | 2.195277196087 | 2.204143173809 | -0.008865977722 |
| EN | Mary Wollstonecraft Shelley | novel | 4 | 2.128348333510 | 2.131602718742 | -0.003254385232 |
| EN | Ralph Waldo Emerson | essay | 4 | 2.228677297625 | 2.229755363694 | -0.001078066069 |
| EN | John Stuart Mill | essay | 4 | 2.084798369563 | 2.086990843063 | -0.002192473500 |
| EN | Charles Darwin | science | 4 | 1.835905352109 | 1.835933618338 | -0.000028266228 |
| EN | Michael Faraday | science | 4 | 2.375828917081 | 2.379436449931 | -0.003607532850 |

## Agrégation et incertitude préenregistrées

Chaque auteur a le même poids. Un gain positif favorise l’adaptatif.
Bootstrap apparié de 9 999 réplications au niveau auteur, avec rééchantillonnage
séparé des six auteurs de chaque langue et poids égal des langues.
Les lanes et les octets ne sont pas comptés comme études indépendantes.
Les intervalles supposent des unités auteur approximativement indépendantes ;
le panel de convenance n’est pas un tirage aléatoire de toutes les langues.

| Périmètre | Gain moyen bits/octet | Intervalle bootstrap approximatif 95 % | p unilatéral sign-flip | Victoires / pertes |
|---|---:|---|---:|---:|
| ALL | -0.001379351145 | [-0.002671952455, -0.000299641456] | 0.94140625 | 3 / 9 |
| FR | 0.000412414643 | [-0.000466900698, 0.001293988281] | 0.203125 | 3 / 3 |
| EN | -0.003171116934 | [-0.005640210849, -0.001276587221] | 1 | 0 / 6 |

La moyenne pondérée par nombre de cibles, endpoint secondaire, est
−0,001335694666 bit/octet. La moyenne française est légèrement positive,
mais son intervalle inclut zéro. La moyenne anglaise est négative et les
six auteurs anglais perdent. L’intervalle global est entièrement négatif
sous les hypothèses de bootstrap déclarées. Le test sign-flip préenregistré
était unilatéral en faveur de l’adaptatif : aucune nouvelle déclaration de
significativité inverse n’est ajoutée après observation du résultat.

Les critères échouent : effet moyen >=0,01 dans chaque langue absent,
bornes basses positives absentes, test global et correction Holm des deux
langues échoués, minimum de quatre auteurs gagnants par langue absent,
moyennes anglaises négatives dans chacun des trois genres. Le contrôle
secondaire FIXED_4 donne les mêmes écarts, puisque DEV a choisi 4 partout.

| Langue | Genre | Gain moyen bits/octet |
|---|---|---:|
| FR | novel | 0.000001293320 |
| FR | essay | 0.000852862501 |
| FR | science | 0.000383088108 |
| EN | novel | -0.006060181477 |
| EN | essay | -0.001635269784 |
| EN | science | -0.001817899539 |

## Périmètre effectivement scoré

Événements = 64 × (floor(octets/64) − 1). Résidu = octets modulo 64.
Les trois gaps indiquent les octets omis entre le TRAIN plafonné et DEV,
après DEV avant TEST, et après le TEST plafonné. Ils ne sont pas scorés.

| Source Gutenberg | Langue | TRAIN octets / événements / résidu | DEV octets / événements / résidu | TEST octets / événements / résidu | Gaps TRAIN / DEV / TEST |
|---:|---|---|---|---|---|
| [17489](https://www.gutenberg.org/ebooks/17489) | FR | 405974 / 405888 / 22 | 32768 / 32704 / 0 | 65536 / 65472 / 0 | 0 / 102557 / 69789 |
| [5711](https://www.gutenberg.org/ebooks/5711) | FR | 600000 / 599936 / 0 | 32768 / 32704 / 0 | 65536 / 65472 / 0 | 22514 / 174737 / 141969 |
| [13846](https://www.gutenberg.org/ebooks/13846) | FR | 418981 / 418880 / 37 | 32768 / 32704 / 0 | 65536 / 65472 / 0 | 0 / 106892 / 74125 |
| [65434](https://www.gutenberg.org/ebooks/65434) | FR | 150401 / 150336 / 1 | 32768 / 32704 / 0 | 50134 / 50048 / 22 | 0 / 17366 / 0 |
| [16234](https://www.gutenberg.org/ebooks/16234) | FR | 381848 / 381760 / 24 | 32768 / 32704 / 0 | 65536 / 65472 / 0 | 0 / 94515 / 61747 |
| [16825](https://www.gutenberg.org/ebooks/16825) | FR | 349751 / 349632 / 55 | 32768 / 32704 / 0 | 65536 / 65472 / 0 | 0 / 83816 / 51048 |
| [98](https://www.gutenberg.org/ebooks/98) | EN | 462904 / 462784 / 56 | 32768 / 32704 / 0 | 65536 / 65472 / 0 | 0 / 121534 / 88766 |
| [84](https://www.gutenberg.org/ebooks/84) | EN | 252924 / 252800 / 60 | 32768 / 32704 / 0 | 65536 / 65472 / 0 | 0 / 51540 / 18773 |
| [2944](https://www.gutenberg.org/ebooks/2944) | EN | 247743 / 247616 / 63 | 32768 / 32704 / 0 | 65536 / 65472 / 0 | 0 / 49813 / 17045 |
| [34901](https://www.gutenberg.org/ebooks/34901) | EN | 183888 / 183808 / 16 | 32768 / 32704 / 0 | 61296 / 61184 / 48 | 0 / 28528 / 0 |
| [1228](https://www.gutenberg.org/ebooks/1228) | EN | 560697 / 560576 / 57 | 32768 / 32704 / 0 | 65536 / 65472 / 0 | 0 / 154131 / 121364 |
| [14474](https://www.gutenberg.org/ebooks/14474) | EN | 134036 / 133952 / 20 | 32768 / 32704 / 0 | 44679 / 44608 / 7 | 0 / 11911 / 0 |

## Qualification et limites

Neuf tests ciblés, dont un vrai trajet synthétique `seal` puis `test`,
comparent les pertes aux scores d’un n-gramme indépendant et contrôlent
la sélection DEV malgré une profondeur différente préférée sur TEST,
les bornes de gate, les sources/sceaux corrompus, les panels incomplets,
les probabilités sign-flip exactes, le bootstrap reproductible et une
inversion entre moyenne auteur et moyenne pondérée par tokens.
Le hook obligatoire Rust 1.89 fmt/Clippy strict/tests workspace a réussi.

La [campagne CI 38393099708](https://github.com/Memorithm/SML-GENIUS/actions/runs/38393099708)
a terminé les deux langues et l’analyse globale au commit exact
`65d76ad7f229a544054f79da161cf43e49b907cd`. Les cinq contrôles actifs
(`rust`, `fmt`, FR, EN, `paired-panel`) sont verts. Les 24 records hébergés
reproduisent exactement toutes les identités de source/store, sélections
DEV, pertes et profondeurs de la campagne locale ; seuls les hashes de
binaire et des sceaux qui les incluent diffèrent comme attendu. Le rapport
global `panel.tsv` est identique octet par octet. L’archive CI et ses records
sont conservés dans `evidence/eng1c-testtest-first-run/hosted-ci/`, avec
empreinte ZIP et compte rendu de qualification. Il s’agit d’une réplication
du même panel TEST, pas de douze nouveaux auteurs indépendants.

Le workflow de collecte est corrigé dans la PR de résultats : les artefacts
FR et EN sont téléchargés par noms explicites. Le motif initial `testtest-*`
pouvait, lors d’une relance du même run, sélectionner aussi le précédent
`testtest-complete-panel`, qui contient déjà `panel.tsv`, puis déclencher le
refus volontaire de réécriture du rapport. Cette correction concerne
uniquement la collecte CI ; les modèles, scores et critères restent figés.

Une œuvre à la fois par langue, au plus deux workers, limite virtuelle
262 144 KiB et délai de 3 000 secondes par phase. Même index complet
profondeur 32 pour tous les bras : aucune économie mémoire, vitesse de
service, qualification GPU ou supériorité SML25B ne découle de ce banc.
Les anciens TEST Dumas/Austen/Verne/Flaubert ne sont pas ouverts par le banc.

Ce résultat concerne FR129/EN64 sur ces extraits historiques, avec
entraînement séparé sur le préfixe de chaque œuvre. Il ne démontre pas
l’inutilité de toute adaptation, ni la supériorité universelle du classique.
Le panel n’inclut pas texte web récent, dialogue, code, autres langues ou
transfert entre auteurs. Les paramètres ne sont pas retouchés ici ; une
nouvelle politique nécessitera un nouveau panel indépendant préenregistré.
Les réexécutions de ces TEST sont des réplications, pas de nouvelles
confirmations indépendantes.

[Résultats bruts, sceaux, hashes et environnement](evidence/eng1c-testtest-first-run/).
Le module Rust `paired_panel` est réutilisable pour d’autres comparaisons
appariées sans dépendre de ce prédicteur. Le workflow dispose d’une entrée
`workflow_dispatch` pilotable par RemoteOps, qui reste le plan de contrôle
distant de référence.
