# Préenregistrement — apprentissage actif de monômes ANF sparse à grande largeur (A6-WL1)

**Gel : 12 octobre 2026, après fusion de la PR #499, avant toute implémentation ou mesure A6-WL1.** Branche `research/anf-wide-active-membership-learning`. Aucun choix de paramètres ou de seuil ne sera optimisé à partir des jeux TEST.

## Hypothèse scientifique et limite fondamentale

Les PR #498–499 qualifient l'**exécution** de monômes ANF à fan-in au moins 1024, leur intégration MOR/SBG/Delta-KV et leur rollback. Elles ne qualifient pas l'**apprentissage** du support des monômes. L'identification d'une ANF booléenne arbitraire à `k=1024` reste sans garantie générale de coût polynomial.

Cette tranche cible explicitement un **sous-espace identifiable grâce à une intervention contrôlée** : une somme XOR de **quatre conjonctions**, chacune conditionnée par un sélecteur public distinct. L'architecte connaît les positions des sélecteurs `x0..x3`, mais **ni les variables des conjonctions cachées, ni leur taille exacte**. Pour l'évaluation de porte, `f(x)=⊕_{a=0..3} (x_a ∧ ∧_{j∈H_a} x_j)`, `H_a ⊆ {4..k−1}`, `H_a` non vide. Tous les littéraux sont positifs. Les sélecteurs sont une **hypothèse structurelle forte et documentée** analogue à des entrées de routage connues ; elle n'est pas une solution à l'apprentissage ANF sans prérequis.

**H1 :** un apprenant Rust `WideAnfAnchoredLearner` récupère **exactement les quatre monômes**, leurs supports cachés et la fonction bit-à-bit, pour `k∈{8,16,32,64,128,256,1024}`, trois graines et deux panels d'observation d'entraînement (propre et bruité) : **42/42** configurations, après validation et sur TEST inconnu au moment du fit. Aucun masque caché, vérité des monômes, seed du générateur, accès à VAL ou TEST n'entre dans le learner, seulement un **oracle de requêtes actif** fournissant l'étiquette de l'entrée soumise pendant TRAIN. Ceci est **apprentissage par membership queries/interventions**, pas apprentissage passif de documents.

**H2 :** la variante TRAIN bruitée retourne une réponse de l'oracle inversée indépendamment avec `p=1/16` ; chaque requête logique est répétée **15** fois et décidée à majorité stricte pour réduire les erreurs. La version propre ne répète chaque requête qu'une fois. Les 42 réussites sont une cible préfixée et un éventuel échec doit être rapporté, sans changer les répétitions, le bruit ou les graines.

**H3 :** une fonction hors de cette grammaire, comportant les quatre termes précédents **et** le terme d'interaction `x0∧x1`, conserve les réponses aux interventions **one-hot** mais doit être **rejetée par la validation sur 16 combinaisons des sélecteurs**, pour les 7 largeurs. Ne pas prétendre que tous les hors-grammaire sont détectables. La **vérification indépendante des résultats** sera distincte de la procédure de découverte.

## Algorithme gelé

Pour chaque sélecteur `a`, soumettre une entrée **one-hot** sur les sélecteurs, toutes les autres variables initialisées à 1. La sortie doit valoir 1. Désactiver ensuite par groupes des positions non sélectrices et observer si la sortie devient 0. Ce test équivaut à « le groupe intersecte-t-il le monôme caché ? » uniquement sous la grammaire annoncée. Découper récursivement en deux moitiés, conserver les variables découvertes. C'est une **recherche adaptative par groupe** ; coût en requêtes logiques borné par le nombre de littéraux et `log₂(k)` *pour cette famille*, pas une complexité garantie pour les ANF arbitraires.

Après les quatre supports, construire réellement un `WideAnfGate` avec `WideAnfGate::from_monomials`. Aucune adaptation des données après un résultat de validation ou de TEST. Le learner doit journaliser le nombre de requêtes logiques, de requêtes physiques (répétitions comprises), les supports effectivement récupérés et les octets de représentation/scratch déclarés. Erreurs fail-closed sur largeur, sélecteurs dupliqués/inexistants, nombre de sélecteurs, répétitions impaires non autorisées, budgets de requêtes, degré maximal et erreurs d'exécution de l'oracle.

**Budgets figés :** `max_fan_in=2048`, `max_selectors=4`, `max_physical_queries=20000`, `max_hidden_literals_per_term=8` ; backend `WideAnfLimits::new(2048,256,8192,8192,8192)`. Le learner prend une **callback Rust d'oracle** et n'a pas de pointeur vers les monômes cachés. L'observateur est utilisé uniquement sur TRAIN et ne reçoit pas le booléen `is_truth` du modèle ; la vérité sert rétrospectivement dans le banc.

## Génération et échantillonnage

Pour chaque largeur et graine `0,1,2`, générer un polynôme de quatre monômes, un par sélecteur `0,1,2,3`. Les degrés cachés ciblés sont `[2,3,4,5]` variables non sélectrices, plafonnés à `k-4` ; supports choisis de façon pseudo-aléatoire seedée **sans remise**, parmi les positions `4..k-1`. Les supports se chevauchent librement *entre* sélecteurs, mais chaque monôme reste distinct grâce à son sélecteur. Générer les labels depuis un `WideAnfGate` enseignant séparé ; fournir à l'apprenant **uniquement une fermeture évaluant les requêtes**.

Trois flux RNG distincts pour positions cachées, bruit de TRAIN et VAL/TEST :
`HIDDEN=0xA6B1_1001_51ED_0001`, `NOISE=0xA6B1_1002_51ED_0002`, `VAL=0xA6B1_1003_51ED_0003`, `TEST=0xA6B1_1004_51ED_0004`. Chaque graine dérivée de `(seed+1).wrapping_mul(0xD1342543DE82EF95) ^ width as u64 ^ salt`. Les panels propre/bruité partagent volontairement le même enseignant par (largeur,graine) : **42 fits mais seulement 21 supports cachés distincts**, pas 42 populations indépendantes.

VAL créé après fit de chaque panel, sur **512 points aléatoires**, plus **16 configurations des quatre sélecteurs avec toutes les autres entrées à 1**, et **16 configurations avec toutes les autres entrées à 0** = **544 points par fit**. La validation ne modifie pas la porte proposée : elle **accepte seulement un score erreur 0**. Pour le test négatif H3, sélectionner un enseignant possédant le terme hors-grammaire `x0&x1`, conserver les interventions TRAIN one-hot identiques, puis comparer sur exactement les 16 combinaisons de sélecteurs avec les autres variables à 1 ; **7/7 rejets** attendus.

**TEST créé uniquement après gel de toutes les 42 propositions et des 42 décisions VAL** : **4 096 entrées aléatoires par fit**, label issu du modèle enseignant séparé, compteur de désaccord bit-à-bit sur la sortie du `WideAnfGate` appris. **Pour `k=8`**, aussi contrôle exhaustif des **256 entrées** pour les 6 fits largeur 8. Les résultats par graine/panel doivent être publiés, y compris les échecs.

## Pont réel MOR/SBG

Pour la configuration k1024, graine 0, panel propre, prendre la **porte réellement construite par le learner** et l'insérer dans un `WideAnfPageAtlas<1024>` avec 1024 sources `SbgCoordinate::Value` et une sortie `Meaning[0]`. Fournir une `MorDecision` sélectionnant cette page et contrôler au moins **16 états SBG** dont les entrées originales one-hot / tous bits à 1 / zéro. Vérifier à chaque fois le résultat contre l'enseignant. Ce test couvre **l'exécution d'une porte apprise dans une page sélectionnée**, pas un entraînement end-to-end de MOR ou de Delta-KV.

## Mesures et refus de sur-interprétation

Enregistrer **42 lignes `WIDE_LEARN_FROZEN`**, **42 lignes `WIDE_LEARN_TEST`**, **7 lignes `WIDE_LEARN_NEGATIVE`**, **1 `WIDE_LEARN_SBG`**, les budgets réellement consommés, temps CPU fit, et statut des 256 tests exhaustifs k8. `H1/H2 PASS` requièrent **42/42** supports exacts, zéro erreur sur VAL et TEST, et six vérifications exhaustives réussies. `H3 PASS` requiert 7/7 rejets négatifs. Tout budget dépassé ou contre-exemple entraîne un journal explicite puis une qualification négative.

Runner GitHub-hosted Ubuntu 24.04, Rust 1.89 épinglé, `permissions: contents:read`, checkout du SHA exact sans identifiants persistants, aucun runner RemoteOps privé, aucun SDK NVIDIA, `#![forbid(unsafe_code)]`. Les anciens moteurs/API restent utilisables. La PR ne sera fusionnée qu'avec CI spécifique et CI générale vertes sur HEAD final, rapport scientifique et bootstrap/roadmap actualisés.

**Hors portée :** recherche arbitraire de monômes de degré élevé sans sélecteur connu, entrées uniquement passives, apprentissage de centaines de millions de portes, découverte autonome de liaisons MOR ou qualité linguistique SML-25B. Ces recherches doivent avoir un protocole distinct.
