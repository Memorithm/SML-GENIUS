# HLE-5 — Audit scirust et premiers résultats locaux

Date : 2026-10-24 (Europe/Paris). Branche : `feat/hybrid-hle5-learned-memory`,
empilée sur `feat/hybrid-hle-p1-s5h` (la pile HLE-1..P1 n'est **pas** sur `main`).

## 1. Audit scirust (master `9943fd88`, tests exécutés localement)

| Crate | Tests | Utilisé par SML ? | Verdict |
|---|---|---|---|
| scirust-modalg `boolean.rs` (Möbius, Walsh-Hadamard) | 144+2+17 OK | non | **repris** (idée) : criblage Walsh de degré 1 des bits de contexte |
| scirust-neural-operator `anf_search.rs` | 52 OK | non | exhaustif, fonctions exactes seulement, refuse les budgets non couvrables → inadapté aux étiquettes bruitées ; on garde l'idée « recherche de structure » |
| scirust-neural-operator `hybrid.rs` (BooleanRouter) | idem | non | **repris** (idée) : routage conditionnel du mélangeur |
| scirust-simd `pvp.rs` | 110 OK / 2 ignorés | partiellement (layout PVP) | CPU seulement ; **pas de PVP WGPU dans scirust** (réfs WGPU = PR FLAT-ATTENTION #334/#335) |
| scirust-rl-algo (GA/MCTS/beam/SA) | 47 OK | non | jouet ; recherche (1+1) réimplémentée en ~80 lignes |
| scirust-evo (GA/CMA-ES/OpenES/NSGA2) | — | non | candidat futur pour la recherche de groupes |
| scirust-neuro-symbolic (egraph/sat/datalog) | 45+1 OK | non | trop petit/hors sujet ici |
| scirust-core / tensor-ir | 1102+… / 104+… OK | non | rien de spécifique au LM bit |

Aucune dépendance scirust ajoutée : les trois idées tiennent en Rust local sans
NVIDIA ni nouveau crate. Diagnostic HLE-1..4 : les circuits recevaient les
mêmes octets locaux que les comptes d'ordre k encodent déjà exactement.

## 2. Trois améliorations (module `crates/sml-core/src/hle5.rs`)

1. **Mémoire à slots bornée à écriture apprise** (4 voies, 7 o/slot, clés
   d'ordre 8 et 4, éviction par priorité) ; porte d'écriture = régression
   logistique équilibrée sur étiquettes *hindsight* (le contexte réapparaît-il
   dans l'horizon ?), ajustée sur TRAIN uniquement.
2. **Contextes binaires longue portée cherchés** : criblage Walsh puis
   recherche (1+1) de groupes de 12 bits parmi 96 (12 octets), notée sur SELECT.
3. **Routage conditionnel** du mélangeur : 160 routes (bit × confiance mémoire
   × confiance d'ordre).

Contrôles : mémoire « always » et « random à taux égal » de même capacité,
groupes contigus et aléatoires, match non borné HLE-4. Découpage par hash du
nom de fichier (TRAIN/SELECT/TEST disjoints), 3 graines (7, 1007, 2007).

## 3. Résultats locaux (TEST, bits/octet, moyenne 3 graines, 1 Mo TRAIN)

| Bras | SML docs (358 fichiers) | scirust docs (105) |
|---|---|---|
| ordre 5 seul (meilleur n-gram) | **1.971** | 3.761 (ordre 4 : **3.719**) |
| B0 mélange ordres 3+4 | 2.062 | 3.916 |
| B1 + cache + match HLE-4 | 2.002 | **3.169** |
| M0 slot always | 2.004 | 3.185 |
| M1 slot random (taux égal) | 2.020 | 3.179 |
| M2 slot **appris** | 2.035 | 3.588 |
| M3 B1 + slot always | 2.000 | **3.150** |
| M4 B1 + slot appris | 2.001 | 3.169 |
| G0/G1/G2 têtes contiguës/aléatoires/**cherchées** | 2.047/2.058/2.057 | 3.239/3.288/3.235 |
| R0 B1 + **routage** | **1.993** | 3.218 |
| F0 complet contrôles / F1 complet appris | 2.034 / 2.045 | 3.329 / 3.288 |

Latence d'évaluation 170–420 ns/octet, entraînement < 1 s par bras (+ ~9 s de
recherche de groupes), mémoire 7 168 o.

**Lecture honnête (résultats surtout négatifs) :**
- La porte apprise **perd** contre « always » et « random » à capacité égale.
  Les étiquettes hindsight ne prédisent pas l'utilité future.
- Les têtes cherchées battent les aléatoires de peu (~0.05 sur scirust) mais
  **dégradent** B1 sur les deux corpus : le mélangeur sur-apprend.
- Le routage gagne 0.008 sur SML docs et perd 0.05 sur scirust : non robuste.
- Le seul gain stable est la mémoire bornée « always » ajoutée au match
  (−0.002 / −0.019), avec un faible effet.
- Le mélangeur d'ordres 3+4 fait **moins bien** que l'ordre 5 seul sur SML docs :
  il faut des bases plus fortes avant de conclure sur les ajouts.

Reproduire : `bash scripts/hle5-bench.sh` (et `HLE5_CORPUS_DIR=… ` pour un autre corpus).
