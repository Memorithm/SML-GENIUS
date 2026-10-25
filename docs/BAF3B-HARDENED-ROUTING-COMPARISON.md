# BAF-3b — Comparaison de routage durcie, multi-graines / Hardened multi-seed routing comparison

Recherche uniquement. **Aucune revendication de qualité, de vitesse, d'énergie ou de supériorité.** Pas de dépendance NVIDIA.

## Motivation

BAF-3 (`baf_focus_compare`) montrait `anf_degree2` à 0/0/0/0 bits d'erreur contre 8/8/4/4 pour le routeur Hamming. Mais la cible synthétique y était **par construction** une conjonction de degré 2 : **BAF-3 seul ne prouve aucune supériorité.** BAF-3b ajoute des cibles où le degré 2 ne devrait pas suffire, plusieurs graines et des splits aléatoires.

```sh
cargo run --release --locked -p sml-core --example baf_hardened_compare
cargo test --locked -p sml-core --example baf_hardened_compare
```

## Hypothèses préenregistrées / Preregistered hypotheses

- **H1 (contrôle positif)** : sur `conj2`, `anf_degree2` atteint ~0 erreur sur tous les splits.
- **H2** : sur `xor2` / `parity3` (non monotones), aucun bras n'est attendu à 0 ; l'avantage de `anf_degree2` sur Hamming ne doit **pas** être présumé.
- **H3 (négatif dur)** : sur `and3` (degré 3) et `random`, `anf_degree2` ne doit pas surpasser nettement `inactive` / Hamming en TEST/OOD.
- Critère d'échec de la revendication « ANF > Hamming » : si `anf_degree2` ne bat pas Hamming en TEST **et** OOD (moyenne sur graines) hors `conj2`, la revendication n'est pas soutenue.

## Protocole

- Atlas identique pour tous les bras : 2 pages écrivaines (masques 4 et 8, `write_value = mask`), 2 pages inertes, top-k = 2, même priorité. Charges d'écriture appariées.
- Bras : `inactive`, `unconditional`, `anf_degree1`, `anf_degree2`, `hamming_key_router` (contrat de routage HLE-0 uniquement, mêmes écritures masquées).
- Familles cibles (graine → bits choisis) : `conj2`, `xor2`, `parity3`, `and3`, `random`. Écritures en OU : `next = state | f0<<2 | f1<<3`.
- 128 entrées (event 3 bits × state 4 bits). OOD = event bit 2 **et** state bit 2 à 1 (16 entrées). Les 112 restantes sont mélangées par graine : TRAIN 56 / VALIDATION 24 / TEST 32. 8 graines.
- Ajustement : TRAIN seul ; VALIDATION = veto unique post-ajustement. TEST/OOD jamais passés aux ajusteurs. Une erreur de l'ajusteur ANF retombe sur l'atlas inactif (fail-closed).
- Tests : splits disjoints (paires et IDs) couvrant les 128 entrées ; déterminisme par graine ; empoisonnement des labels TEST/OOD sans effet sur les modèles ajustés ; contrôle positif `conj2`.

## HLE complet / Full HLE

`hybrid_logic_engine` est présent sur `main`, mais il exécute des `HardLogicNetwork` appris avec écritures XOR : l'inclure romprait l'appariement des écritures. Il n'est **pas** un bras ici ; seul son contrat de routage est réutilisé. Un contrôle HLE apparié reste un travail séparé.

## Résultats (moyenne [min..max] bits d'erreur, 8 graines)

Voir la description de la PR / sortie de l'exemple sur le commit soumis. Les chiffres sont des sondes synthétiques à l'échelle unitaire, pas une évaluation réelle.
