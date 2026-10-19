# F2 — Préenregistrement : sélecteur TRAIN-only entre W2+Sketch/MITM et Walsh-3
Date de gel 2026-10-19, avant code et première nouvelle mesure F2. Ce document ne modifie pas les seuils/protocoles A6-C12 existants.

## Acquis, négatifs obligatoires et intérêt

C12c : l'hybride W2+Sketch/MITM a retrouvé les 24/24 structures sur n508 sur quatre tâches synthétiques. Le rapport mesure environ 35x de temps de recherche de différence face à W3 sur huit paires de runs, sans borne de complexité générale.
C12g/h : sur la parité XOR3 avec bruit label de 40 %, W3 a retrouvé 16/16 cas à 32768 bits alors que l'hybride a retrouvé 0/16 ; une détection n'utilisant que le classement pairwise peut perdre tout signal. C12d/e/f ont déjà étudié alias corrélés/temporels, bruit d'état et équivalence native. La référence FWHT exacte F1 est distincte de la recherche distribuée.

## F2a — contrat du sélecteur Rust

TrainOnlySpectralMethodGate ingère des observations comparatives de deux méthodes déjà entraînées sur les mêmes cibles TRAIN, avec deux partitions TRAIN_calibration disjointes. Chaque observation : partition 0/1, nombre d'exemples, erreurs hybrides, erreurs Walsh3, temps de recherche hybride ns, temps Walsh3 ns. Aucun index du vrai triplet, oracle caché, donnée DEV ou TEST n'est fourni. Les erreurs évaluent des sorties d'un réseau gelé contre des labels autorisés de TRAIN, pas la récupération des paramètres enseignants. Le harnais doit attester les frontières TRAIN_fit et TRAIN_calibration et les SHAs source : l'API seule ne peut le prouver.

La politique admet W3 uniquement si les deux partitions dépassent une marge positive d'erreur par cible en ppm et si le coût ns/exemple W3 respecte le plafond explicite sur chaque partition. Sinon elle admet Hybrid uniquement si son coût et son taux d'erreur demeurent sous les plafonds sur chaque partition. Sinon Abstain. Égalité/tie favorise Hybrid si qualifié ; évidence absente = Abstain. Les paramètres — minimum de cas par partition, gain minimal en ppm, plafond de taux d'erreur et plafonds de coût — sont fixés à la création, validés et jamais réglés à partir des données DEV/TEST. Une décision freeze est irréversible.

Rejeter sans mutation partielle : partition invalide, exemples nuls, erreurs > exemples, temps nul, configuration invalide, dépassement de compteur, observations post-freeze. Calculer les marges de taux d'erreur par produit entier (u128) et non par f64 approximatif. La politique est une proposition de routage, pas la preuve qu'un modèle a appris une parité.

## F2a — tests de décision

- Les deux partitions préfèrent W3 avec marge, W3 dans le budget et erreur admissible : Walsh3Exhaustive.
- Hybrid et W3 équivalents : HybridW2SketchMitm seulement si admissible ; W3 trop coûteux : Hybrid.
- W3 ne gagne que dans une partition : Hybrid ou Abstain, jamais promotion W3.
- Échantillons insuffisants, coûts et erreurs hors limites : Abstain.
- Overflow, erreurs > N, partition inconnue, paramètres impossibles, second freeze, mutation post-freeze : erreurs ou abstention sans falsifier les compteurs.
- Les tests Rust synthétiques ne sont PAS une étude de qualité linguistique ni une nouvelle campagne LPN.

## F2b — campagne indépendante, à implémenter après F2a

Comparer Hybrid figé, Walsh3 figé, W2, Sketch-MITM, route coût seul, route F2a, Gauss sur parité propre seulement et oracle représentable, sur identiques cibles, budget de modèle comparable et quatre sources synthétiques (XOR3, AND→XOR→XOR, nul, distrateurs). Inclure n=32/128/508, labels propres et bruit de 20/30/40 %, nouvelles graines et splits TRAIN_fit, TRAIN_cal1, TRAIN_cal2, VAL, TEST strictement gelés. W3 à n508 doit avoir un plafond de durée et mémoire ; dépassement explicite NOT_EVALUATED_BUDGET, jamais un succès implicite. Reporter coût de calibration des DEUX modèles, coût du routeur, validation et inférence, RSS, p50/p95, faux positifs, abstentions et erreurs natives de prédiction. Garder tous les négatifs et intervalles par flux/graine. Un test sur les mêmes graines historiques de C12g/h n'est qu'une vérification de régression, pas un nouveau TEST indépendant.

## STOP

Pas d'oracle des vraies sources injecté dans la recherche ; pas de choix après consultation TEST ; pas de gain proclamé si le temps économisé exclut la calibration W3 ; ne pas appeler réussite universelle LPN. Corpus SEC G0 NOT_ADMITTED et 2024 TEST UNOPENED. Rust-only, aucune dépendance NVIDIA SDK, RemoteOps sert à qualifier les runners Dell et Thor sans transfert de données sensibles.

**Après l'acceptation de F1, réaliser F2a comme PR de code autonome, puis F2b avec résultats appariés.**
