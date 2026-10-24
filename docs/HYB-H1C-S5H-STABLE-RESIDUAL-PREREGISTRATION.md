# HYB H-1c — tête ternaire moyennée, noyau popcount, jugée sur un échantillon neuf : préenregistrement

**Gelé le 24 octobre 2026, avant toute exécution H-1c et avant toute lecture d'EVAL2.** Le propriétaire a choisi, le 24 octobre 2026, l'option « H-1c » (stabilité, noyau rapide, nouvel échantillon).

Ce document fait suite à `RESULTS-HYB-H1B-S5H-CALIBRATED-RESIDUAL.md` (verdict NO_GO). H-1c est un **nouvel essai, déclaré comme tel**. Il ne révise ni le verdict H-1 ni le verdict H-1b, qui restent publiés.

## Ce que H-1b a montré, et ce que H-1c change

Sur `bytes_q4`, H-1b a gagné 0,041 et 0,027 BPC selon la graine. Il a échoué sur deux critères :

1. **L'écart entre graines.** Il valait 0,0143 BPC : le gain le plus faible (0,0266) n'atteignait pas deux fois cet écart (0,0287). Avec une seule époque, la tête dépendait de l'ordre des documents, et les deux graines retenaient des (τ, λ) différents.
   - **H-1c :** chaque graine fait 4 époques, chacune dans son propre ordre permuté. La tête exportée vient de la **moyenne de Polyak–Ruppert** des maîtres, échantillonnés après chaque document de CAL-FIT, pendant les époques 2 à 4 (l'époque 1 sert de rodage).
2. **Le coût.** Le ratio valait 1,89 (limite 1,5). Le noyau bitplane allouait de la mémoire à chaque appel, soit 8 allocations par position.
   - **H-1c :** un noyau sans allocation (`FastHead`). Il garde deux plans `u64` par profondeur (+1 et −1) et calcule chaque produit avec 4 popcounts. Ses sorties doivent rester identiques à l'oracle scalaire.

Ces deux changements ont été choisis **après** avoir vu les résultats H-1b sur le DEV S5h. Ce DEV a déjà été lu deux fois (H-1 et H-1b) : il ne peut plus servir de critère. H-1c est donc jugé sur un échantillon **jamais lu**, EVAL2. La tête H-1c n'ouvre jamais la trace DEV.

## L'échantillon EVAL2

- **Source.** Le fichier peS2o-0002 (épinglé `d6aedcba…`). Le stage S5h ne l'a jamais ouvert : son budget TRAIN de 1 Gio est atteint dans peS2o-0001 (37 947 lignes lues, run RemoteOps 40093854277).
- **Règle.** Seuls les articles du seau DEV entrent dans EVAL2 (`SHA-256(id)[0] % 32 == 0`, la règle de découpe S5h). Aucun article d'EVAL2 n'aurait donc pu être un article TRAIN selon la règle S5h.
- **Filtres.** Les mêmes que S5h : forme, droits revus par l'opérateur, adresses e-mail.
- **Doublons.** Le stager fait d'abord le stage S5h complet, puis lit peS2o-0002 avec les mêmes ensembles de doublons. Un article dont l'identifiant, le texte exact ou le texte normalisé apparaît déjà dans S5h (ou plus tôt dans EVAL2) est exclu.
- **Taille.** On s'arrête à 32 Mio, en paquets de 1 Mio au plus.
- **Audit.** L'audit de recouvrement S4d est relancé sur le manifeste S5h, avec les lignes EVAL2 étiquetées DEV. Le verdict doit être `PASS_NARROW`.
- **Sorties du stager** (`--eval2`) : `manifest.eval2.tsv` (rôle `EVAL2`), `eval2-attribution.tsv` et `eval2-preflight.tsv`. Le manifeste S5h reste octet pour octet celui du témoin (`966eb65f…`).
- **Limite déclarée.** EVAL2 ne contient que des articles scientifiques peS2o. Le DEV S5h mêle Common Corpus, cyber et peS2o. Les valeurs A0 d'EVAL2 ne sont donc pas comparables à celles du DEV. Les critères comparent les bras entre eux, sur EVAL2.

## Run P1

- **Mode.** `sml-gb11-p1-s5h run-h1c` reprend `run-h1b` sans rien changer : même recherche, mêmes configurations, CAL (`doc_index % 16 == 7`) hors de l'entraînement et tracé gelé.
  - En plus, les fichiers d'EVAL2 sont tracés avec le modèle gelé (split 3) dans `<bras>/trace-eval2.bin`, après une passe EVAL2 sans trace qui mesure `eval2_us_per_position`.
  - Les fichiers d'EVAL2 ne servent ni à la recherche ni à l'entraînement.
- **Contrôle de déterminisme.** Les SHA-256 des poids P1 doivent être ceux du run H-1b :
  - `search_q4` : `48135431…` ;
  - `bytes_q4` : `7f4c233d…`.
  
  Sinon, le run s'arrête avant la tête. P1 recalcule son propre BPC DEV dans `metrics.tsv` comme avant. Ce BPC fait partie du contrôle, pas de la tête.

## Bras, pour chaque base

Le code est dans `hyb_h1c_stable.rs` et le binaire `sml-hyb-h1c-s5h`. Ce qui ne change pas par rapport à H-1b :

- les entrées et la perte ;
- CAL-FIT (`% 32 == 7`) et CAL-SEL (`% 32 == 23`) ;
- l'export à échelle (`ScaledHead`) ;
- les grilles τ et λ8 ;
- les graines 11 et 23 et le taux 1/128 ;
- A0c.

- **A0 — P1 seul.**
- **A0c — table booléenne de même taille.** 320 o, remplie sur CAL-FIT, λ choisi sur CAL-SEL.
- **A1 — tête ternaire moyennée.**
  - **Apprentissage.** 4 époques sur CAL-FIT. L'ordre de l'époque e est trié par `splitmix(graine ^ (e << 32) ^ doc)` (l'époque 0 reprend l'ordre H-1b).
  - **Moyenne.** Le maître exporté est la moyenne f64 des instantanés pris après chaque document des époques 1 à 3 (en comptant à partir de 0).
  - **Choix.** Export et choix de (τ, λ) sur CAL-SEL, par graine, comme en H-1b. λ = 0 fait partie de la grille.
  - **Inférence.** `FastHead`, entièrement entière jusqu'à la table de squash, 320 o résidents.

## Critères (figés, pour chaque base, sur EVAL2)

1. **Exactitude.**
   - Sur le Dell, `FastHead` et `ternary_forward_oracle` doivent donner 0 écart sur toutes les positions EVAL2, pour les deux graines.
   - Les 4 têtes sont rejouées sur Thor avec le noyau popcount, qui est comparé à l'oracle sur chaque enregistrement. Le SHA-256 des corrections i32 sur 65 536 enregistrements synthétiques doit être le même que sur le Dell.
2. **Go.**
   - Pour les deux graines, il faut BPC_EVAL2(A1) < min(BPC_EVAL2(A0), BPC_EVAL2(A0c)) − 0,02.
   - Cet écart doit aussi dépasser deux fois l'écart entre les graines.
3. **Coût.**
   - Formule : (`eval2_us_per_position` de P1 + temps tête) / `eval2_us_per_position` ≤ 1,5.
   - Le temps tête est le temps de scoring A1 moins le temps du coût de décision commun, mesuré comme en H-1b.
   - Si ce critère échoue, le verdict devient `GO_DIAGNOSTIC_COST`.
4. **Lecture.**
   - `verdict` = le verdict de `bytes_q4`. Seul un GO sur cette base autorise l'étape H-2, qui demandera encore un accord.
   - Un GO sur `search_q4` seul est rapporté, mais ne lance pas H-2.
5. **Rapport.** `h1c.tsv` publie tout, sans sélection a posteriori :
   - tous les bras, les deux bases et les deux graines ;
   - (τ, λ), le BPC sur CAL-SEL, les pas `unit`, la fraction de poids nuls et le nombre d'instantanés moyennés ;
   - les temps et le ratio de coût ;
   - `dev_read false`.

## Ce que H-1c ne démontre pas

- Ni le routage MOR neuronal (H-2), ni un gain moteur natif (H-3), ni un gain d'énergie.
- Ni une généralisation au-delà d'articles scientifiques peS2o.
- Un GO sur EVAL2 ne dit rien sur le DEV S5h mixte, qui n'est pas relu.

Research only. TEST reste fermé. NOT_PROMOTED.
