# WideSparseANF ↔ SBG/MOR/Delta-KV — qualification préenregistrée

**Gel : 12 octobre 2026, après la fusion vérifiée de la PR #498 (`65008e66e8f0e85818dc8fc238b6e91025d49b73`), avant la première implémentation et la première mesure de cette tranche.**

## Objectif vérifiable

La PR #498 a livré un vrai exécuteur `WideAnfGate` bit-slicé et `WideAnfNetwork` récurrent jusqu'à au moins 1 024 entrées, mais le moteur existant `LargeAnfPageAtlas` reste limité à 31 bits d'adresse. La présente tranche crée un **atlas de pages WideSparseANF natif** interopérant avec les **vraies décisions `MorDecision`**, les coordonnées `SbgCoordinate` des quatre champs d'état, la boucle `RoutedSemantic<K,S>` et la mémoire `DeltaKvDynamicRekeyKernel<S,Q>`. Elle ne transforme pas automatiquement les routes PVP6 historiques.

H1 — Une page contenant une **porte AND de 1 024 entrées**, activée à partir d'une décision MOR, lit bien 1 024 coordonnées de l'état SBG et écrit un résultat exact. Un autre expert non sélectionné ne modifie aucune coordonnée. Vérifier le résultat sur entrées toutes à 1, puis avec un zéro à la position 0, 512, 1023 (au minimum). Mesurer la propriété **snapshot** : si deux pages sont sélectionnées et l'une écrit un bit que l'autre lit, l'autre doit lire la valeur **avant** l'exécution des deux pages, et non l'écriture provisoire.

H2 — L'atlas **rejette sans mutation** : ID introuvable, ID en double, écriture de même destination par deux pages, plus de pages/gates que le budget, coordonnées invalides/dupliquées, scratch insuffisant, monôme invalide et dépassement de trace. Un échec ne doit pas consommer le nombre de gates/routes/cycles/lectures Delta-KV. Le nouvel atlas ne modifie aucune API de `LargeAnfPageAtlas` ni `Pvp6Bank`.

H3 — Une boucle réelle **`RoutedSemantic → WideAnfPageAtlas → DeltaKvDynamicRekeyKernel`** doit produire à chaque cycle une trace de décision MOR, une trace des pages exécutées et une trace de mémoire à **la même profondeur**. À état et contexte identiques, son résultat est **bit pour bit identique** à l'exécution manuelle de ces trois étapes avec les mêmes objets natifs et les mêmes paramètres. Une erreur de mémoire est **atomique** pour les compteurs, les objets owned et les historiques de tous les étages. Des sorties non sélectionnées sont préservées.

## Interfaces, coûts et sécurité

- `WideAnfPageAtlas<S>` : pages avec `PageId`, `Vec<WideAnfGate>`, `Vec<SbgCoordinate>` de sources partagées (vues physiques des portes déjà compilées), et `Vec<SbgCoordinate>` de destinations. Toutes les entrées des portes sont indexées **dans la liste des sources de page** ; le nombre de sources peut atteindre S=1024 et plus sous budget explicite.
- L'évaluation SBG possède un booléen par coordonnée : **diffusion** de chaque valeur en un mot `u64::MAX` ou `0` sur 64 voies, puis appel du **vrai chemin `WideAnfGate::evaluate_lanes`**, et extraction du bit 0 de chaque sortie. Cette diffusion teste le chemin natif u64 ; elle **ne qualifie pas** l'exécution de 64 états SBG distincts dans le même appel.
- Budget cumulatif préfixé par constructeur : **max_selected**, **max_gates**, **max_trace_words**. Tous les ID et destinations sont validés *avant* toute écriture, puis l'état est cloné et modifié localement ; un appel fautif ne change ni l'état source ni les historiques partagés.
- `WideRoutedExperts` : composition avec l'actuel `RoutedSemantic<K,S>` (aucune réimplémentation d'algorithme de routage), et nombre de gate-evaluations/trace words relevé par cycle.
- `WideAnfDeltaKvLoop` : séparation claire de l'état post-expert et de l'état post-mémoire, sources route courante et Delta-KV réaffectés sur **clones candidats**, avec commit des objets et traces seulement si les trois étapes réussissent. Conservation des erreurs typées.
- **Corpus fixe de tests** : 4 tests H1 (toutes entrées à 1 + chaque zéro sur 0/512/1023), 1 test snapshot multi-pages, 4 cas de rejet H2 (missing/duplicate/conflict/zero budget), 2 cycles manuels appariés H3 avec un vrai `RoutedSemantic` et `DeltaKvDynamicRekeyKernel`, et un test de rollback après échec du Delta-KV. Le total de tests Rust peut être supérieur à ces minima.
- Ne pas invoquer de runners privés; GitHub-hosted Ubuntu 24.04, Rust 1.89, checkout du SHA exact, permissions `contents:read`. Chaque étape doit passer formatage, Clippy `-D warnings`, tests, tests globaux et traçabilité de sortie.

## Limites

La PR #498 est le préalable qualifié, mais ces tests restent **des pages câblées à la main**, non un entraînement appris d'un monôme sparse à fan-in 1024, ni une banque de 1 048 576 pages. L'atlas n'est pas automatiquement le runtime choisi par les routes de production ; l'adoption doit être explicite. Pas de mesure de GPU, pas de perplexité ni débit token d'un SML-25B, pas de preuve de pire cas asymptotique.

**Fusion :** uniquement après PR créée, mesures/test positifs ou négatifs consignés, CI spécifique et globale vertes au SHA final, et rollback démontré.
