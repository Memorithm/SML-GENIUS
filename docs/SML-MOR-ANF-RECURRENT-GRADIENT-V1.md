# SML-1 : véritable couplage d'apprentissage MOR + ANF GF(2) + récurrence

Date : 16 octobre 2026. Objectif de la campagne : passer de la banque à routage haché figé à une **chaîne d'apprentissage causale**, avec gradients aux experts paginés, aux poids de routage différentiables et à une petite cellule récurrente.

Le premier moteur supervisé paginé sur Dell a démontré 4 194 304 paramètres f32 de capacité, 460 219 paramètres réellement touchés, 330 608 792 mises à jour, mais sans routage appris ni récurrence. Le second a démontré 50 000 896 paramètres f32 de capacité, seulement 484 724 effectivement touchés, perte F32 hors documents 4,982964361 bpc, Q4 5,176565574 bpc (+0,193601214). **Ces essais ne démontrent pas une compétence générale, ni un modèle 25B.**

## Architecture implémentée en Rust (cette PR)

- **Experts** : deux pages candidates distinctes par bit, calculées par deux fonctions de hachage sur les deux octets causaux précédents. Les poids experts sont les poids **effectivement persistés** du `SparseGradientAtlas`, conservés par cache LRU borné. Chaque expert est évalué et reçoit son propre gradient de la vraie perte de mélange. Ce mécanisme n'est pas un router hard-discrete appris : les candidats sont encore hachés, mais leur pondération est entraînée.
- **Routeur MOR** : 256 buckets de quatre poids réels, paramètres entraînés avec la dérivée exacte de la **probabilité mélangée** : `p=g*p0+(1-g)*p1`, `g=sigmoid(theta.dot([1,anf,h,bit_sign]))`. Le gradient `dL/dg` est propagé aux coefficients du routeur et aux deux experts ; compter séparément la capacité (1024 scalaires), les paramètres ayant réellement changé et le nombre de mises à jour.
- **Algèbre booléenne ANF** : feature GF(2) `(a0*b0) XOR ... XOR (a7*b7)`, conjonsctions non linéaires avec vraie activation `AND`. Ce support ANF est **fixe** ; ses coefficients dans le routage et dans la transition récurrente sont appris. **Aucune affirmation d'apprentissage de la topologie des circuits ANF** : ce sera le palier suivant.
- **État récurrent** : `h_next=tanh(alpha*h+beta*bit_density(b)+gamma*ANF(a,b))`, remise à zéro à la frontière des documents. Les trois paramètres de la transition sont entraînés par éligibilités **forward-mode RTRL** `dh/d(alpha,beta,gamma)` et le gradient provenant de la sortie MOR. Ce signal ne traverse pas l'indexation discrète des deux candidats. Comme les paramètres évoluent en ligne, le calcul décrit correctement une approximation RTRL en ligne et non un BPTT exact de tous les pas historiques.
- **Perte** : entropie croisée binaire sur chaque bit prédit, émise en bits/octet ; mise à jour SGD sur la dérivée de la perte en nats (écart constant `ln(2)` absorbé par le taux d'apprentissage). La validation est *read-only* sur les paramètres, les caches peuvent être chargés sans modifier les poids.
- **Provenance et reprise** : sauvegarder l'atlas paginé SHA-256 existant ; fichier du routeur, activation et trois paramètres de récurrence dans une structure versionnée ; empreintes liant les deux fichiers au même état. Refus si dimension/configuration incompatibles, corruption, symlink ou changement d'identifiants.
- **Sécurité** : aucun `unsafe`, aucune dépendance GPU propriétaire, cache explicitement limité, aucun accès à l'octet cible dans les features causales. Attribution et admission juridique des fichiers SEC : **G0 NOT_ADMITTED**.

## Preuves et seuils scientifiques

1. **Test de dérivée récurrente** : comparaison du Jacobien RTRL analytique à une différence finie sur une séquence de quatre étapes sans mise à jour des paramètres ; erreur absolue < 2e-5.
2. **Test de convergence sur motif** : le même modèle part de p=0,5 pour les experts. Après plusieurs passes sur un motif répétitif, la perte sur une séquence non modifiée doit diminuer significativement (<80% perte initiale), avec paramètres réellement touchés chez les experts, le routeur et la récurrence.
3. **Test de provenance** : évaluation read-only, checkpoint puis réouverture bit-identique de la perte, altération d'un octet du routeur provoquant le rejet.
4. **Ablations à conduire** : comparer à corpus et nombres d'exemples **égaux** la référence 4M sans MOR, sans ANF, sans récurrence, routeur seul, et routeur+ANF+récurrence. Maintenir TEST 2024 fermé et utiliser les rapports 2023 indépendants après pré-enregistrement du protocole.
5. **Objectif large** : monter à 50M experts ensuite, vérifier non seulement la capacité mais **les paramètres effectivement modifiés**, la latence I/O et la mémoire ; introduire sélection top-k *apprise* et circuits ANF adaptatifs si les ablations montrent des gains reproductibles. SML 25B reste un objectif ultérieur : ne pas présenter cette cellule minuscule comme 25B.

Un score en bits/octet de rapports financiers ne constitue pas une preuve de capacité à anticiper les cours boursiers ou de compétence conversationnelle bilingue.
