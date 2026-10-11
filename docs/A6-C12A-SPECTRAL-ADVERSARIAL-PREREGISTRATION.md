# A6-C12a — Préenregistrement du banc adverse spectral (avant toute exécution)

**Date de gel : 12 octobre 2026. Statut : protocole fixé avant la création du code et avant les premières mesures A6-C12a.**

**Erratum arithmétique post-exécution (sans modification du protocole)** : la première version indiquait à tort 80 politiques. Le produit correct **4 tâches × 2 panneaux × 5 graines × 2 longueurs TRAIN × 2 budgets** vaut **160 politiques**. Le programme a dès sa première exécution gelé et évalué 160 politiques ; seule la vérification de cardinalité de la CI contenait 80. La correction de ce décompte n'altère aucun générateur, algorithme, seuil, donnée, budget ou résultat. Première exécution de référence : GitHub Actions run 38182344480, job 114597361506 (échec du seul contrôle de cardinalité après résultats complets).

Dépendance : A6-C11D (PR #491, fusionnée). Cette étape **n'augmente pas la capacité du modèle**, ne modifie pas l'algorithme sélectionné pour la production et n'évalue ni la qualité linguistique ni SML-25B. Tous les changements expérimentaux après ce gel devront être explicitement signalés comme amendements et les mesures antérieures conservées.

## Hypothèse réfutable

Dans l'anneau connu N=128 avec trois portes internes inconnues, la proposition A6-C11D peut réussir sur des signaux fortement corrélés avec un Walsh d'ordre faible, tout en échouant sur une fonction **représentable** dont les corrélations recherchées sont nulles en population.

Le code A6-C11D effectue un classement avec (i) corrélation marginale `z⊕x`, (ii) interaction `z⊕x⊕R_past`, et (iii) le même score après un **unique peeling XOR** de la variable marginale dominante. Ce peeling n'est **pas** une élimination de Gauss GF(2) ni une résolution générale des parités. Ne pas présenter son éventuel échec comme un échec de l'apprentissage affine en général.

## Famille native et témoins (capacité identique)

```text
s0(t+1) = s127(t) XOR x1(t)                 [fixe]
si(t+1) = si-1(t)                           [identité sauf trois portes]
sj(t+1) = f(sj-1(t), x2(t))                 [j=19]
sk(t+1) = g(sk-1(t), x3(t))                 [k=67]
sl(t+1) = h(sl-1(t), x4(t))                 [l=108]
y(t) = x0(t) XOR s0(t+1)                    [fixe]
```

Tables 2 entrées codées sur 4 bits : XOR=6, AND=8. Les quatre tâches suivantes sont **toutes représentables** par exactement la grammaire de trois portes, sans aucun dépassement de capacité :

1. `parity_xor3` : [XOR, XOR, XOR] = `R_past⊕a⊕b⊕c`. Pour des variables indépendantes uniformes, tous les coefficients Walsh portant sur moins de 4 variables sont nuls.
2. `masked_and_xor_xor` : [AND, XOR, XOR] = `(R_past∧a)⊕b⊕c`. Cas mixte représentable. Les corrélations unitaires et les interactions du type `z⊕x⊕R_past` sont nulles en population ; certaines corrélations d'ordre 2 entre les **autres variables** restent non nulles, donc ne pas dire « Walsh d'ordre 2 nul » sans préciser les variables.
3. `historical_xor_and_xor` : [XOR, AND, XOR] : témoin positif A6-C11D.
4. `and_and_and` : [AND, AND, AND] : témoin positif non linéaire A6-C11D.

Les positions et sources vraies ne sont jamais fournies à `fit_three_gate_diagnostic` ou au classeur ; elles ne sont accessibles qu'au générateur et aux métriques **rétrospectives**. Les quatre générateurs sont exécutés comme réseaux ANF natifs, pas comme étiquettes calculées par une formule de substitution. Une vérification algébrique exhaustive sur les 16 combinaisons des quatre entrées valide les assertions spectrales ci-dessus, indépendamment des graines.

## Trois scores comparés, sans changement de sélection en production

- **W1 :** Walsh marginal brut `|corr(z,x)|`.
- **W1+R :** `max(|corr(z,x)|, |corr(z,x,R_past)|)`.
- **W1+R+P :** score exact A6-C11D (interaction racine passée + peeling XOR unique).

Même classe de 508 sources possibles (127 positions × quatre canaux), mêmes données pour les trois scores. Les vrais paramètres ne sont utilisés dans aucun score ni tri. Chaque score produit indépendamment un top24 et un classement des triplets admissibles ; le moteur A6-C11D existant continue de sélectionner uniquement par W1+R+P, budgets 64 et 640, sans réglage à partir des résultats.

## Échantillonnage, budget, indépendance et évaluation

- N=128, cinq canaux, 64 voies `u64`, entrées uniformes et indépendantes.
- Quatre tâches × deux panneaux (propre, corruption indépendante des labels Bernoulli 1/16) × cinq graines = **40 jeux TRAIN/VAL indépendants au niveau graine-tâche-panneau**.
- Deux longueurs TRAIN **512 et 2048**, la première étant le préfixe exact de la seconde ; validation indépendante 512, test indépendant 4096 pas.
- **240 observations de classement** = 4×2×5×2×3 scores, **160 propositions apprises** = 4×2×5×2 et **40 TEST distincts** partagés entre les deux longueurs.
- Budget de classement : exactement les 508 sources, top24, les triplets à positions distinctes ; classement du triplet véritable **dans ce pool**, ou `absent`. Mesurer sa présence au rang ≤64 ou ≤640. Les 64/640 concernent toujours les structures évaluées en TRAIN complet, pas les éléments du top24.
- Test indépendant créé **après gel de toutes les propositions** ; aucune décision basée sur TEST. Mesurer les erreurs du réseau natif proposé, du réseau retenu après validation et du générateur natif sur le même flux TEST. Aucune modification du score ou des seuils après lecture de TEST.
- Toutes les erreurs et toutes les graines sont publiées, y compris les échecs. Les observations de classement sous longueurs et scores différents **ne sont pas indépendantes**.

## Critères préfixés, statistiques et séparation des échecs

**Critère principal de classement** pour chaque tâche × panneau × TRAIN × méthode : proportion des cinq graines ayant **les trois véritables sources dans le top24**. Rapporter obligatoirement aussi top8, top80, les trois rangs **1-based**, médiane et pire rang, rang du triplet dans top24 ou absence, et présence ≤64/≤640.

**Critères secondaires d'apprentissage** : récupération exacte de la configuration (positions, sources et tables), équivalence fonctionnelle mesurée sur TEST natif autonome, comparaison à l'oracle natif, et taux de validation-acceptation. La récupération fonctionnelle ne doit pas être confondue avec l'unicité des tables : plusieurs factorisations ANF peuvent être équivalentes.

**Grille causale** : source absente du top24 => limite du score/filtrage ; sources présentes mais triplet rang>budget => limite du budget ; triplet évalué mais pas de composition récupérée => limite factorisation/rescoring ; proposition correcte mais rejetée => limite de validation. Aucun de ces quatre générateurs ne peut être qualifié « hors capacité » du modèle trois portes.

**Arrêt / extension** : ne modifier ni top24, ni budgets, ni choix des tâches/graines après observation. Les XOR à 4/5 nouvelles sources exigent une capacité ≥4/5, donc sont explicitement réservés à une étape ultérieure avec oracle de représentabilité. Les distracteurs corrélés et variables masquées sont réservés à C12b et doivent recevoir une analyse d'identifiabilité dédiée ; ne pas les introduire rétroactivement.

## Exécution et sécurité

Rust natif 1.89, sans bibliothèque GPU propriétaire, sans technologie logicielle NVIDIA. GitHub-hosted uniquement, permissions lecture seule, aucun code PR non fiable sur les runners Memorithm ou RemoteOps. Exécutable dédié et tests d'identité spectrale. Résultats synthétiques uniquement : aucune extrapolation vers SML-25B, banques ANF de millions de fonctions ou débit GPU.

**Garde scientifique :** PR en brouillon jusqu'à publication des valeurs observées et validation CI ; ne pas fusionner en tant que preuve de récupération si les logs montrent l'inverse.
