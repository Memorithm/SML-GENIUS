# HYB H-2t — reconnaître des templates avant de choisir un expert : étude de faisabilité

**Rédigé le 25 octobre 2026, avant toute exécution sur données réelles.** Cette étude ne rend aucun verdict sur un échantillon réservé. Elle dit seulement si un routage par template (H-2b) vaut la peine d'être préenregistré.

Elle ne lit que la calibration de H-1d (CAL-FIT et CAL-SEL), déjà sur le Dell. Elle n'ouvre ni EVAL4, ni le DEV S5h, ni TEST.

## Pourquoi

H-2 routait chaque octet d'après l'état de P1. Résultat : `NO_GO_CAL_GATE`. L'EM s'est réduit à 2 experts, et 4 têtes au hasard ne faisaient pas mieux qu'une seule. Ajouter de la capacité ne sert donc à rien sans un signal que la tête unique ne voit pas.

Le 25 octobre, le propriétaire a demandé d'évaluer si l'on peut reconnaître des **templates** avant de choisir l'expert. Un template est un gabarit qui revient d'un article à l'autre :
- en-têtes de section ;
- listes de références et citations entre crochets ;
- formules LaTeX ;
- lignes de tableau ;
- mentions légales.

Un template se voit dans la **forme** du texte récent, que les contextes d'octets de P1 ne voient que localement.

Le travail de GPT et Grok sur la même question a été relu :
- **BAF-H4** : une porte ANF sur la tête H-1d, à partir de 8 bits de l'état de P1 ;
- **BAF 0 à 3** : apprentissage de prédicats ANF ;
- **MOR-H4a** : prototypes choisis parmi les états observés.

BAF-H4 ne voit que l'état de P1, c'est-à-dire le signal de H-2. H-2t apporte le signal manquant, la forme du texte. Si l'étude est concluante, H-2b pourra apprendre la règle « template → expert » avec le moteur ANF de BAF, en plus du plus proche prototype.

## Méthode (`sml-hyb-h2-s5h templates`, module `hyb_h2t_templates`)

1. **Signature de forme**, calculée avant chaque octet à partir des seuls octets déjà décodés, donc recalculable par un décodeur. Elle fait 128 bits :
   - classes de caractères (minuscule, majuscule, chiffre, espace, saut de ligne, crochet, symbole mathématique ou de balisage, autre) ;
   - présence des bigrammes de classes et nombre d'octets de chaque classe sur les 64 derniers octets ;
   - classes des 4 derniers octets ;
   - premier caractère de la ligne et colonne ;
   - ligne précédente courte, ou terminée par un point ;
   - texte entre crochets ou entre `$`.

   Les deux premiers octets d'un document ne sont pas tracés par P1 et ne sont donc pas vus.
2. **Templates plats, appris sans étiquettes.** C'est un k-means de Hamming : le centre de chaque groupe est la majorité bit à bit de ses membres. Il part de 1 octet sur 64 de CAL-FIT, avec un départ par points les plus éloignés tiré par la graine, et fait 8 tours.
3. **Stabilité.** On mesure l'accord entre les découpages des graines 11 et 23 sur CAL-SEL, après appariement des étiquettes, pour 4 et 16 templates. On mesure aussi la longueur moyenne des plages d'un même template dans un document : un template doit durer.
4. **Utilité.** On entraîne un expert de la forme H-1d par template (4 templates, apprentissage H-2 sur CAL-FIT). Le choix de (tau, lambda) et l'admission de chaque expert (gain ≥ 0,01 sur P1) se font sur CAL-SEL. On publie aussi :
   - par template, le nombre d'octets, le BPC de P1 et le gain de la tête A1d, pour voir si les templates séparent les régimes ;
   - une **borne oracle** : le meilleur expert pour chaque octet, connu après coup. Ce n'est pas un résultat atteignable.
5. **Recopie.** Un modèle de correspondance LZP (contexte de 12 octets) prédit l'octet qui a suivi la dernière occurrence. Ses taux de réussite par longueur de correspondance sont mesurés sur CAL-FIT. On estime ensuite sur CAL-SEL ce que coûterait le choix, octet par octet, entre P1 et la prédiction de recopie, sur deux variantes : sur tout le flux CAL, et document par document. C'est une estimation optimiste qui sert à dimensionner, pas un codeur.

## Critère de faisabilité (figé ici)

`TEMPLATES_FEASIBLE` exige les deux conditions suivantes. Sinon, le verdict est `TEMPLATES_NOT_FEASIBLE`.
- Pour les deux graines, les experts routés par template gagnent au moins **0,01 BPC sur A1d** sur CAL-SEL. C'est la porte de H-2.
- L'accord entre graines est d'au moins **0,8** à 4 templates.

L'estimation de recopie est publiée à part, sans critère. Le contrôle par hachage à capacité égale est celui de H-2 (A2r : 2,5632 et 2,5575 sur CAL-SEL), produit par le même code d'entraînement.

## Ce qui suit

- **Si l'étude est faisable**, H-2b sera préenregistré :
  - routage par template, avec plus proche prototype et règle ANF de type BAF ;
  - contrôle nul et contrôle par hachage ;
  - porte sur CAL-SEL ;
  - EVAL4 lu une seule fois, après accord du propriétaire.
- **Sinon**, le résultat est publié tel quel, et EVAL4 reste fermé.

## Limites déclarées

- CAL-SEL sert ici à la fois à choisir (tau, lambda), à admettre les experts et à mesurer le gain, comme dans H-2. Le gain est donc optimiste. C'est pourquoi le verdict ne vaut que comme feu vert pour un préenregistrement.
- Il n'y a que des articles peS2o.

Research only. DEV non lu. EVAL4 non lu. TEST fermé. NOT_PROMOTED.
