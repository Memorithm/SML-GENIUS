# HYB H-2t — templates avant le choix d'expert : résultats (TEMPLATES_NOT_FEASIBLE)

**Verdict figé : `TEMPLATES_NOT_FEASIBLE`.** Les experts routés par template battent bien la tête unique A1d de plus de 0,01 BPC sur CAL-SEL, pour les deux graines. En revanche, les deux graines ne retrouvent pas les mêmes templates : l'accord à 4 templates vaut 0,746, sous le seuil de 0,8. EVAL4 (peS2o-0004) n'a été ni préparé ni lu. Il reste fermé.

Protocole : `HYB-H2T-TEMPLATE-FEASIBILITY.md`. Code : SML #724, fusion `c540b0da`. Run : [RemoteOps #853](https://github.com/Memorithm/RemoteOps/pull/853), job Dell `121284290451`, terminé le 25 octobre 2026. `h2t.tsv` : sha256 `fe183523d562e74a87a54b1a97577dceec765f6c96f9c5d602da292fd832aabc`.

## Stabilité des templates (CAL-SEL, 39 633 597 octets ; 644 526 échantillons de CAL-FIT)

| | 4 templates | 16 templates |
|---|---:|---:|
| Accord entre les graines 11 et 23 (seuil : 0,8 à 4) | **0,746** | 0,386 |

Un template dure en moyenne 48 octets (graine 11) ou 60 octets (graine 23) dans un document.

## Utilité (CAL-SEL, P1 seul : 2,7126 BPC)

| | Graine 11 | Graine 23 |
|---|---:|---:|
| A1d — une tête H-1d | 2,5540 | 2,5584 |
| T — 4 experts routés par template | 2,5308 | 2,5308 |
| Gain T sur A1d (porte : ≥ 0,01) | **0,0233** | **0,0276** |
| Borne oracle (meilleur expert connu après coup, non atteignable) | 2,0964 | 2,0327 |
| (τ, λ) choisis | (2, 8/8) | (1, 8/8) |
| Experts admis | 4 sur 4 | 4 sur 4 |

Pour comparaison, 4 experts routés par hachage (A2r, H-2) donnaient 2,5632 et 2,5575, donc rien de mieux que la tête unique. Le gain de T vient donc du signal de forme, pas de la capacité.

Répartition par template :

| Graine 11 | octets | BPC de P1 | gain de A1d | gain de l'expert |
|---|---:|---:|---:|---:|
| t0 | 26 951 761 | 2,557 | 0,158 | 0,174 |
| t1 | 5 312 619 | 2,710 | 0,155 | 0,193 |
| t2 | 6 298 591 | 3,012 | 0,131 | 0,145 |
| t3 | 1 070 626 | 4,889 | 0,347 | 0,536 |

| Graine 23 | octets | BPC de P1 | gain de A1d | gain de l'expert |
|---|---:|---:|---:|---:|
| t0 | 33 156 933 | 2,581 | 0,150 | 0,187 |
| t1 | 2 206 567 | 3,842 | 0,130 | 0,069 |
| t2 | 233 838 | 6,180 | 1,109 | 2,080 |
| t3 | 4 036 259 | 2,971 | 0,148 | 0,095 |

Les deux graines isolent un régime « difficile » où P1 code mal (4,9 ou 6,2 BPC) et où un expert dédié gagne beaucoup. Mais elles ne le découpent pas de la même façon : la graine 23 en fait un petit groupe très dur de 234 Ko, la graine 11 un groupe plus large de 1,07 Mo. Le reste du texte se partage différemment.

## Recopie (estimation optimiste, sans critère)

Modèle LZP, contexte de 12 octets, taux de réussite appris sur CAL-FIT, choix octet par octet entre P1 et la recopie sur CAL-SEL :

| | Couverture | Réussite | Gain estimé |
|---|---:|---:|---:|
| Tout le flux CAL (entre documents) | 0,999 | 0,422 | 0,566 BPC |
| Document par document | 0,880 | 0,289 | 0,310 BPC |

Sur CAL-FIT, une correspondance de 12 à 15 octets prédit juste 24 à 39 % du temps, une de 16 à 63 octets 94 %, et une de 64 octets ou plus 99 %. C'est le signal le plus fort mesuré dans cette étude, dix fois au-dessus du gain des templates. L'estimation suppose un choix parfait entre P1 et la recopie, elle surestime donc le gain réel.

## Lecture

- **Le routage par forme apporte un signal que l'état de P1 n'a pas.** Le gain de T sur A1d (+0,023 à +0,028) est réel sur CAL-SEL, alors que le routage par l'état de P1 (H-2) et le hachage ne donnaient rien. Il reste optimiste, car CAL-SEL sert à la fois au choix et à la mesure.
- **Les templates ne sont pas stables.** Deux graines produisent deux découpages différents (0,746 à 4, 0,386 à 16). Un routage qui dépend de la graine ne peut pas être préenregistré tel quel. La règle figée tranche : pas de H-2b sur cette base, EVAL4 reste fermé.
- **La recopie est la piste la plus prometteuse.** Les articles se répètent à l'intérieur d'eux-mêmes (noms, formules, références), et P1 ne le voit pas. Une étude dédiée devrait mesurer ce gain dans le vrai mélange avec la tête H-1d, avant tout préenregistrement.

Research only. DEV non lu. EVAL4 non lu. TEST fermé. NOT_PROMOTED.
