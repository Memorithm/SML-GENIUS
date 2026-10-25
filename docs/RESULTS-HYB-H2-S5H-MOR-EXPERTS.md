# HYB H-2 — routage MOR entre têtes H-1d : résultats (NO_GO_CAL_GATE)

**Verdict préenregistré : `NO_GO_CAL_GATE`.** Le routeur appris ne bat pas la tête unique H-1d de 0,01 BPC sur CAL-SEL : +0,0012 pour la graine 11, −0,0027 pour la graine 23. EVAL4 (peS2o-0004) n'a été ni préparé ni lu. Il reste disponible.

Protocole : `HYB-H2-S5H-MOR-EXPERTS-PREREGISTRATION.md`. Code : SML #716, fusion `157cd133`. Run : [RemoteOps #849](https://github.com/Memorithm/RemoteOps/pull/849), workflow [40211105367](https://github.com/Memorithm/RemoteOps/actions/runs/40211105367), Dell puis Thor, terminé avec succès le 25 octobre 2026.

## Résultats sur CAL-SEL (39 633 597 octets)

P1 seul (A0) : 2,7126 BPC. Pour tous les bras, λ = 4/8.

| Bras | Graine 11 | Graine 23 |
|---|---:|---:|
| A1d — une tête H-1d (contrôle) | 2,5540 | 2,5584 |
| A2 — 4 experts, routeur appris | 2,5528 | 2,5611 |
| A2r — 4 experts, routeur par hachage | 2,5632 | 2,5575 |
| Gain A1d − A2 (porte : ≥ 0,01) | **0,0012** | **−0,0027** |

## Ce que montrent les diagnostics

- **L'EM a réduit le routeur à deux experts.** Le nombre d'octets par expert passe ainsi d'un tour à l'autre :
  - tour 0 : 9,2 M, 11,5 M, 4,4 M, 16,2 M ;
  - tour 1 : 10,7 M, 11,9 M, 18,7 M, 0 ;
  - tour 2 : 22,5 M, 0, 18,7 M, 0.

  Les prototypes finaux ne diffèrent que de quelques bits. Le contexte et les banques de la profondeur 0 ne séparent pas le texte en régimes qu'une tête spécialisée exploiterait mieux.
- **L'admission a fait son travail.** Les deux experts vides ne sont pas admis. Les deux experts actifs gagnent environ 0,11 à 0,20 BPC sur leurs propres octets, comme la tête unique. Avec le routeur par hachage, les 4 experts sont admis (gains de 0,14 à 0,16).
- **Contrôle nul : 0 expert admis** sur la tâche sans signal.
- **Exactitude.** Les routeurs et les experts des deux graines ont été rejoués sur Thor, avec des empreintes identiques à celles du Dell. EVAL4 n'a pas été lu, donc le contrôle oracle sur EVAL4 n'a pas eu lieu.

## Lecture

Sur ce moteur, un routage par octet à partir de l'état de P1 n'apporte rien de plus qu'une tête unique qui voit déjà toutes les banques. La tête H-1d capte l'essentiel de ce que l'état de P1 permet de corriger. La capacité supplémentaire (A2r, 4 têtes au hasard) n'apporte rien non plus.

Comme prévu au préenregistrement, H-3 se fait avec la tête unique H-1d, qui est GO.

Ce que ce résultat ne dit pas : il ne dit pas qu'un routage sur d'autres signaux serait inutile, par exemple des pages SBG, un document ou un domaine. Ces variantes demanderaient leur propre préenregistrement.

Research only. DEV non lu. TEST fermé. NOT_PROMOTED.
