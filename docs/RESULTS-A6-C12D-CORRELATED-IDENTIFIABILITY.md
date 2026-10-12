# A6-C12d — Robustesse sous distracteurs corrélés et frontière d'identifiabilité

**Statut : expérience préenregistrée exécutée, résultats positifs bornés et témoins négatifs conservés.** Le préenregistrement scientifique `docs/A6-C12D-CORRELATED-IDENTIFIABILITY-PREREGISTRATION.md` a été commité **avant toute implémentation ou mesure** au SHA `3c39a5093f18d425b8889694ae90e85a8818bda0`. La première campagne complète a été exécutée sur le commit source `2a27816bf5d33942d4f11867572663e117ae620f` : [GitHub Actions A6-C12d — run 38186966600 / job 114611264102](https://github.com/Memorithm/SML-GENIUS/actions/runs/38186966600/job/114611264102). Format/check/Clippy strict/tests du banc : **SUCCESS**, puis **80/80 sélections gelées, 80/80 TEST indépendants de la sélection et 24/24 preuves-oracle évaluées**. Les TEST sont générés seulement une fois les 80 modèles gelés.

## 1. Question et modèle réellement expérimenté

Il ne s'agit pas d'une grammaire libre ni de 508 états. **L'automate natif a 128 bits d'état**, une racine/lecture et un anneau de délais connus, avec **exactement trois portes cachées**. Les **508** caractéristiques sont les 127 positions possibles des portes internes multipliées par les quatre canaux bruts possibles. Deux tâches : `XOR→XOR→XOR` et `AND→XOR→XOR` ; trois graines, deux panneaux (propre, labels avec bruit Bernoulli 1/16) et deux modes de corrélation. L'apprenant ne reçoit pas les paramètres cachés du générateur : ces valeurs servent exclusivement à construire les labels synthétiques et aux diagnostics rétrospectifs.

Les masques de corrélation utilisent un flux PRNG distinct des entrées brutes et du bruit de labels :

- **NEAR_ALIAS** : `x1=x2 XOR e1` et `x3=x4 XOR e2` avec `e1,e2~Bernoulli(1/32)` indépendants. Sur les 12 TRAIN mesurés, le taux de différence moyen vaut **3,1189 %** pour x1/x2 et **3,1069 %** pour x3/x4. Les canaux sont donc fortement corrélés, sans être identiques.
- **EXACT_ALIAS** : `x1=x2` sur chaque pas et chaque voie ; x3/x4 restent indépendants (différence observée **50,0341 %** en moyenne). Pour la première porte j20, lire x1 ou x2 est **observablement équivalent**. L'identification **du nom du canal d'origine** n'a alors pas de solution unique, même avec davantage de données, sur ce domaine d'entrées.

Les deux configurations sont **appariées sur les graines brutes**. Il existe 12 cellules tâche×panneau×graine, donc **12 jeux TEST par mode et 24 datasets au total pour les deux modes**, appariés sur les graines de base : ce ne sont pas 24 répliques indépendantes.

## 2. H1 — Forte corrélation, sources encore identifiables : réussite 12/12

À n=508, chaque ligne correspond à trois graines ; succès « exact » = les trois positions, trois canaux et trois tables booléennes originaux ainsi qu'une erreur native TEST strictement identique à celle du générateur.

| NEAR_ALIAS | W2 seul exact/oracle | Sketch-MITM seul exact/oracle | **Hybride exact/oracle** | W3 témoin exact/oracle |
|---|---:|---:|---:|---:|
| XOR³, propre | 0/3 | 3/3 | **3/3** | 1/1 |
| XOR³, labels bruités | 0/3 | 3/3 | **3/3** | 1/1 |
| AND→XOR→XOR, propre | 3/3 | 0/3 | **3/3** | 1/1 |
| AND→XOR→XOR, labels bruités | 3/3 | 0/3 | **3/3** | 1/1 |
| **Total** | **6/12** | **6/12** | **12/12** | **4/4** |

Le seuil **≥2/3 graines par tâche et panneau**, déclaré avant la campagne, est dépassé avec **3/3 sur chacune des quatre cellules**. La complémentarité des deux méthodes est maintenue malgré les distracteurs : Sketch-MITM retrouve la parité pure ; W2 retrouve la composition AND/XOR ; leur union retrouve les deux.

Pour vérifier que NEAR_ALIAS ne représente **pas** une impossibilité d'identification, le second réseau natif au premier gate `j20/x1` plutôt que `j20/x2` est exécuté sur les mêmes **entrées TEST à labels propres**. Les désaccords mesurés se situent entre **73 043 et 74 571 bits** sur 262 144 pour la famille XOR³ (six flux) et entre **7 461 et 7 709 bits** pour AND→XOR→XOR (six flux). L'écart n'est pas une preuve formelle d'identifiabilité universelle, mais la différence de prédictions est directement observable sur les TEST utilisés.

## 3. H2 — Aliases stricts : non-identifiabilité structurelle démontrée

**Témoin théorique :** si `x1(t)=x2(t)` pour tout t et toute voie, remplacer dans une porte interne `f(s,x2(t))` par `f(s,x1(t))`, sans modifier le reste du réseau, laisse chaque transition d'état et chaque sortie identiques par récurrence sur t. Les deux configurations ont des paramètres différents mais aucune expérience sur ce domaine de données ne peut les distinguer.

Le banc a instancié les **deux modèles natifs distincts** et a comparé leurs prédictions en utilisant les mêmes entrées TEST, avec des labels propres pour isoler la différence entre générateurs. Résultat : **12/12 discordances nulles**, soit zéro bit de différence dans chacun des 12 flux (4096 pas×64 voies). Ce constat valide le témoin expérimental ; l'impossibilité de lever l'ambiguïté découle de l'égalité des entrées et ne dépend pas des 12 observations.

| EXACT_ALIAS | W2 : équivalence TEST | Sketch seul : équivalence TEST | **Hybride : équivalence TEST** | W3 témoin : équivalence TEST |
|---|---:|---:|---:|---:|
| XOR³, propre | 0/3 | 3/3 | **3/3** | 1/1 |
| XOR³, labels bruités | 0/3 | 3/3 | **3/3** | 1/1 |
| AND→XOR→XOR, propre | 3/3 | 0/3 | **3/3** | 1/1 |
| AND→XOR→XOR, labels bruités | 3/3 | 0/3 | **3/3** | 1/1 |
| **Total** | **6/12** | **6/12** | **12/12** | **4/4** |

Dans ce mode, le nombre de récupérations des **canaux d'origine à l'identique** est **0/12 pour l'hybride** et **0/4 pour W3** ; les trois tables booléennes sont néanmoins exactes **12/12 chez l'hybride**, et les prédictions autonomes reproduisent exactement l'erreur TEST de l'oracle. **Ce zéro de récupération des noms n'est pas un échec d'apprentissage**, mais une démonstration que l'égalité des paramètres n'est pas un critère valide lorsque les sources sont non identifiables. Les méthodes isolées échouent, elles, à récupérer la fonction complète sur leur famille adversariale : W2 sur XOR³, Sketch sur AND/XOR.

## 4. H3 — Mesures de coût, avec même protocole

Moyennes **sur 12 flux par méthode et mode** pour W2/Sketch/Hybride ; le W3 exhaustif ne couvre **que quatre flux par mode**, seed0 de chacune des deux tâches et deux panneaux. Temps de fit TRAIN + factorisation/validation, Rust 1.89 RELEASE, runner GitHub-hosted x86_64 ; pas des performances Dell, Thor, GPU ou de génération de tokens.

| Mode | W2 total | Sketch total | **Hybride total** | W3 exhaustif total |
|---|---:|---:|---:|---:|
| **NEAR_ALIAS** | 135,89 ms | 33,80 ms | **168,95 ms** | 5 921,44 ms |
| **EXACT_ALIAS** | 135,60 ms | 34,24 ms | **169,08 ms** | 5 946,54 ms |

Ratios **strictement appariés** aux quatre mêmes flux W3 de chaque mode :
- NEAR_ALIAS : W3 **5 921,4435 ms** / Hybride **169,24925 ms** = **34,99×** observé ;
- EXACT_ALIAS : W3 **5 946,53525 ms** / Hybride **168,8995 ms** = **35,21×** observé.

Le coût du moteur hybride reste dominé par la partie W2+factorisation : environ **82 ms de criblage et 87 ms de factorisation/validation**. Sketch seul ≈33–34 ms et n'évalue intégralement que **256** candidats par flux après environ **62 500–62 900** structures distinctes retrouvées par collisions de hash, mais ne reconnaît pas le terme AND/XOR. Une recherche exhaustive W3 calcule **21 336 000 corrélations complètes** par configuration.

Ne **pas** transformer 256/21 336 000 en gain de vitesse : W2 et les millions de recherches dans les seaux ont un coût CPU, et les collisions peuvent croître de façon défavorable. Les gains 34,99× et 35,21× n'ont été établis que sur huit appariements synthétiques et **ne sont pas une preuve de complexité sous-cubique de pire cas**.

## 5. Provenance, reproductibilité, limitations

Premiers résultats complets : commit `2a27816bf5d33942d4f11867572663e117ae620f`, [workflow GitHub-hosted](https://github.com/Memorithm/SML-GENIUS/actions/runs/38186966600). Toutes les 80 lignes `A6C12D_FROZEN`, 80 lignes `A6C12D_TEST`, 24 `A6C12D_ALIAS_PROOF` et 24 `A6C12D_INPUT` ont été émises ; les tests n'ont pas caché le zéro de récupération structurelle dans EXACT_ALIAS. Le premier passage CI a nécessité deux corrections purement statiques (formatage et références de compteurs), puis une annotation localisée d'une variante utilisée uniquement pour tests de régression. Aucun générateur, seuil, graine, méthode ou évaluation de données n'a été changé. Vérifier séparément la CI générale finale avant fusion.

Rejouer :

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --bin sml-correlated-identifiability-bench
cargo run --locked --release -p sml-core --bin sml-correlated-identifiability-bench -- 3
```

**Limites non levées :** deux tâches seulement et trois graines, distributions spécifiquement construites, étiquettes propres ou bruit indépendant, 128 bits d'état et un anneau connu à trois portes. Des variables masquées, bruits corrélés dans le temps, déséquilibres forts, graphes libres, k=4/5 et apprentissage réel de fonctions ANF massives ne sont pas testés. Il n'existe ici aucune mesure de débit token, perplexité, efficacité VRAM ou performance d'un SML-25B. Aucune promotion de la méthode hybride en moteur général n'est justifiée.

## 6. Décision scientifique

**H1 : PASS dans le périmètre ; H2 : PASS de la démonstration de non-identifiabilité et de la récupération fonctionnelle ; H3 : des coûts appariés sont publiés, avec environ 35× d'écart observé sans revendication de borne.**

Prochain banc à préenregistrer : généraliser aux distracteurs temporellement décalés et distributions biaisées, définir formellement les **classes d'équivalence observationnelle** pour éviter les pénalités de sources interchangeables, tester des perturbations d'état et sélection adaptative des budgets. Toute tâche à quatre ou cinq portes exige explicitement une grammaire d'apprentissage qui accepte quatre ou cinq mutations, avec contrôles de représentabilité avant scoring.
