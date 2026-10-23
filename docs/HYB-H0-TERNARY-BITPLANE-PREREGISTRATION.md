# HYB — moteur neuronal hybride sur bus de bits commun : préenregistrement H-0

**Préenregistrement gelé le 23 octobre 2026, avant tout entraînement et toute mesure native du chemin neuronal.**
Option retenue par le propriétaire : **B, bus de bits commun**. Références : GB-9 (`docs/GB9-PVP6BANK-BITSLICE-INTEGRATION-PREREGISTRATION.md`, `docs/GB9-DELL-THOR-NATIVE-EVIDENCE-20261023.md`), porte de coût RTLR-V6 (`docs/SML-RTLR-V6-COST-AWARE-GATE.md`). Cette étape ne modifie ni `Pvp6Bank`, ni l'atlas SBG, ni le routeur MOR, ni aucun poids.

## Architecture visée

Les experts ANF bit-slicés existants et de nouveaux **experts neuronaux ternaires** lisent et écrivent le même état SBG, stocké en bitplanes `u64`. MOR choisit page par page quel expert exécuter. Une page neuronale n'est admise que par une porte de coût par page, inspirée de RTLR-V6. Le chemin neuronal est **entier** à l'inférence (poids ternaires, activations sur 1 à 8 bits, accumulation `i32`). Il n'utilise ni flottant, ni CUDA, ni dépendance SciRust, FLAT ou NNIS à l'exécution. Les sorties doivent donc rester identiques bit à bit sur Dell et sur Thor.

## Étapes

0. **H-0 (cette PR)** : noyau `ternary_bitplane` et oracle scalaire. Aucune revendication de vitesse ni de qualité.
1. **H-1** : tête ternaire résiduelle sur l'état SBG, entraînée sur TRAIN. On mesure le BPC sur DEV contre le booléen seul, à budget mémoire égal. C'est la **porte go/no-go** : si le gain de BPC reste dans le bruit inter-graines, on arrête.
2. **H-2** : expert de page neuronal dans l'atlas, admission par coût, contrôle nul.
3. **H-3** : campagne native Dell/Thor via RemoteOps, au commit fusionné.

## Contrat du noyau H-0

- `TernaryLayer` : poids en `{-1, 0, 1}`, rangés par sortie dans deux plans disjoints `w+` et `w-` de `ceil(inputs/64)` mots. Les bits de remplissage sont nuls. Tout autre poids est refusé.
- `BitplaneActivation` : valeurs non signées sur `bits ∈ 1..=8` plans. `from_bits` enveloppe directement un plan d'état SBG déjà empaqueté et met à zéro les bits de remplissage.
- Une sortie vaut `Σ_b 2^b · (popcount(x_b ∧ w+) − popcount(x_b ∧ w−))`, en `i32`. Le nombre d'entrées est limité à `2^20`, pour que `255 · 2^20` reste représentable.
- `forward_bits` réécrit `pré-activation > seuil` sous forme d'un plan booléen empaqueté, ce qui permet de le rendre au bus SBG.

## Exactitude (obligatoire avant toute mesure)

On exige l'égalité complète avec `ternary_forward_oracle`, une somme scalaire indépendante `i8 × u8`, sur les grilles suivantes : entrées ∈ {1, 2, 63, 64, 65, 127, 128, 1024, 1030}, sorties ∈ {1, 3, 17, 64, 65}, bits ∈ {1, 2, 4, 8}, poids et valeurs tirés par un LCG déterministe.

Cas limites testés : valeurs extrêmes sans débordement, plans disjoints, remplissage nul, état SBG empaqueté avec bits de remplissage sales, seuillage, et refus explicite des entrées invalides.

## Critères de promotion du moteur hybride (H-1 à H-3, figés ici)

| Critère | Seuil |
| --- | --- |
| Exactitude | 0 écart entre le noyau et l'oracle. SHA de `deterministic.csv` identique sur Dell et Thor |
| Qualité | BPC DEV hybride < booléen seul à budget mémoire égal, sur au moins 2 graines. TEST non ouvert. Le témoin 15 M (BPC 3,66) est rapporté sans revendication si l'écart reste dans le bruit |
| Coût | µs par appel moteur ≤ 1,5× le moteur booléen bit-slicé de GB-9. Octets résidents rapportés |
| Routage | 0 page neuronale admise sur la tâche nulle et sur les tâches ANF exactes |
| Reproductibilité | Source épinglée. Tests `pvp6_` et `ternary_bitplane` verts sur l'hôte avant la mesure |

L'énergie n'est pas mesurée (GB-7). Le gradient à travers les bits ANF sera aligné sur le fil d'apprentissage différentiable des décisions booléennes. En son absence, H-1 entraîne seulement la tête, sur un état ANF figé. Le corpus DEV du témoin 15 M doit être identifié et épinglé avant H-1.
