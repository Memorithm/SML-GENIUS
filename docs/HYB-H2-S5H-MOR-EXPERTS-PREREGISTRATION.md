# HYB H-2 — routage MOR entre têtes expertes H-1d, jugé sur un échantillon neuf : préenregistrement

**Gelé le 25 octobre 2026, avant toute exécution H-2 et avant toute lecture d'EVAL4.** Le 25 octobre 2026, après le GO de H-1d, le propriétaire a demandé de continuer « sur le reste » pour obtenir un ensemble complet démontré : H-2 (routage MOR), puis H-3 (moteur natif).

Ce document fait suite à `RESULTS-HYB-H1D-S5H-BANK-HEAD.md` et `RESULTS-HYB-H1D-COST.md` (H-1d : GO, gain de 0,159 et 0,152 BPC sur EVAL3, coût 1,17×). Il ne révise aucun verdict publié.

## Question

L'option B prévoit des experts neuronaux ternaires, choisis octet par octet par un routeur MOR booléen. H-1d a montré qu'**une** tête ternaire qui voit chaque banque améliore P1. H-2 demande :

> Plusieurs têtes de même forme, avec un routeur booléen appris qui envoie chaque octet à une seule d'entre elles, font-elles mieux qu'une seule tête, et mieux que les mêmes têtes routées au hasard, pour un coût par octet presque inchangé ?

Le témoin à routage par hachage sépare l'effet du routage de celui de la capacité (4 têtes au lieu d'une).

## L'échantillon EVAL4

- **Source.** Le fichier peS2o-0004 (épinglé `b1208e9e…`). Ni le stage S5h (qui s'arrête dans peS2o-0001), ni EVAL2 (peS2o-0002), ni EVAL3 (peS2o-0003) ne l'ont ouvert.
- **Règle, filtres, doublons, taille et audit.** Les mêmes que pour EVAL2 et EVAL3 :
  - seuls les articles du seau DEV de la découpe S5h entrent ;
  - un article déjà présent dans S5h (identifiant, texte exact ou normalisé) est exclu ;
  - on s'arrête à 32 Mio ;
  - l'audit S4d sur le manifeste S5h, avec les lignes EVAL4 étiquetées DEV, doit rendre `PASS_NARROW`.
- **Sorties du stager** (`--eval4`) : `manifest.eval4.tsv` (rôle `EVAL4`), `eval4-attribution.tsv` et `eval4-preflight.tsv`. Le manifeste S5h reste celui du témoin (`966eb65f…`).
- **Limite déclarée.** EVAL4 ne contient que des articles peS2o.

## Réutilisation du run H-1d

H-2 ne réentraîne pas P1. Il réutilise, sur le Dell :

- **le run P1 de H-1d** (`sml-gb11-p1-h1d-s5h-d8ca5c39…`) : `trace-cal.bin`, son fichier compagnon `.banks.bin`, `trace.json` et `weights.bin`. Les sommes SHA-256 publiées doivent être vérifiées avant usage. Les poids doivent être `7f4c233d…` ; le programme le vérifie lui-même.
- **les deux têtes A1d de H-1d** (`b77e3f13…` pour la graine 11, `3c6192e8…` pour la graine 23), inchangées, comme contrôle A1d. Le programme vérifie leur SHA-256 et que `h1d.tsv` est un run terminé sur les mêmes poids P1.

Le CAL est le même que pour H-1d : CAL-FIT (`doc_index % 32 == 7`) pour l'entraînement, CAL-SEL (`% 32 == 23`) pour la sélection. EVAL2 et EVAL3 ne sont pas relus, et le DEV S5h n'est jamais ouvert.

**Étape EVAL4 de P1, seulement si la porte est passée :** `sml-gb11-p1-s5h trace-h2-eval4`. Le modèle gelé est restauré depuis `weights.bin`, une passe sans trace mesure `eval4_us_per_position`, puis EVAL4 est tracé (split 5) avec son fichier compagnon, et le programme écrit `eval4.json`. Ces nouveaux fichiers sont ajoutés au dossier P1 de H-1d, sans toucher aux fichiers existants.

## Bras

Le code est dans `hyb_h2_mor_experts.rs` et le binaire `sml-hyb-h2-s5h`. Chaque expert a exactement la forme de la tête H-1d : mêmes entrées (172 bits sur 3 mots), même perte, même pas, même taux, même initialisation par graine, 4 époques permutées, moyenne de Polyak des époques 2 à 4, même règle d'export et mêmes grilles τ et λ8.

- **A0 — P1 seul.**
- **A1d — une tête H-1d (contrôle).** Les têtes publiées de H-1d, inchangées.
- **A2 — 4 experts derrière le routeur appris.**
  - **Bits de routage :** 128 bits connus avant de décoder l'octet : le mot de contexte de P1 et les thermomètres de banques à la profondeur 0.
  - **Routeur :** un prototype de 128 bits par expert. L'octet va à l'expert dont le prototype est le plus proche en distance de Hamming (égalité : le plus petit indice). Coût : 8 popcounts par octet.
  - **Apprentissage du routeur, EM dur sur CAL-FIT :**
    1. Groupes initiaux : quartiles du |stretch| de P1 à la profondeur 0 sur CAL-FIT. Prototypes initiaux : majorité bit à bit de chaque groupe.
    2. À chaque tour, chaque octet de CAL-FIT est routé par les prototypes courants, puis un maître moyenné est entraîné par expert sur ses octets. L'entraînement voit donc toujours le routage d'inférence.
    3. Entre deux tours, chaque octet est étiqueté par l'expert dont le maître f32 le code le mieux, et chaque prototype devient la majorité stricte bit à bit de ses octets (un groupe vide garde son prototype).
    4. 2 tours de reroutage, soit 3 entraînements. Les experts exportés sont ceux du dernier entraînement.
  - Le routeur initial est commun aux deux graines ; l'expert `k` part de la graine `graine ^ (k << 40)`.
- **A2r — les mêmes 4 experts derrière un routeur par hachage (contrôle).** L'octet va à l'expert `splitmix(contexte ^ constante) mod 4`. Les experts sont entraînés une fois sur ce routage, avec la même procédure.
- **Sélection.** Pour A2 et A2r, un seul couple (τ, λ8), commun aux 4 experts d'un bras, est choisi sur CAL-SEL, comme en H-1d.
- Graines 11 et 23.

## Critères (figés, sur `bytes_q4`)

1. **Étape 1 : porte CAL-SEL, avant toute lecture d'EVAL4** (`sml-hyb-h2-s5h gate`).
   - Pour chaque graine, gain CAL-SEL = BPC_CAL-SEL(A1d) − BPC_CAL-SEL(A2).
   - Si ce gain est inférieur à 0,01 pour au moins une graine, le verdict est `NO_GO_CAL_GATE`. EVAL4 n'est ni tracé ni lu, et reste disponible. Sinon, le verdict d'étape est `CAL_GATE_PASS`.
   - `sml-hyb-h2-s5h run` refait cette étape (elle est déterministe ; les experts jugés doivent être identiques octet pour octet à ceux de la porte), puis lit EVAL4 une seule fois.
2. **Exactitude.**
   - Sur le Dell, sur toutes les positions EVAL4 et pour les deux graines, le routeur et son oracle scalaire, ainsi que le noyau popcount de l'expert choisi et son oracle, doivent donner 0 écart.
   - Les routeurs et experts A2 des deux graines sont rejoués sur Thor. Le SHA-256 des routes et des corrections i32 sur 65 536 enregistrements synthétiques doit être le même que sur le Dell.
3. **Go.** Pour les deux graines, sur EVAL4 :
   - BPC(A2) < BPC(A1d) − 0,01 ;
   - BPC(A2) < BPC(A2r) − 0,005 : le routage appris compte, pas seulement la capacité ;
   - BPC(A1d) − BPC(A2) > 2 × l'écart entre les graines de A2.
4. **Coût.**
   - Formule : (`eval4_us_per_position` de P1 + temps routeur et expert fusionné) / `eval4_us_per_position` ≤ 1,5.
   - Le temps routeur et expert est le temps de routage plus le scoring par le noyau fusionné (`FusedBankHead`) de l'expert choisi, moins le temps du coût de décision commun, mesuré comme en H-1d-cost.
   - Si ce critère échoue, le verdict devient `GO_DIAGNOSTIC_COST`.
5. **Lecture.**
   - GO : le routage MOR est démontré sur ce moteur ; H-3 (moteur natif) peut être préenregistré.
   - NO_GO ou NO_GO_CAL_GATE : H-3 se fera avec une seule tête H-1d, qui est déjà GO.
   - Le gain A0 − A1d sur EVAL4 est publié comme réplication de H-1d sur un deuxième échantillon neuf. C'est un diagnostic : il ne change pas le verdict H-2.
6. **Rapport.** `h2.tsv` publie tout, sans sélection a posteriori :
   - les deux graines, A0, A1d, A2 et A2r, sur CAL-SEL et (si la porte passe) sur EVAL4 ;
   - les bornes des quartiles initiaux, le routeur initial et les routeurs finaux ;
   - le nombre d'octets par expert à chaque tour et sur EVAL4 ;
   - (τ, λ), la fraction de poids nuls, le nombre d'instantanés moyennés ;
   - les temps et le ratio de coût ;
   - `dev_read false` et `eval4_read`.

## Ce que H-2 ne démontre pas

- Ni un gain moteur natif (H-3), ni un gain d'énergie.
- Ni une généralisation au-delà d'articles scientifiques peS2o.
- Ni un routage par page au sens du stockage SML : le routage est fait octet par octet, sur l'état de P1.

Research only. TEST reste fermé. NOT_PROMOTED.
