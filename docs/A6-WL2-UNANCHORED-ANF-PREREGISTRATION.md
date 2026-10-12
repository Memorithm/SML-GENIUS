# A6-WL2 — préenregistrement : ANF sparse sans sélecteurs connus

**Gel le 12 octobre 2026, avant tout code, test, ou résultat A6-WL2.** Dépendance : A6-WL1 PR #500 fusionnée sur `main` (`d585b9601df1083e20306e4d0f5189ecc8570369`).

## Problème scientifique réel

A6-WL1 apprend une conjonction inconnue **par sélecteur connu** `x0..x3` grâce à des requêtes one-hot. Une porte ANF de quatre monômes n'a toutefois **aucun sélecteur canonique** : les facteurs d'un AND sont commutatifs. Demander de retrouver le nom du « vrai sélecteur » pour un monôme est une attribution non identifiable. La nouvelle cible est l'**ensemble non ordonné des supports complets** `S₁..S₄`, sans donner un seul index spécial à l'agent.

Hypothèse structurelle annoncée et nécessaire : `f(x)=AND_{j∈S₁} x_j XOR ... XOR AND_{j∈S₄} x_j`, avec **exactement 4 monômes positifs**, supports **non vides, pairwise disjoints** et degrés `2..5` inclus, inconnus du learner. Les supports disjoints interdisent un chevauchement confondant l'isolation des monômes. Ni le nombre de positions par monôme ni les positions spécifiques ne sont donnés. L'algorithme reçoit uniquement `k`, le nombre de monômes attendu (4), la borne de degré (5), les budgets et une callback Rust de **requêtes actives TRAIN** qui renvoie une étiquette booléenne ; il n'a pas de source de vérité sur la structure ni accès à VAL/TEST. La recherche n'est **pas** une identification universelle d'ANF sparse, pas un apprentissage passif ni un apprentissage sans interventions.

## H1/H2 et populations figées

**H1 :** 7 largeurs `k=8,16,32,64,128,256,1024`, 3 graines distinctes `0,1,2`, 2 panels TRAIN (réponses propres ou flips indépendants Bernoulli(1/16)) = **42 politiques**, soit **21 enseignants/sup­ports réels distincts**, partagés entre les panels. Objectif préfixé **42/42 supports non ordonnés exactement récupérés**, **42/42 VAL sans erreur**, **42/42 TEST à zéro bit de désaccord**, plus **6/6** tests de vérité exhaustifs k8 sur 256 entrées. Tous les résultats sont publiés même si ces seuils ne sont pas atteints.

Les degrés cachés sont [2,2,2,2] pour k8, puis [2,3,4,5] pour k≥16. Pour chaque largeur et graine, un Fisher–Yates déterministe permute les `k` positions avec `SplitMix64`; les quatre monômes reçoivent sans remise les tranches consécutives de longueur prescrite. Les monômes sont ensuite triés (variables locales croissantes ; liste de monômes lexicographique). Ils peuvent toucher n'importe quelle variable parmi les `k` entrées ; aucune entrée privilégiée n'est désignée. Cette distribution est **construite pour l'identifiabilité**, pas un modèle représentatif de la langue naturelle.

**H2 :** panel propre 1 observation par requête logique, panel bruité **21** répétitions par requête logique, verdict majorité stricte. Probabilité d'inversion physique Bernoulli(1/16), générée par PRNG indépendant `stream XOR 0xA6B1_2002_51ED_0002`. Un échec de majorité est possible en théorie ; ne pas le dissimuler. Budget par apprentissage : `max_fan_in=2048`, `terms=4`, `max_degree=5`, `max_physical_queries=25 000`, `max_witness_attempts_per_term=512`; compilation : `WideAnfLimits::new(2048,256,8192,8192,8192)`.

### Algorithme préfixé — extraction des termes par témoin positif minimal

1. Maintenir une liste `known` de supports déjà isolés, initialement vide. Une requête **résiduelle** vaut `f(x) XOR (XOR des monômes déjà isolés évalués localement sur x)`. **Un seul** oracle existe dans le fitter; le résidu est calculé exclusivement à partir de réponses TRAIN et de supports appris, jamais à partir des vraies règles.
2. Tirer les bits du candidat `x` indépendamment Bernoulli(1/2), avec PRNG candidat interne seedé transmis au learner (`(seed+1)*0xD1342543DE82EF95 XOR width XOR 0xA6B1_2005_51ED_0005`), distinct des vérités et du bruit ; tenter **au plus 512** témoins par terme. Accepter un témoin si le résidu répond 1.
3. À résidu=1, tenter de désactiver **un groupe d'indices** (les bits actuellement à 1), puis garder l'effacement si le résidu reste 1 ; sinon restaurer ce groupe et subdiviser récursivement en moitiés jusqu'aux bits isolés. Répéter le parcours de groupes **tant que la taille du support diminue**, au plus `k` fois ; l'appelant contrôle le nombre exact de requêtes et aucune réduction laissant résidu=0 n'est gardée.
4. Accepter un support minimal candidat uniquement s'il est de longueur `2..5`, inédit et sous budget. Les 4 monômes sont construits dans un vrai `WideAnfGate` natif via `from_monomials`. Les supports seront comparés *comme ensembles*, sans ordre ou annotation de « sélecteur ».
5. Raison de la restriction : sous **supports positifs disjoints** et témoin résiduel positif, les termes actifs sont de cardinal impair (1 ou 3 pour quatre termes initiaux). Trois termes actifs impliquent ≥6 bits vrais puisque chaque degré ≥2 ; le cap fixé à 5 empêche d'accepter cette fausse isolation, même si une minimisation locale s'arrête sur une configuration à trois termes. Un candidat plus large peut être abandonné et un autre tiré, jamais requalifié après TEST. Le raisonnement cesse de valoir pour monômes chevauchants, négations ou autre grammaire.
6. Le fitter ne reçoit ni un ensemble de requêtes prédéfini couplé au générateur ni des labels de référence. Chaque rappel à l'oracle compte une **requête physique**, y compris en cas d'échec. En cas de budget épuisé ou d'incapacité d'isoler un monôme, échouer explicitement.

## Validation et TEST, deux séparations effectives

Pour chaque politique TRAIN (et avant TEST), comparer la porte proposée au générateur dans un banc hors du learner sur une VAL de **544 requêtes** : 512 aléatoires Bernoulli(1/2) à seed `VAL=0xA6B1_2003_51ED_0003`, plus 32 contrôles dont état tout zéro, tout un, un monôme isolé et combinaisons de supports (rejouées/étendues déterministiquement). Les positions vraies sont utilisées **uniquement par le banc pour constituer les contrôles de VAL**, jamais par l'apprenant. Le modèle ne peut être accepté que si erreurs VAL=0, sans mise à jour des monômes en réaction à VAL.

**Tous les 42 fits et décisions VAL doivent être gelés avant la première génération TEST.** TEST : **4096 entrées aléatoires Bernoulli(1/2)** par largeur×graine, PRNG `TEST=0xA6B1_2004_51ED_0004` ; les panels partagent les mêmes entrées TEST, mais sont des fits indépendants sur bruit TRAIN différent. k8 : les 256 requêtes possibles en oracle indépendant, pour chacun des six modèles. L'égalité TEST est la comparaison **bit à bit des sorties**, pas l'égalité de deux nombres d'erreurs.

## H3 — contrôle négatif hors grammaire

Pour chaque largeur, générer un enseignant supplémentaire : les quatre monômes disjoints fixés pour seed0, **plus un cinquième monôme de degré k**, `AND(x0..x(k−1))`, hors borne de degré 5 et hors cardinalité 4. Soumettre au même ajusteur avec budget et nombre de termes 4. Considérer comme **rejet** soit une erreur fail-closed du learner, soit une proposition avec **au moins un désaccord sur les 544 contrôles VAL**, dont **l'entrée tout-un** qui rend le monôme ajouté observable. **7/7 rejets exigés** ; publier séparément les échecs au fit et les échecs VAL. Ne pas considérer tous les autres modèles hors grammaire comme détectables.

## Mesures et pont SBG

Publier 42 lignes `WIDE_UNANCHORED_FROZEN`, 42 `WIDE_UNANCHORED_TEST`, 7 `WIDE_UNANCHORED_NEGATIVE`, plus une `WIDE_UNANCHORED_SBG` : la porte **apprise** pour k1024 seed0 panel propre est insérée dans `WideAnfPageAtlas<1024>` avec un `MorDecision` désignant explicitement sa `PageId`; vérifier **16** états SBG distincts sur entrées générées séparément contre l'enseignant natif. La route est fournie, pas apprise. Rapporter par width/panel : supports corrects, budget logique/physique, retries, temps CPU local, octets de payload et scratch, erreurs VAL/TEST, et structure du dispositif négatif. Pas de réglage après observation de ces sorties.

Sécurité : Rust 1.89 épinglé, CI read-only GitHub-hosted Ubuntu 24.04, checkout SHA exact sans credentials persistants, pas de runner RemoteOps, pas de SDK NVIDIA autre que les pilotes GPU, pas de `unsafe`. Ne fusionner qu'après rapport vérifiable et CI spécifique+générale verte au SHA final.

**Exclusions explicites :** supports chevauchants ou emboîtés, fonctions avec termes négatifs, portes passivement identifiées, degré >5, nombre de monômes inconnu, sources temporelles, langage, 25B, million de pages et 64 états SBG distincts à la fois.
