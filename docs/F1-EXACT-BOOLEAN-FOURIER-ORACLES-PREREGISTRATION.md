# F1 — Référence exacte et bornée Fourier–Walsh × Möbius / ANF (préenregistrement)

**Gel avant code/mesure : 2026-10-19.** Tranche F1 de la [feuille de route](SML-FOURIER-WALSH-ANF-ROADMAP-20261019.md). Cette PR ne produit aucune prétention de récupération automatique de parité bruitée, de gain linguistique, ni d'amélioration SML25B.

## Contrat et convention

- Entrées vérité `truth[x]` indexées par l'entier `x∈[0,2^n)`, indice du bit `i` = `(x>>i)&1`, `0≤n≤16` et longueur exactement puissance de deux.
- `g(x)=(-1)^f(x)` ; `χ_S(x)=(-1)^popcount(x&S)`; `ghat[S]=2^{-n} Σ_x g(x)χ_S(x)` : coefficient **uniforme**, non la corrélation d'une population arbitrairement biaisée. Transformation rapide WHT avant normalisation et sommation directe indépendante.
- Möbius GF(2) de la vérité vers coefficients ANF : `a[S]=XOR_{T⊆S}truth[T]`; involution. Prédiction depuis ANF : XOR des coefficients `a[T]` pour `T⊆x`.
- Tolérance absolue oracle direct vs rapide : `1e-12` en `f64`; Parseval `|Σ_S ghat[S]^2−1|≤1e-12` sous distribution uniforme ; le test de 16 variables alloue ≤65536 coefficients `f64`. Validation stricte avant toute mutation des buffers.
- Tableau vide, longueur non puissance de deux, largeur `n>16`, valeurs f64 non finies => erreur explicite, sans allocation d'un tableau `2^n` hors borne ni modification partielle du tableau fourni.
- Les exemples obligatoires sont constantes n0/n1, parité XOR d'ordre 4, AND à 3 variables, majorité à 3 variables, table déterministe non triviale, n16 d'exactitude intégrale, et témoins négatifs invalides.

## Critères de décision

PASS F1 si tous tests algébriques passent avec outils Rust du dépôt, `cargo fmt --all -- --check`, Clippy strict, CI complète verte, et la fonction reste strictement opt-in : aucune mise à jour de MOR/ANF, RTLR, dataset, droits ou pipeline de publication. Les tests synthétiques peuvent comparer spectre et ANF mais ne doivent pas annoncer une meilleure qualité de langage.

Après F1 : F2 doit **réutiliser** les travaux A6-C12c/d/e/f/g/h (W2+Sketch/MITM, Gauss, W3) et expérimenter un sélecteur TRAIN-only sous le contrôle négatif parité bruitée ; il ne réécrit pas les moteurs. Le TEST Flaubert A6-E5 et SEC 2024 restent non ouverts, SEC G0 NOT_ADMITTED.
