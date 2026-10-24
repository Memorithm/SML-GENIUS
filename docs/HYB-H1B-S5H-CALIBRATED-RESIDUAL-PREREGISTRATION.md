# HYB H-1b — tête ternaire résiduelle calibrée sur le modèle gelé : préenregistrement

**Gelé le 24 octobre 2026, avant toute exécution H-1b et avant toute nouvelle lecture du DEV S5h.** Le propriétaire a choisi, le 24 octobre 2026, l'option « H-1b complet ».

Ce document fait suite à `RESULTS-HYB-H1-S5H-TERNARY-RESIDUAL.md` (verdict NO_GO : A1 3,823 contre A0 3,500 BPC). H-1b est un **nouvel essai, déclaré comme tel**. Il ne révise pas le verdict H-1, qui reste publié.

## Les deux défauts de H-1 que H-1b corrige

1. **Le régime d'apprentissage.**
   - La tête H-1 apprenait sur les logits préquentiels de l'entraînement en ligne de P1 (TRAIN 2,76 BPC), puis elle était évaluée sur les logits du modèle gelé sur le DEV (3,50 BPC).
   - TRAIN-HOLD appartenait au même régime que l'entraînement : le choix de τ ne pouvait donc pas corriger ce biais.
   - H-1b n'apprend et ne choisit rien hors du régime « modèle P1 gelé, documents jamais vus ».
2. **L'export ternaire sans échelle.**
   - H-1 exportait chaque poids maître survivant (|w| ≥ τ) comme une unité pleine de 1/8 ln-odds, quelle que soit sa magnitude.
   - Un poids de bruit, par exemple 0,13, devenait donc ±1, et la correction se trouvait amplifiée.
   - Sur des données synthétiques (test `scaled_export_keeps_the_master_magnitude`), cet effet suffit à rendre la tête pire que P1 quand τ est bas.

Le DEV S5h a déjà été lu une fois, en H-1. Il reste le critère de H-1b. Aucun choix n'y est fait, et le rapport rappelle cette seconde lecture.

## Données et run P1

- **Le run P1.** Le binaire `sml-gb11-p1-s5h run-h1b` reprend le protocole GB-11 P1 de référence (recherche, configurations, budgets, manifeste `966eb65f…`), avec une seule différence :
  - les fichiers TRAIN dont `doc_index % 16 == 7` (**CAL**) sont exclus de la recherche de masques et de l'entraînement ;
  - après l'entraînement, ils sont tracés avec le modèle gelé (split 2), exactement comme le DEV (split 1, 60 237 946 positions).
- **Bras P1 tracés.**
  - `search_q4`, la base de H-1 ;
  - `bytes_q4`, le meilleur bras booléen de P1 (3,021 BPC lors du run de référence).
  - Chaque bras écrit `<bras>/trace-cal.bin`, `trace-dev.bin`, `trace.json` et `weights.bin`.
- **Mesure de vitesse.** `trace.json` porte `dev_us_per_position` : le temps d'inférence P1 gelé par position, mesuré sur une passe DEV sans trace.
- **Découpe de CAL.**
  - CAL-FIT : `doc_index % 32 == 7`, pour l'apprentissage.
  - CAL-SEL : `doc_index % 32 == 23`, pour les choix.
- **TEST** reste fermé. Les données ne quittent pas le Dell.

## Bras, pour chaque base

Le code est dans `hyb_h1b_calibrated.rs` et le binaire `sml-hyb-h1b-s5h`. Ce qui ne change pas par rapport à H-1 : la factorisation MSB, les mêmes 124 entrées 1 bit (seaux |ℓ| corrigés en #683) et la perte logistique.

- **A0 — P1 seul.**
- **A0c — table booléenne de même taille.**
  - 320 o de cellules (n0, n1).
  - Remplie sur CAL-FIT.
  - Correction λ·ln((n1 + ½)/(n0 + ½)), avec λ ∈ {0 ; 1/8 ; 1/4 ; 1/2 ; 1 ; 2} choisi sur CAL-SEL.
- **A1 — tête ternaire à échelle.**
  - **Apprentissage.** Maîtres f32, une époque sur CAL-FIT, taux 1/128. L'ordre des documents est permuté par la graine (11, 23) : en H-1, les deux graines avaient produit la même tête.
  - **Export à τ.**
    - Poids ternaires ±1 là où |w| ≥ τ.
    - Pas entier par profondeur : `unit[d] = max(1, round(32 · moyenne |w| des poids survivants))`, en unités de stretch, ou 0 si aucun poids ne survit.
    - Biais conservé en unités de stretch : `round(32 · b)`.
  - **Correction.** `floor((popcount_dot · unit[d] + bias[d]) · λ8 / 8)`.
  - **Grilles.** τ ∈ {0,0625 ; 0,125 ; 0,25 ; 0,5 ; 1 ; 2} et λ8 ∈ {0, 1, 2, 4, 8, 16}, soit λ ∈ {0 ; 1/8 ; 1/4 ; 1/2 ; 1 ; 2}.
  - **Choix.** (τ, λ) est choisi sur CAL-SEL, par graine. Comme λ = 0 fait partie de la grille, A1 ne peut pas être pire que P1 sur CAL-SEL.
  - **Taille.** 320 o résidents : deux plans u64 de 2 mots, un pas i32 et un biais i32 par profondeur.
  - **Inférence.** Entièrement entière jusqu'à la table de squash.
- **W — témoin S5h** f32, 3,655 BPC. Repère, pas critère.

## Critères (figés, pour chaque base)

1. **Exactitude.**
   - Sur le Dell, les sorties du noyau bitplane et de `ternary_forward_oracle` doivent être identiques sur toutes les positions DEV, avec 0 écart.
   - Les 4 têtes exportées (2 bases × 2 graines) sont rejouées sur Thor. Le SHA-256 des corrections i32 sur 65 536 enregistrements synthétiques doit être le même que sur le Dell.
2. **Go.**
   - Pour les deux graines, il faut BPC_DEV(A1) < min(BPC_DEV(A0), BPC_DEV(A0c)) − 0,02.
   - Cet écart doit aussi dépasser deux fois l'écart entre les graines.
3. **Coût.**
   - Formule : (`dev_us_per_position` de P1 + temps tête) / `dev_us_per_position` ≤ 1,5. Le temps tête est le temps de scoring A1 moins le temps du coût de décision commun.
   - En H-1, A0 ne mesurait que la relecture d'une trace, sans le calcul du modèle P1.
   - Si ce critère échoue, le verdict devient `GO_DIAGNOSTIC_COST` : rapporté, sans lancer H-2.
4. **Lecture.**
   - `verdict` = le verdict de `bytes_q4`. Seul un GO sur cette base autorise l'étape H-2, qui demandera encore un accord.
   - Un GO sur `search_q4` seul est rapporté, mais ne lance pas H-2.
5. **Rapport.** `h1b.tsv` publie tout, sans sélection a posteriori :
   - tous les bras, les deux bases et les deux graines ;
   - le couple (τ, λ) retenu et le BPC sur CAL-SEL ;
   - les pas `unit`, la fraction de poids nuls et les octets.

## Ce que H-1b ne démontre pas

H-1b ne démontre ni le routage MOR neuronal (H-2), ni un gain moteur natif (H-3), ni un gain d'énergie. Il ne démontre pas non plus une généralisation au-delà du DEV S5h, qui est découpé par article et non par auteur.

Retirer CAL de l'entraînement modifie légèrement P1. Les valeurs A0 de H-1b ne sont donc pas celles de H-1. C'est attendu, et c'est déclaré ici.
