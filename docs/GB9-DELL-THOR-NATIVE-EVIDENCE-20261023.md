# GB-9 — qualification native Dell et Thor du bras bit-slicé Pvp6Bank (23 octobre 2026)

**Statut : exact sur les deux machines. Gain net au niveau de la banque. Gain marginal (≤ 1,22×) sur le moteur complet**, car le surcoût SBG en dehors de la banque domine. Aucune revendication d'énergie (non mesurée, GB-7), de mémoire, de SIMD/GPU ni de qualité de modèle.

## Provenance

- Source SML `f53dee18aeaa5af9fc3d8cf5bdfaebb68ea7c4e8`, tête de la PR #674 **non fusionnée**. La campagne doit être rejouée sur le commit de fusion avant toute promotion.
- [RemoteOps PR #801](https://github.com/Memorithm/RemoteOps/pull/801), [workflow #39993043250](https://github.com/Memorithm/RemoteOps/actions/runs/39993043250) **success** : [Dell job 120549396946](https://github.com/Memorithm/RemoteOps/actions/runs/39993043250/job/120549396946), [Thor job 120549396345](https://github.com/Memorithm/RemoteOps/actions/runs/39993043250/job/120549396345). Rust 1.89.0, Release.
- Sur chaque hôte, avant la mesure : tests de parité `pvp6_` et tests du binaire. Ensuite 1 008 cellules déterministes et 9 072 mesures admises par `validate_csv.py`.
- `deterministic.csv` est **identique bit à bit sur Dell et Thor** (`d45b7f6e9cbbb66547855aa3574829f5143782065e9df5178416a641b71f03fa`). Les 8 bras produisent donc les mêmes sorties sur les deux architectures.
- Les CSV complets sont dans les logs privés des jobs, entre `GB9_BEGIN_FILE` et `GB9_END_FILE`. Leur rétention est limitée.

## Résultat 1 : moteur complet (routage MOR + atlas + projection + banque + réécriture)

Le tableau donne le ratio `engine_selected / engine_bitslice`, c'est-à-dire le gain par rapport au chemin de production actuel, au premier R gagnant et à R=64.

| D | k | Q | Dell : 1er R | Dell R=64 | Thor : 1er R | Thor R=64 |
|---:|---:|---:|---:|---:|---:|---:|
| 64 | 2 | 8 | 1 | 1,06× | 1 | 1,07× |
| 64 | 2 | 64 | 2 | 1,03× | 2 | 1,05× |
| 64 | 32 | 8 | — | 0,91× | — | 0,95× |
| 64 | 32 | 64 | 2 | 1,03× | 4 | 1,03× |
| 1024 | 2 | 8 | 1 | 1,13× | 1 | 1,21× |
| 1024 | 8 | 8 | 1 | 1,07× | 1 | 1,17× |
| 1024 | 32 | 8 | — | 0,89× | 1 | 1,02× |
| 1024 | 32 | 64 | 4 | 1,02× | 4 | 1,02× |

Pourquoi les gains sont si faibles : sur Thor, à D=1024, k=32, Q=64 et R=32, un appel moteur coûte environ **190 µs**. La banque seule prend 3,9 µs et le routage MOR 0,54 µs. Environ 98 % du temps part dans la sélection de l'atlas (`BTreeSet` des destinations, construit deux fois), dans `plan.apply` et dans la recopie coordonnée par coordonnée.

## Résultat 2 : banque seule

| Comparaison (Q=64, k=2) | Dell R=64 | Thor R=64 | Premier R gagnant |
|---|---:|---:|---|
| vs chemin de production (Pascal à chaque appel) | 13,5–17,8× | 11,6–13,8× | 2 à 4 |
| vs ANF scalaire | 12,3–20,6× | 10,6–15,8× | 2 |
| vs LUT résidente (contrôle GB-6) | 5,4–7,0× | 8,7–10,5× | 2 à 8 |

- Contre la LUT résidente, le bit-slicing **ne gagne jamais avec Q=8 lanes dès k ≥ 8**, et ne gagne pas à D=1024, k=32 sur Dell, quel que soit Q. Sur Thor, avec D=1024, k=32 et Q=64, il gagne une seule fois, à R=32.
- Avec k=32 et Q=8, le bit-slicing perd aussi contre Pascal (0,45 à 0,74×), sauf sur Thor à D=1024.
- Ces résultats confirment GB-6 dans le moteur réel : il faut beaucoup de lanes, des monômes peu nombreux et plusieurs portes qui partagent la même transposition.

## Décision

- Le bras bit-slicé est **exact** et **intégrable** sans changer le contrat de `Pvp6Bank` ni de l'atlas.
- Dans l'état actuel du moteur, il n'apporte pas de gain matériel significatif de bout en bout. Le prochain verrou est la plomberie SBG (validation des écritures, `apply`), pas le noyau booléen.
- Aucune règle de routage automatique ne doit être tirée de ces seuils. Énergie : non mesurée.
