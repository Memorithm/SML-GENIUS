# GB-6 — qualification CPU Dell et Thor, première campagne native (23 octobre 2026)

**Statut : mesure native positive mais conditionnelle**, sur un microbenchmark synthétique gelé. Aucune preuve d'accélération du système SML-25B, de qualité de génération, d'efficacité énergétique, de SIMD/NEON, ni de remplacement de l'attention.

## Provenance vérifiée

- SML-GENIUS source exacte **`8f367ff163db98a47695a9025e35073f1276af8a`**, GB-6 PR #671, fusionnée avant l'essai.
- RemoteOps [PR #799](https://github.com/Memorithm/RemoteOps/pull/799), contrôle de campagne approuvé après [workflow #39977860338](https://github.com/Memorithm/RemoteOps/actions/runs/39977860338) **success** pour les deux machines, commit de fusion RemoteOps `5feeaa6c986dcffc589fafb163008bb063bf5b60`.
- [Dell x86_64 job 120497082169](https://github.com/Memorithm/RemoteOps/actions/runs/39977860338/job/120497082169) et [Thor aarch64 job 120497081676](https://github.com/Memorithm/RemoteOps/actions/runs/39977860338/job/120497081676), Rust 1.89.0, Release.
- Par machine : 432 cellules de validation déterministe ; 3 888 timings bruts, soit 9 mesures par combinaison 4 largeurs × 3 familles × 2 motifs × 6 tailles de banque × 3 chemins ; **tous les checksums identiques** pour les trois chemins, vérifiés avant archivage.
- Les CSV complets et profils CPU sont délimités par `GB6_BEGIN_FILE`/`GB6_END_FILE` dans les **logs privés des jobs**. Le quota GitHub Actions d'artefacts était saturé. Les journaux ont une rétention limitée, aucune archive durable externe n'est revendiquée.

Empreintes SHA-256 déclarées et vérifiées sur chaque hôte :

**dell**

```text
62ba9883e3d843f2f6d3ae5f4634eef6ea0f95d0cbdd5624b833ba7ff709b28d  deterministic.csv
4ed544665c1695ae30da87446d08a175af9f41f24997ca48513e2e0d0a73f2a3  benchmark.csv
6d4ebc7188d3aa05cee83ac970ebc09e44ef5674728ae3e2428ec4f70155e84b  summary.csv
```

**thor**

```text
62ba9883e3d843f2f6d3ae5f4634eef6ea0f95d0cbdd5624b833ba7ff709b28d  deterministic.csv
3d3cec7241e9ea9a75b100b2c57d1123852b89ec18ffd9389cc300fe38e6d277  benchmark.csv
cfdff69152982a3a9b317b3aea7847dbfc31510c4b8644d124c9055872a105dc  summary.csv
```

## Seuils de gain du chemin complet (transposition incluse)

Comparaison : `scalar_lut` vs `bitslice_one_pack`, mêmes 64 adresses et R circuits distincts, préparation des masques/coefficients exclue des deux temps. Un seuil est le **premier R testé** avec ratio de médianes `latence_LUT/latence_bitslice > 1`, et non une garantie de performance. Un ratio 0,56× signifie que LUT est plus rapide.

| Secteurs | Famille | Adresses | Premier R gagnant Dell | Premier R gagnant Thor | Gain Dell à R=32 | Gain Thor à R=32 |
|---:|---|---|---:|---:|---:|---:|
| 8 | sparse | cyclic | 4 | 4 | 3.07× | 4.66× |
| 8 | sparse | clustered | 4 | 4 | 3.07× | 4.65× |
| 8 | alternating | cyclic | 4 | 4 | 2.91× | 4.35× |
| 8 | alternating | clustered | 4 | 4 | 2.91× | 4.35× |
| 8 | pi | cyclic | 4 | 4 | 2.94× | 4.31× |
| 8 | pi | clustered | 4 | 4 | 2.94× | 4.32× |
| 16 | sparse | cyclic | 8 | 8 | 1.93× | 2.12× |
| 16 | sparse | clustered | 8 | 8 | 1.93× | 2.12× |
| 16 | alternating | cyclic | 8 | 8 | 1.97× | 2.17× |
| 16 | alternating | clustered | 8 | 8 | 1.97× | 2.17× |
| 16 | pi | cyclic | 8 | 8 | 1.99× | 2.19× |
| 16 | pi | clustered | 8 | 8 | 1.99× | 2.19× |
| 32 | sparse | cyclic | 16 | 16 | 1.79× | 1.99× |
| 32 | sparse | clustered | 16 | 16 | 1.79× | 1.99× |
| 32 | alternating | cyclic | 16 | 16 | 1.13× | 1.14× |
| 32 | alternating | clustered | 16 | 16 | 1.13× | 1.14× |
| 32 | pi | cyclic | 16 | 32 | 1.11× | 1.13× |
| 32 | pi | clustered | 16 | 32 | 1.11× | 1.13× |
| 64 | sparse | cyclic | 32 | 32 | 1.01× | 1.04× |
| 64 | sparse | clustered | 32 | 32 | 1.01× | 1.04× |
| 64 | alternating | cyclic | — | — | 0.56× | 0.56× |
| 64 | alternating | clustered | — | — | 0.56× | 0.56× |
| 64 | pi | cyclic | — | — | 0.56× | 0.56× |
| 64 | pi | clustered | — | — | 0.56× | 0.56× |

Bilan sur les 144 cellules indépendantes par hôte (4×3×2×6) : **Dell 56/144** gains, **Thor 54/144** gains du chemin complet. À 64 secteurs famille π ou alternance, **aucun gain** à R≤32 sur Dell comme Thor. À huit secteurs, les trois familles gagnent dès R=4 sur les deux architectures.

Remarques : GB-5D avait **0/12** configurations gagnantes par hôte lorsque la transposition était payée pour un seul prédicat. GB-6 change la **granularité de réutilisation** ; cela explique pourquoi les deux mesures ne se contredisent pas. Les temps absolus Dell et Thor ne doivent pas être amalgamés.

## Limites et décision

- Les mêmes 64 adresses sont répétées sur 4096/R lots, les données restent en caches, les fonctions sont petites et déterministes. Ni séquences longue durée, ni batch variable, ni hot/cold-cache, ni randomisation inter-campagnes, ni charge réelle de routage SBG/MOR ne sont mesurés.
- `BitsliceOnePack` inclut le coût de transpose `[u8;64]` en bitplanes `[u64;6]`, mais ne comprend ni classificateur de direction, ni sélection des experts, ni transfert de données inter-mémoire, ni compilation ANF. Le gain ne se propage pas automatiquement au modèle.
- Le compteur `ns/Boolean_output` est normalisé sur R×64×(4096/R) = **262 144 sorties par répétition** : ne pas confondre avec une latence utilisateur par génération.
- Les deux modes `cyclic` et `clustered` constituent un contrôle de localité limité ; des requêtes non répétitives restent à caractériser.
- Prochaine validation (GB-7) : mémoire physique et énergie si les capteurs existent, avec provenance et sans inventer de joules, plus variation R sous charges réelles. Une réduction de mémoire ou d'énergie n'est **pas** démontrée par GB-6.
- Validation modèle (GB-8/9) : ablation PVP6/SBG/MOR en mode non productif, banques réellement exécutées et mêmes données, puis six familles de tâches avec partition train/dev/test réellement tenue hors entraînement, baseline SML sans GB et baseline attention/Transformer sous budget comparable. Exactitude symbolique et débit de circuits ne constituent pas une preuve de qualité linguistique.

**Ne pas déployer de règle de routage automatique à partir de ces seuils** avant une mesure répétée et une décision explicite de promotion.
