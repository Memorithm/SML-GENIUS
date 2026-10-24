# GB-11 P1 — résultats sur S5h (24 octobre 2026)

Protocole : `docs/GB11-P1-S5H-PREREGISTRATION.md`, avec l'amendement écrit avant tout résultat. Code : `crates/sml-core/src/gb11_p1_stream.rs` et le binaire `sml-gb11-p1-s5h`.

Le run de référence est le run RemoteOps `sml-gb11-p1-s5h-dell.yml` au commit `3c7acc9` (SML `958c11d8`), lancé sur le Dell T430 le 24 octobre 2026 (run 40079071322). Il a duré 28 minutes en tout, dont 346 s de recherche. Les métriques complètes sont dans `research/results/gb11/P1-s5h-metrics.tsv`. Les poids et les traces sont restés sur le Dell, dans `~/sml-artifacts/sml-gb11-p1-s5h-3c7acc9b4cdba348916a1095d4272789477d8a56/`. TEST n'a pas été ouvert.

## Réponse courte

- **Les experts en forme close battent le témoin à gradient, à contexte et octets égaux.** Avec les deux mêmes octets de contexte que le témoin et la même taille de table que son q4, `order2_q4` obtient 3,516 BPC sur DEV, contre 3,785 pour le témoin q4 et 3,655 pour son f32, qui occupe 8 fois plus d'octets. Ce bras est le seul des trois à 25 Mo qui ne gagne pas grâce à un contexte plus long.
- **Le câblage appris par recherche gloutonne perd nettement contre un câblage fait à la main.** `search_q4` obtient 3,500 ; `bytes_q4`, avec les n-grammes d'octets d'ordres 0 à 6, obtient 3,021. C3 échoue avec un écart de +0,48 BPC.
- **La cause est identifiée : la recherche s'arrête après 2 banques.** Elle construit une deuxième banque très bonne : l'octet précédent sans son bit de casse, l'octet d'avant, un bit du 3e octet et un bit du registre de mot. Ensuite, aucune troisième banque ne démarre. Une banque neuve d'un seul bit ne vaut pas mieux que l'ordre 0, et sa contribution au mélange reste sous le seuil de 0,001 bit par octet. La recherche ajoute un bit à la fois, ce qui la rend myope.
- **Le critère primaire C2 passe**, mais il ne dit rien de la méthode : 3,500 < 3,785 (et < 3,655).

## Tableau DEV (60 237 946 événements, même manifeste `966eb65f…`)

| Bras | Table (octets) | Banques | Mélange / mise à jour | Nœuds appris | BPC TRAIN préquentiel | **BPC DEV** |
|---|---:|---:|---|---:|---:|---:|
| témoin `--stream-50m-msb` q4 | 25 000 448 | — | gradient, 17 traits | 15 068 983 | 2,783 | 3,785 |
| témoin f32 | 200 003 584 | — | gradient | 15 068 983 | 2,783 | 3,655 |
| `order2_q4` (ordres 0, 1, 2) | 25 000 416 | 3 | top / stationary | 846 179 | 2,913 | **3,516** |
| `search_q4` (primaire) | 8 333 440 | 2 | top / stationary | 747 712 | 2,760 | **3,500** |
| `search_f32` | 66 667 840 | 2 | top / stationary | 1 358 388 | 2,749 | **3,354** |
| `bytes_q4` (ordres 0, 1, 2, 3, 4, 6) | 25 000 320 | 6 | average / stationary | 2 546 674 | 2,239 | **3,021** |

Les bras cherchés gardent `table / 6` octets par banque, comme le veut l'amendement. Avec 2 banques seulement, `search_q4` n'occupe donc qu'un tiers de son budget. Le masque de la banque 1 est `0x20000000000040ffdf` : les bits 0 à 4 et 6 à 7 de c1 (tout sauf le bit de casse 5), les 8 bits de c2, le bit 6 de c3 et le bit 5 de `W`.

## Critères

- **C1 (intégrité) : vrai.**
  - Le manifeste est égal à celui du témoin.
  - DEV compte 60 237 946 événements.
  - Les tables sont inchangées par DEV.
  - Les tailles restent sous le budget.
  - `trace-dev.bin` fait 60 237 946 × 40 octets et redonne 3,500060621.
  - `bytes_q4` et `order2_q4` sont **identiques bit à bit** entre le run annulé et le run de référence (mêmes SHA de poids).
- **C2 (primaire) : vrai.** 3,500 < 3,785 ; forme forte, < 3,655, vraie aussi.
- **C3 (câblage appris) : faux.** La recherche donne 3,500 contre 3,021 pour les n-grammes, soit +0,479, au-delà du seuil de +0,02. Le câblage appris est donc moins bon que les n-grammes.
- **C4 (décomposition) :** à contexte égal (2 octets), les compteurs font −0,269 BPC contre le témoin q4 et −0,139 contre le f32. Le contexte plus long (`bytes_q4`) apporte ensuite −0,495 de plus.
- **C5 (échelle) :** 8,3 → 66,7 Mo fait passer `search` de 3,500 à 3,354.

## Run annulé (commit `73f9199`, SML `cd05f26`)

La recherche y a retenu les mêmes 2 banques. L'amendement prévoit ce cas : ses bras `search_*` ne comptent pas. Ils répartissaient tout le budget entre les 2 banques et obtenaient 3,403 (`search_q4`, 25 Mo) et 3,339 (`search_f32`, 200 Mo). Ces chiffres sont rapportés à titre d'information seulement. Même avec ce budget triplé, la recherche reste à +0,38 de `bytes_q4`.

## Lecture honnête

- **Ce que P1 montre.** Sur du texte réel, un expert de type « table de comptes » adressé par des bits de contexte (porte booléenne de câblage, lecture de table, arithmétique entière) atteint une meilleure qualité qu'un modèle linéaire à gradient de même mémoire et de même contexte. La règle `average` ou `top` est fixe, sans poids appris. L'apprentissage tient en une passe de comptage : environ 8 à 21 min par bras sur le Dell, contre 5 h 54 pour le témoin.
- **Ce que P1 ne montre pas.** Le câblage *appris* ne bat pas un câblage choisi à la main. Le gain de `bytes_q4` vient d'un contexte plus long (jusqu'à 6 octets), pas de l'apprentissage. Sur ce point, la question de l'utilisateur, « apprendre les représentations », reste ouverte.
- **Pourquoi la recherche échoue, et comment la corriger.** Ajouter un bit à la fois ne peut pas lancer une nouvelle banque : avec un seul bit, elle est dominée par celles qui existent. Deux corrections sont possibles, à préenregistrer avant tout nouveau run :
  1. initialiser chaque banque par un octet entier ou par la banque précédente plus un octet, puis retirer ou ajouter des bits (recherche d'échange, comme le « hard-EM » de P0b) ;
  2. noter une banque seule par son gain de mélange avec un seuil adapté à sa taille.
  Le résultat de `bytes_q4` sert de point de départ naturel à la variante 1.
- **L'échec se voyait déjà sur TRAIN.** Sur la holdout TRAIN de 2 Mio, `bytes_q4` obtenait 3,816 (`average`), contre 4,252 pour `search_q4` (`top`). L'écart n'est donc pas un artefact du DEV. Le biais vers les adresses courtes, annoncé à cause du préfixe de 16 Mio, s'ajoute à la myopie de la recherche.
- **Aucun temps natif ni énergie n'est revendiqué** (c'est P2). Le témoin n'a pas été relancé.

## Traces pour H-1

Les traces viennent du bras `search_q4` du run de référence :
- `trace-dev.bin` : 60 237 946 enregistrements, BPC 3,500060621, SHA-256 `d273af88…c408` ;
- `trace-train.bin` : 65 797 543 enregistrements préquentiels, BPC 2,759864504, SHA-256 `9cff4673…134e` ;
- poids : `257a3480…562f`.

H-1 corrige donc le bras primaire préenregistré, qui n'est pas le meilleur bras P1. Le fil H-1 en est informé.
