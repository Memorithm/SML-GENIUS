# HYB H-1 — tête ternaire résiduelle sur S5h : résultats

**Verdict préenregistré : NO_GO.** L'option B s'arrête à H-1 et H-2 n'est pas lancé. Le résultat est publié tel quel, conformément à `HYB-H1-S5H-TERNARY-RESIDUAL-PREREGISTRATION.md`.

## Exécution

- **Code :** SML main `76b8e47a` (#683), binaire `sml-hyb-h1-s5h`.
- **Traces :** run GB-11 P1 de référence `3c7acc9b…` (SML 958c11d8, bras search_q4).
  - DEV : 60 237 946 positions, SHA `d273af88…`.
  - TRAIN tracé : 65 797 543 positions, SHA `9cff4673…`.
  - Le premier run P1 (`73f91995…`) n'est pas conforme à l'amendement P1 et n'a pas été utilisé.
- **Machines :** Dell, puis rejeu sur Thor, via RemoteOps #817 (run 40082080671), le 24 octobre 2026.
- **Contrôles :**
  - Le BPC P1 recalculé est identique à `trace.json` pour TRAIN et pour DEV.
  - Les splits sont disjoints, l'ordre est strict, TEST est fermé.

## Résultats (DEV S5h, BPC)

| Bras | BPC | Détails |
| --- | --- | --- |
| A0, P1 seul | **3,500060621** | |
| A0c, table booléenne de 288 o | 3,500060621 | λ = 0, retenu sur TRAIN-HOLD |
| A1, graine 11 | 3,822746625 | τ = 0,125 ; 29,7 % de poids nuls ; 288 o |
| A1, graine 23 | 3,822746625 | τ = 0,125 ; 29,7 % de poids nuls ; 288 o |
| W, témoin S5h f32 (repère) | 3,655 | |

Critères préenregistrés :

| Critère | Résultat |
| --- | --- |
| Écarts noyau / oracle | 0 sur toutes les positions DEV |
| Identité Dell/Thor | SHA `a86e99d5…` identique pour les deux têtes |
| Go, seuil A1 < 3,480 | 3,823 → **échec** |
| Écart entre graines | 0 |
| Coût A0 | 0,229 µs/position |
| Coût A0c | 1,013 µs/position |
| Coût A1 | 0,986 µs/position, soit 4,3× A0 |

A1 dégrade donc P1 de **0,32 BPC**. Le surcoût de 4,3× dépasse lui aussi la limite de 1,5×.

## Lecture (diagnostic, hors critères)

- **Les deux graines donnent des têtes identiques au bit près.** L'initialisation (±0,01) disparaît après une époque déterministe d'une perte convexe, et l'export à τ = 0,125 efface le reste. L'écart nul entre graines ne mesure donc aucune variance d'apprentissage.
- **La cause probable de la dégradation est un décalage de calibration entre l'entraînement et le DEV.**
  - Les logits TRAIN sont préquentiels sur des fichiers que P1 a vus pendant son passage : leur BPC est de 2,76.
  - Les logits DEV viennent du modèle gelé, sur des articles jamais vus : leur BPC est de 3,50.
  - La tête apprend une correction (sur-confiance et seaux de |ℓ|) qui vaut pour le régime TRAIN, et TRAIN-HOLD appartient au même régime. Sur le DEV, cette correction est fausse.
  - λ = 0 pour A0c va dans le même sens : même sur TRAIN-HOLD, la table n'apporte rien.
- **Ce résultat ne dit rien d'une tête entraînée sur des logits hors échantillon.** Ce serait par exemple des fichiers TRAIN retenus, tracés avec le modèle P1 gelé. Un tel essai demanderait un nouveau préenregistrement et l'accord du propriétaire. Il ne peut pas réutiliser ce DEV comme critère sans le dire.

## Ce que H-1 établit

- Le noyau ternaire bitplane est exact sur 60 M de positions réelles, et identique entre x86 et ARM.
- Sur ce montage (P1 search_q4, tête de 288 o au total (36 o par profondeur), entraînée sur des logits préquentiels), le neuronal résiduel n'apporte pas de BPC au booléen : il en retire.

NOT_PROMOTED.
