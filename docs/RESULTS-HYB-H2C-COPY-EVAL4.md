# HYB H-2c — expert de recopie jugé une fois sur EVAL4 : **GO**

Préenregistrement : `HYB-H2C-COPY-EVAL4-PREREGISTRATION.md` (SML #826, erratum de transcription SML #827). Run : [RemoteOps #1285](https://github.com/Memorithm/RemoteOps/pull/1285), job 127828369608, SML `6ccc55b6`, Dell, 6 novembre 2026. `h2c-eval4.tsv` : sha256 `ca843c6e1cd7a07a6a61286eb8bafbfba3093c0cf6409c0f6d249b7c6e1b91ed`.

## Déroulement

1. **Premier lancement** (run 42188087747) : EVAL4 a été préparé, audité (`PASS_NARROW`) et tracé une fois par P1. Ensuite, `copy-eval4` s'est arrêté au contrôle de gel, avant d'ouvrir EVAL4. La constante figée du nul de la graine 23 avait 47 valeurs au lieu de 48 : un `0` avait été perdu en recopiant.
2. **Diagnostic sur CAL seul** ([RemoteOps #1288](https://github.com/Memorithm/RemoteOps/pull/1288)) : `copy` a été relancé trois fois, deux fois avec le build du troisième run et une fois avec celui d'EVAL4. Les trois runs redonnent `h2c.tsv` 4644dca1… à l'octet près. Le réajustement est donc déterministe, et la seule cause était la constante tronquée.
3. **Reprise** (SML #827 fusionnée) : elle réutilise la trace P1 d'EVAL4 du premier lancement, vérifiée par ses SHA-256 : `trace-eval4.bin` 70c8ac46…, `trace-eval4.banks.bin` 0e352def…, 33 559 221 enregistrements. Il n'y a eu ni nouvelle préparation ni nouveau tracé. Les 48 décalages des six mélangeurs réappris sur CAL-FIT sont exactement ceux du troisième run. EVAL4 a été lu une seule fois par la recopie, le nul et le donneur.

## Résultats

EVAL4 compte 37 articles de peS2o-0004, soit 33 559 221 octets tracés. P1 seul y fait 2,5903 BPC. La recopie couvre 85,3 % des positions et devine juste dans 64,8 % des cas.

| Mesure | Graine 11 | Graine 23 | Seuil |
|---|---:|---:|---|
| A1d seule (BPC) | 2,4296 | 2,4386 | |
| A1d + recopie (BPC) | **1,9274** | **1,9059** | |
| Gain de la recopie sur A1d | **0,502** | **0,533** | ≥ 0,01 |
| Borne bootstrap 2,5 % du gain (97,5 %) | 0,488 (0,517) | 0,518 (0,547) | ≥ 0,01 |
| Gain du donneur sur A1d | 0,090 | 0,110 | |
| Recopie au-delà du donneur | **0,412** | **0,423** | ≥ 0,01 |
| Borne bootstrap 2,5 % au-delà du donneur | 0,396 | 0,408 | ≥ 0,01 |
| Nul sur A1d | −3·10⁻⁷ | −5·10⁻⁷ | \|·\| < 0,002 |
| Articles qui gagnent | 100 % | 100 % | |
| Gain médian par article | 0,493 | 0,524 | |

La causalité est vérifiée : sur 1 539 625 positions, aucun écart n'apparaît quand la seconde moitié de l'article est empoisonnée. Sur P1 seul, la recopie gagne 0,628 BPC.

**Le verdict figé est `GO`**, pour les deux graines. Les valeurs sur EVAL4 sont proches de celles de la calibration : 0,52 et 0,55 BPC de gain, 0,36 et 0,37 au-delà du donneur. Sur EVAL4, la part due au vocabulaire commun (le donneur) est plus faible, et la part due aux répétitions internes à l'article est plus forte.

## Limites

- EVAL4 ne contient que 37 articles. Ce sont de longs articles, puisque les 32 Mio sont atteints avec 37 articles seulement. Le bootstrap tire donc ces 37 articles avec remise. La borne basse reste loin du seuil, mais la diversité des documents est faible.
- La recopie a été retouchée trois fois sur la calibration avant ce jugement (voir `RESULTS-HYB-H2C-COPY-FEASIBILITY.md`). EVAL4 est son premier échantillon jamais vu.
- Le coût n'est pas mesuré ici. Il reste à mesurer dans le moteur natif.

## Suite

Comme le prévoit le préenregistrement, la prochaine étape est d'intégrer la recopie au moteur natif, comme H-3 l'a fait pour la tête H-1d. Il faudra un coût ≤ 1,5× le moteur booléen, un aller-retour exact et Dell = Thor. Cela passera par un préenregistrement séparé, sur un échantillon jamais lu.

Research only. DEV non lu. TEST fermé. NOT_PROMOTED.
