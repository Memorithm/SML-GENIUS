# Moteur hybride booléen + ternaire (option B) : bilan de H-0 à H-3

**État au 25 octobre 2026 : le moteur hybride est démontré de bout en bout (H-3 GO).** Une tête ternaire entière, posée sur le modèle booléen à compteurs P1 et branchée sur un codeur arithmétique binaire, comprime mieux que P1 seul. Elle le fait sur deux échantillons jamais lus : EVAL3, jugé sur traces, et EVAL5, jugé en compression réelle. Le coût est de 1,2× à 1,3×, et les sorties sont identiques sur Dell et Thor.

Le routage par experts (H-2) n'apporte rien de plus. Il n'entre donc pas dans le moteur démontré.

## Ce que dit chaque étape

| Étape | Question | Verdict | Fait principal |
|---|---|---|---|
| H-0 | Le noyau ternaire en bitplanes est-il exact ? | Fusionné | Égal à l'oracle scalaire sur toute la grille. Les critères de H-1 à H-3 sont fixés. |
| H-1 | Une tête ternaire sur l'état P1 bat-elle P1 ? | NO_GO | −0,32 BPC. Cause : la tête était entraînée sur un P1 en ligne et jugée sur un P1 gelé. |
| H-1b | La même tête, calibrée | NO_GO | Gain jusqu'à 0,041, mais instable entre graines et trop coûteux |
| H-1c | Une tête moyennée, jugée sur EVAL2 | NO_GO | Stable (écart 0,0002) et coût de 1,32×, mais gain de 0,016 < 0,02 |
| H-1d | La tête voit chaque banque, jugée sur EVAL3 | GO_DIAGNOSTIC_COST → **GO** | Gain de 0,159 et 0,152. Coût de 1,74×, ramené à 1,17× par le noyau fusionné (H-1d-cost). |
| H-2 | Quatre têtes expertes derrière un routeur booléen | NO_GO_CAL_GATE | +0,001 et −0,003 par rapport à la tête unique. L'EM se réduit à 2 experts. Contrôle nul : 0 admis. EVAL4 non lu. |
| H-3 | Le moteur réel (P1 + tête + codeur), sur EVAL5 | **GO** | Gain de 0,160 et 0,152 en bits de conteneur. 0 écart avec l'oracle, 0 échec d'aller-retour. Coût ≤ 1,26×. Dell = Thor. |

Documents de résultats : `RESULTS-HYB-H1-S5H-TERNARY-RESIDUAL.md`, `RESULTS-HYB-H1B-…`, `RESULTS-HYB-H1C-…`, `RESULTS-HYB-H1D-S5H-BANK-HEAD.md`, `RESULTS-HYB-H1D-COST.md`, `RESULTS-HYB-H2-S5H-MOR-EXPERTS.md`, `RESULTS-HYB-H3-NATIVE-ENGINE.md`.

## Le moteur démontré

- **P1.** Modèle booléen `bytes_q4` à banques de compteurs, gelé (`7f4c233d…`), qui prédit chaque bit de l'octet.
- **Tête H-1d.** 1 536 poids ternaires. Elle lit le logit de P1, le stretch de chaque banque, les bits déjà décodés, le contexte et la confiance, et ajoute une correction entière au logit. Le calcul est entier (`i32`), sans flottant, sans CUDA et sans dépendance à SciRust à l'exécution.
- **Codeur.** Codeur arithmétique binaire `bool_coder`. Son surcoût mesuré est d'environ 0,0001 BPC.

| Échantillon | P1 seul | P1 + tête (graines 11 / 23) | Gain |
|---|---:|---:|---:|
| EVAL3 (peS2o-0003, traces) | — | — | 0,159 / 0,152 |
| EVAL5 (peS2o-0005, conteneurs réels) | 2,592 | 2,432 / 2,440 | 0,160 / 0,152 |

## Écarts avec les critères de H-0

Les critères de promotion fixés en H-0 ont été appliqués, avec trois écarts déclarés dans les préenregistrements suivants :

1. **Qualité.** H-0 visait le DEV. Pour ne jamais régler sur le DEV, la qualité a été jugée sur des échantillons neufs (EVAL2 pour H-1c, EVAL3 et EVAL5 pour la tête H-1d), lus une seule fois chacun. EVAL4 n'a pas été lu. Ce critère est plus strict.
2. **Coût.** H-0 visait le moteur Pvp6Bank de GB-9. Celui-ci n'est pas un modèle de texte et ne peut pas compresser EVAL5. Le coût est donc mesuré contre P1 dans le même codeur.
3. **Routage.** Le contrôle nul est fait (0 expert admis). Les « tâches ANF exactes » n'ont pas été rejouées en H-2. Le routage étant NO_GO, aucune page neuronale n'est admise dans le moteur.

## Ce qui n'est pas démontré

- **Le routage MOR entre experts neuronaux.** H-2 est NO_GO sur un routage par octet à partir de l'état P1. D'autres signaux de routage (pages SBG, document, domaine) restent à préenregistrer. EVAL4 reste disponible pour cela.
- **La généralisation.** Les échantillons ne contiennent que des articles peS2o.
- **L'énergie** (non mesurée, GB-7).
- **L'apprentissage en ligne :** P1 et la tête sont gelés.
- **Une comparaison au témoin de 15 M paramètres** (3,655 BPC sur le DEV S5h), qui n'a pas été refaite sur EVAL5.

Research only. TEST fermé. NOT_PROMOTED.
