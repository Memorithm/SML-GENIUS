# HYB H-3 — le moteur hybride natif, de bout en bout, sur EVAL5 : résultats (GO)

**Verdict préenregistré : `GO`.** Le moteur hybride se comporte comme un vrai compresseur sans perte : P1 gelé, plus la tête ternaire H-1d, plus le codeur arithmétique binaire.

- **Exact :** 0 écart avec l'oracle des traces.
- **Réversible :** 0 échec d'aller-retour.
- **Meilleur que P1 seul :** −0,160 et −0,152 BPC sur un échantillon neuf, EVAL5.
- **Coût borné :** 1,23× en compression et 1,25× en décompression.
- **Identique sur Dell et Thor :** même `deterministic.csv`.

Protocole : `HYB-H3-NATIVE-ENGINE-PREREGISTRATION.md`, gelé avant le verdict H-2 et avant toute lecture d'EVAL5. Code : SML #718, fusion `2ea518d2`. Run : [RemoteOps #851](https://github.com/Memorithm/RemoteOps/pull/851), workflow [40216508387](https://github.com/Memorithm/RemoteOps/actions/runs/40216508387), le 25 octobre 2026.

## Têtes jugées

H-2 a rendu `NO_GO_CAL_GATE` (`RESULTS-HYB-H2-S5H-MOR-EXPERTS.md`). La règle figée donne donc la forme `single`, avec les têtes A1d publiées de H-1d. Le workflow a relu ce verdict sur le Dell et vérifié les empreintes :

- tête de la graine 11 : `b77e3f13…` ;
- tête de la graine 23 : `3c6192e8…` ;
- P1 : `7f4c233d…`.

## EVAL5

EVAL5 contient 40 documents et 33 609 917 octets : les articles du seau DEV de peS2o-0005, un fichier qu'aucune étape n'avait ouvert. Le manifeste a pour empreinte `2513645d…`. L'audit de recouvrement, avec EVAL5 étiqueté DEV, rend `PASS_NARROW`. EVAL5 n'a été lu qu'une fois.

## Résultats (Dell, conteneurs réels)

| Bras | Octets de conteneur | BPC de conteneur | BPC de modèle | µs/octet compression | µs/octet décompression |
|---|---:|---:|---:|---:|---:|
| B0 — P1 seul | 10 891 211 | 2,5924 | 2,5923 | 0,884 | 0,862 |
| B1 — P1 + tête, graine 11 | 10 218 996 | **2,4324** | 2,4323 | 1,085 | 1,085 |
| B1 — P1 + tête, graine 23 | 10 252 204 | **2,4403** | 2,4402 | 1,083 | 1,078 |

| Critère | Seuil | Graine 11 | Graine 23 | |
|---|---|---:|---:|---|
| Écarts avec l'oracle | 0 | 0 | 0 | ✓ |
| Échecs d'aller-retour | 0 | 0 | 0 | ✓ |
| Gain BPC(B0) − BPC(B1) | > 0,02 et > 2 × l'écart entre graines (0,016) | 0,160 | 0,152 | ✓ |
| Rapport de coût en compression | ≤ 1,5 | 1,227 | 1,225 | ✓ |
| Rapport de coût en décompression | ≤ 1,5 | 1,259 | 1,250 | ✓ |
| `deterministic.csv` Dell = Thor | identique, 24/24 cellules | `d1a163b4…` des deux côtés | | ✓ |

- **Coût du codeur.** L'écart entre bits de conteneur et bits de modèle est d'environ 0,0001 BPC : le codeur est pratiquement transparent.
- **Gain par rapport à P1.** Le conteneur hybride est 6,2 % plus petit que celui de P1 seul (graine 11).
- **Reproductibilité du gain.** Les mêmes têtes H-1d avaient gagné 0,159 et 0,152 sur EVAL3, jugées sur traces. Elles gagnent 0,160 et 0,152 sur EVAL5, en compression réelle. C'est le deuxième échantillon neuf pour ces têtes. EVAL2 jugeait une autre tête, celle de H-1c, et EVAL4 n'a pas été lu.
- **Temps sur Thor.** Ils sont publiés sans critère : 0,196 µs/octet pour P1 seul, 0,378 pour P1 + tête simple, 0,397 pour P1 + têtes routées, sur le P1 synthétique.

## Lecture

H-3 démontre le moteur hybride de bout en bout. Il est exact, réversible, meilleur que le moteur booléen seul sur un échantillon jamais lu, d'un coût borné, et identique sur deux architectures (x86-64 et ARM64).

Ce que H-3 ne démontre pas :

- une généralisation au-delà des articles peS2o ;
- un gain d'énergie ;
- un apprentissage en ligne ;
- une comparaison avec le moteur Pvp6Bank de GB-9.

Le bilan de l'ensemble se trouve dans `HYB-H0-H3-SUMMARY.md`.

Research only. DEV non lu. TEST fermé. NOT_PROMOTED.
