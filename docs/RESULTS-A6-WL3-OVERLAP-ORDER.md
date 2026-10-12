# A6-WL3 — Rapport : monômes ANF chevauchants, cardinalité cachée, certificats de Möbius

**12 octobre 2026 — première campagne réussie, préenregistrée, exécutée en Rust natif.**

Le [préenregistrement A6-WL3](A6-WL3-OVERLAP-ORDER-PREREGISTRATION.md) a été commité **avant toute implémentation ou mesure** au SHA `bf11496c1cabb19c22c9ab66fe37dee2da8398df`. Première exécution dédiée complète : [GitHub Actions run 38235045274 / job 114763533440](https://github.com/Memorithm/SML-GENIUS/actions/runs/38235045274/job/114763533440), commit source `c6316264e3ec27d0d86fbf9adc57f872672b33ab`, **SUCCESS**. Le code, les résultats négatifs et les conclusions sont publiés sans modifier les critères après TEST. Avant merge, la CI générale et cette même expérience doivent aussi repasser au SHA documentaire final.

## 1. Hypothèses et frontière scientifique

A6-WL1 avait besoin de **quatre sélecteurs d'entrées connus** pour retrouver des supports cachés. A6-WL2 a supprimé ce privilège, mais supposait **quatre monômes disjoints**. A6-WL3 supprime le **non-chevauchement** ainsi que la connaissance exacte du **nombre de monômes** : l'apprenant reçoit seulement `k`, `max_terms=4`, `min_degree=2`, `max_degree=5`, les budgets et une callback d'oracle TRAIN actif. Il ne voit jamais la structure du générateur, les graines des supports, ni VAL/TEST pendant le fit.

La famille demeure très contrainte :
`f(x)=⊕_{S∈T} ∏_{i∈S}x_i`, `|T|∈{2,3,4}`, coefficients GF(2) unitaires, **littéraux positifs**, degrés 2..5. Les ensembles S peuvent se recouper ou être emboîtés. Le nombre réel de termes est **inconnu**, mais le plafond de 4 et la borne de degré 5 sont **communiqués à l'algorithme**. Aucune absence de représentation de fonctions de degré supérieur n'est extrapolée à un apprentissage universel.

Les 63 supports cachés distincts proviennent des trois familles suivantes :
- **STAR4** : quatre monômes avec un hub partagé, degré [2,3,4,5] pour k≥16 ou quatre degrés 2 pour k8.
- **CHAIN3** : trois monômes en chaîne, [2,3,4] pour k≥16 ou trois degrés 2 pour k8.
- **NESTED2** : deux monômes emboîtés, de degrés 2 et 5, indépendamment de k.

La bijection entre positions de rôle et indices physiques a été construite par Fisher–Yates seedé **dans le générateur séparé**, avant de passer uniquement une callback de valeurs à l'apprenant. Aucun rôle/monôme n'est transmis au fitter. Il s'agit de topologies synthétiques conçues pour la falsification, pas de fonctions aléatoires uniformes ni de langage.

## 2. Avancée algébrique : pourquoi la minimisation seule ne suffit plus

Pour une ANF sur des supports chevauchants, une affectation résiduelle positive peut satisfaire **trois termes à la fois** et demeurer positive à cause de la parité. Exemple `x0x1 ⊕ x0x2 ⊕ x0x3` sur `x0=x1=x2=x3=1` : le résidu vaut 1, mais mettre à zéro n'importe quelle variable individuelle le fait devenir 0. Une simple stratégie de suppression locale pourrait prendre **à tort** `{0,1,2,3}` pour un monôme inexistant.

Le nouveau module Rust `anf_wide_overlap_learn.rs` combine :
1. recherche de témoins uniformes Bernoulli 1/2 dans le résidu `f(x) XOR ⊕_{termes déjà appris} monôme(x)` ;
2. réduction adaptative par suppression de groupes, en maintenant une sortie résiduelle égale à 1 ;
3. **certification GF(2)** des candidats de taille `d≤5` à partir des `2^d` sous-ensembles, avec transformée de Möbius `c_U=⊕_{V⊆U} f_res(1_V)`. Seuls des coefficients **algébriquement vrais** égaux à 1 peuvent être admis : un groupe de monômes co-actifs n'est pas assimilé à un coefficient absent ;
4. soustraction des termes certifiés du résidu, nouvelle recherche d'un monôme, puis **512 essais aléatoires résiduels négatifs** pour déclarer l'arrêt probabiliste. Même après le quatrième terme, l'algorithme teste la présence d'un cinquième : un résidu positif déclenche une erreur `TooManyTerms`, sans troncature silencieuse.

La transformée de Möbius est exacte **pour des réponses d'oracle exactes** ; en présence de bruit, 21 répétitions à majorité stricte corrigent probabilistiquement les inversions Bernoulli(1/16). Le vote majoritaire conserve une probabilité de réponse erronée non nulle (environ `1,11×10^−8` par requête logique sous indépendance des flips), donc « certificat » ne veut pas dire zéro risque.

**Arrêt par zéro résiduel :** pour un polynôme GF(2) **non nul et de degré ≤5** sous des entrées uniformes indépendantes, la distance minimale de Reed–Muller impose `P(f_res(x)=1)≥1/32`; 512 tirages négatifs ont une probabilité de faux arrêt au plus `(31/32)^512≈8,72×10^−8` par arrêt **avec oracle sans erreurs**. C'est une borne **conditionnelle** et non un résultat sur données passives corrélées, réponses bruitées adversariales, degré supérieur à cinq ou polynômes arbitraires sans contrainte de degré.

## 3. Population, gel TRAIN/VAL et mesure TEST

Sept fan-in [8,16,32,64,128,256,1024] × trois familles [STAR4, CHAIN3, NESTED2] × trois graines × deux panels TRAIN [clean/noise1/16] : **126 apprentissages gelés**, mais **63 enseignants distincts** car les panels bruité/propre partagent une vérité structurale. VAL est constitué de **544 observations par fit** (512 uniformes Bernoulli 1/2 et 32 configurations composées à partir des supports vrais **exclusivement dans le banc de validation**). Les propositions ne changent pas après consultation de VAL. Les **126 décisions ont été gelées avant la première génération TEST**.

TEST = **4 096 entrées indépendantes de TRAIN/VAL par fit**, soit **516 096 comparaisons binaires**, parmi lesquelles les flux sont appariés entre panels ayant le même enseignant : seulement **63 streams TEST distincts**. Le cas k8 fournit en outre **18 tests exhaustifs de 256 entrées**, soit 4 608 comparaisons supplémentaires. L'apprenant exécute lui-même les portes proposées via `WideAnfGate` en mode scalar natif; les validations comparent directement les bits de prédiction, et non le seul nombre d'erreurs contre des étiquettes.

| Critère préenregistré | Mesure |
|---|---:|
| Politiques structurelles gelées avant TEST | **126/126** |
| Nombre de termes **et** supports exacts (2,3,4 termes) | **126/126** |
| Validation indépendante avec zéro erreur | **126/126** |
| TEST bit à bit, sans aucune divergence | **0/516 096 erreurs** |
| Vérités exhaustives k8 | **18/18**, zéro désaccord sur 4 608 affectations |
| Contrôles hors grammaire rejetés | **7/7** |
| Porte k1024 **réellement apprise** et insérée dans SBG | **16/16** sorties exactes |

La campagne n'a observé **aucun candidat de minimisation finalement rejeté** sur les enseignants positifs sélectionnés : les 126 fits ont obtenu 126 décisions VAL zéro, et les certificats de Möbius ont servi à authentifier les monômes, pas à « sauver » une série d'échecs. Il ne faut pas présenter ces expériences comme une étude empirique exhaustive des configurations XOR adversariales ; le test unitaire contient néanmoins un **contre-exemple explicite de coalition à trois monômes** dont un support faux est écarté par Möbius.

## 4. Coût physique, nombre de termes, arrêt et ressources

Le compteur mesure chaque lecture de l'oracle, y compris votes bruités, requêtes de bissection, sous-masques Möbius et tests d'arrêt. Il ne mesure pas des lectures de corpus naturel ou de véritables requêtes vers un service externe.

**Moyennes sur les 21 configurations largeur×graine par famille/panel** :

| Famille / termes réels | Requêtes logiques propres | Requêtes physiques bruitées (×21) | Nombre de certificats de Möbius | Temps propre | Temps bruité |
|---|---:|---:|---:|---:|---:|
| STAR4 / quatre | **801,4** | **16 830** | 4 | 0,2221 ms | 0,4676 ms |
| CHAIN3 / trois | **685,8** | **14 401** | 3 | 0,1879 ms | 0,3805 ms |
| NESTED2 / deux | **674,5** | **14 164** | 2 | 0,1774 ms | 0,3245 ms |

**À k=1024 uniquement**, trois graines par famille/panel :

| Famille | Requêtes logiques | Observations bruitées (21×) | Fit CPU propre | Fit CPU bruité |
|---|---:|---:|---:|---:|
| STAR4 | **978,7** | **20 552** | 0,7974 ms | 1,2610 ms |
| CHAIN3 | **799,7** | **16 793** | 0,6778 ms | 1,0248 ms |
| NESTED2 | **753,7** | **15 827** | 0,6578 ms | 0,8695 ms |

Toutes les **126/126 politiques** ont utilisé `zero_residual_probes=512` pour l'arrêt. Le maximum de requêtes physiques constaté par politique est **22 470**, inférieur au budget préfixé **160 000**. La somme sur les 126 politiques est **998 690 lectures physiques d'oracle synthétique**. Les fits des familles k1024 comptent **2 à 4 monômes GF(2) actifs**, sans allocation de table `2^1024` ; les payloads logiques varient sur ce corpus entre **8 544 et 8 960 octets**, selon l'arbre de préfixes et les supports, avec 6 à 15 mots `u64` de scratch. Ces chiffres excluent la mémoire du générateur de TRAIN, les vecteurs temporaires de recherche et l'allocateur.

**Ne pas confondre le coût local de fit en millisecondes avec celui de 20 000 interventions réalisables sur des données du monde réel**. Aucun débit inference GPU, aucune propriété de généralisation linguistique et aucune complexité sous-cubique universelle ne sont mesurés ici.

## 5. Sept contrôles négatifs : refus avant VAL, pas camouflage

Pour chaque k, une fonction STAR4 seed0 est prolongée par un **cinquième monôme de degré 6** sur les six premières positions de rôle permutées. Le monôme excède le maximum appris de quatre termes et son degré excède les cinq autorisés. L'apprenant aurait pu retourner silencieusement une ANF approchée après quatre termes ; cela est interdit par son contrôle de résidu final.

**Résultat : les sept enseignants négatifs (k8,16,32,64,128,256,1024) échouent tous explicitement pendant TRAIN avec `TooManyTerms`.** Il n'y a donc **aucune erreur VAL mesurée sur les négatifs**, car aucun modèle n'a été retourné : les logs montrent `fit_rejected=true`, `validation_errors=None`, `accepted=false` pour 7/7. Cela ne valide **pas** le comportement contre toutes les fonctions hors prior, notamment celles dont le résidu de degré très élevé devient si rare que 512 essais ne le détectent pas. La borne `(31/32)^512` ne s'applique **pas** à ces négatifs de degré 6.

## 6. Vrai pont avec les pages SBG/MOR

Le fit k1024 STAR4 seed0 propre fournit une **porte WideAnfGate effectivement construite à partir des monômes récupérés**, non à partir des coefficients véritables de l'enseignant. Cette porte est installée dans `WideAnfPageAtlas<1024>` avec entrées `Value[0..1023]`, destination `Meaning[0]`, et une `MorDecision` de `PageId(23)`. **Seize états SBG** distincts (zéro, un, monômes seuls, entrées pseudo-aléatoires) sont exécutés réellement sur le backend WideSparseANF ; les 16 sorties sont identiques à l'enseignant natif indépendant, les traces confirment PageId(23).

Le routage est **fourni explicitement**, pas appris. Le chemin MOR/Delta-KV couplé existe par PR #499, mais A6-WL3 ne simule pas de multiples transitions Delta-KV pendant l'apprentissage : c'est une future gate de recherche.

## 7. CI, corrections transparentes et décision

Run scientifique initial complet : [GitHub Actions `overlap-learning`](https://github.com/Memorithm/SML-GENIUS/actions/runs/38235045274/job/114763533440), commit `c6316264e3ec27d0d86fbf9adc57f872672b33ab`, **SUCCESS** sur `cargo fmt --check`, `cargo check --locked -p sml-core --all-targets`, `cargo clippy --locked -p sml-core --all-targets -- -D warnings`, tests unitaires de l'algèbre et du binaire et reproduction RELEASE des 126 expériences. La CI spécifique contrôle les comptes des 126 FROZEN/TEST, des 7 négatifs, de la page SBG, et exige le total PASS. La CI générale Rust doit être verte sur **SHA final après documentation**.

Les échecs CI initiaux concernaient **le formatage rustfmt**, **une assignation d'initialisation immédiatement écrasée** et **un import inutilisé du benchmark**, tous corrigés avant les mesures. Ils ne modifient ni grammaire, ni graines, ni seuil, ni données, ni algorithme évalué. Rust 1.89 épinglé sur runner GitHub-hosted Ubuntu 24.04, checkout exact SHA `persist-credentials:false`, droits `contents:read`, aucun service résident, runner RemoteOps privé, code unsafe ou SDK NVIDIA propriétaire.

**H1/H2 PASS 126/126 sur cette population synthétique, H3 PASS 7/7 négatifs spécifiques.** L'extension vers des monômes de degré inconnu, supports génériques denses, interactions négatives, observations passives/récurrentes, banques ANF massives ou SML-25B n'est pas qualifiée.

**Prochaine validation prioritaire :** apprentissage depuis des **observations passives** dans une boucle récurrente, avec analyse informationnelle de la couverture des contextes rares, contre-exemples d'identifiabilité et coûts de collecte. Une seconde priorité est l'estimation du nombre de monômes avec degré non borné, pour laquelle les 512 essais de résidu et la borne actuelle sont insuffisants.
