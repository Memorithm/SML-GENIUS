# A6-E5 — Dirichlet full-byte hiérarchique sur la structure 256 classes déjà existante

**Extension opt-in du module `sml_core::anf_byte_backoff`, sans nouvelle structure sparse concurrente.** La [PR #518](https://github.com/Memorithm/SML-GENIUS/pull/518) a déjà qualifié une interpolation Witten–Bell pour les contextes de signatures booléennes sur 256 symboles. Cette étape ajoute une variante probabiliste Dirichlet contrôlée **à partir des mêmes compteurs immuables TRAIN**, tout en préservant strictement l'API et les résultats de `SparseByteBackoff::distribution()` Witten–Bell existants.

L'appel `distribution_dirichlet_with_parent(signature, ng_parent, alpha, tau)` exige une **distribution NG4 parent** positive et normalisée, fournie par l'appelant après un entraînement sans contamination de TEST. Pour chaque profondeur d de préfixe observé, il calcule :

`p_local_d(y)=(C_d(y)+α)/(N_d+256α)` ;
`w_d=N_d/(N_d+τ)` ;
`p_d(y)=w_d*p_local_d(y)+(1-w_d)*p_parent(y)`.

Le parent racine est NG4 ; une signature de profondeur d non observée reproduit **exactement son parent** ; toutes les nouvelles valeurs restent des distributions 256 classes positives et normalisées. `α>0` et `τ>0` sont contrôlés, tout comme chaque composante de NG4 et sa somme. Les valeurs de grilles préenregistrées pour A6-E5 sont `α∈{0.5,4}` et `τ∈{16,64}`. Le modèle ne garantit pas une validation temporelle OOF automatiquement : ce contrat est imposé par le banc.

**Tests synthétiques :** la perte d'un contexte profond conduit à la même prédiction qu'un modèle limité à la profondeur parent, les probabilités totalisent 1, l'objet entraîné reste inchangé, Witten–Bell n'est pas remplacé, et les signatures hors domaine / taux non finis / NG4 non normalisé sont rejetés. Le workflow Rust 1.89 épinglé impose `fmt`, `check`, Clippy strict et tests, sans appeler le TEST Flaubert.

**Limites :** A6-E5 doit encore intégrer ce repli au Beam Search et à l'arbitre log-loss via un harnais TRAIN/VAL/TEST complet. Ce code ne prouve **aucune** amélioration linguistique, aucun gain d'énergie ou de débit GPU, ni une capacité SML-25B.
