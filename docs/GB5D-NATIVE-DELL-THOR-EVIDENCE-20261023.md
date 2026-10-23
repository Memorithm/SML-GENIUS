# GB-5D — observation native CPU Dell x86_64 et Thor aarch64 (23 octobre 2026)

**Statut scientifique : observation expérimentale native reproductible sous le protocole GB-5D, limitée aux fonctions et aux lots synthétiques gelés.** Aucun gain de qualité du modèle SML-25B, de mémoire totale, d'énergie, de NEON/SIMD ni de GPU n'en découle.

## Source et preuve

- Source scientifique SML-GENIUS : `9c18996d56f34abee7b53b73e2740f252e0972a2` (GB-5D, PR #669, intégré à `main`).
- Contrôle RemoteOps : PR #798, [workflow réussi](https://github.com/Memorithm/RemoteOps/actions/runs/39962386271), Rust **1.89.0**.
- [Thor ARM64, job 120443373993](https://github.com/Memorithm/RemoteOps/actions/runs/39962386271/job/120443373993) et [Dell x86_64, job 120443374552](https://github.com/Memorithm/RemoteOps/actions/runs/39962386271/job/120443374552). Les runners ont vérifié identité, architecture, SHA exact, rustc, test et Clippy sur binaire opt-in.
- **Par hôte** : 120 contrôles déterministes, 1 080 durées brutes, 120 cellules de synthèse, 9 répétitions mesurées par cellule après warmup ; CSV et empreintes validés par `ops/sml-gb5/validate_csv.py`.
- **Contrainte d'archivage** : quota GitHub Actions d'artefacts saturé lors du premier essai `39961996019`. Le second essai réussi a donc publié les CSV complets et les SHA-256 dans **les logs privés des deux jobs**, avec les marqueurs `GB5D_BEGIN_FILE` / `GB5D_END_FILE`. Ces logs sont soumis à une rétention GitHub **non garantie** : conserver une copie indépendante avant expiration. Aucun fichier brut n'est revendiqué comme téléversé en artefact.
- Profils observés : Dell `x86_64`, Thor `aarch64` (14 CPU visibles dans `lscpu`), aucune revendication d'exclusivité CPU, de même fréquence ni d'isolation des charges.
- Deux jeux de données séparés ; **ne pas agréger** les nanosecondes absolues des deux hôtes.

### Empreintes déclarées dans les journaux validés

**thor**

```
42decf0f317ec68a6c978994713448858dd3156d1041f7b52679906299fc5ccc  benchmark.csv
0fdc738e112508559d3c54d317cbf91eb27461858cbcf4ca73a441410f7a227a  summary.csv
```

**dell**

```
2596f7b142593549116e6cfbda8dfb38c2cb51315cc71985895896567d2f3f39  benchmark.csv
4eb2f666e860af4eb45de6868df853d9b86fc407af968d0765fc73ae7ba75521  summary.csv
```

## Rapports de médianes sur les 12 cellules par hôte

Chaque ligne correspond à une largeur et une famille. **A** : `ANF_AND / LUT_AND` (supérieur à 1 = composition ANF plus lente). **B** : `LUT / ANF` pour restriction et permutation (supérieur à 1 = ANF plus rapide). **C** : `LUT_scalaire / bit-slicing` (supérieur à 1 = bit-slicing plus rapide). Le dénominateur des ratios C est soit le calcul à bitplanes préparés, soit le calcul **incluant leur transposition**.

| Machine | Secteurs | Famille | A ANF/LUT | B restriction LUT/ANF | B permutation LUT/ANF | C prépacké LUT/bits | C avec pack LUT/bits |
|---|---:|---|---:|---:|---:|---:|---:|
| thor | 8 | sparse | 5.20 | 1.25 | 0.67 | 3.20 | 0.45 |
| thor | 8 | alternating | 1.62 | 4.26 | 2.84 | 12.69 | 0.49 |
| thor | 8 | pi | 5.38 | 2.31 | 1.24 | 6.45 | 0.47 |
| thor | 16 | sparse | 9.49 | 1.21 | 0.55 | 1.76 | 0.26 |
| thor | 16 | alternating | 1.63 | 7.96 | 4.57 | 11.95 | 0.28 |
| thor | 16 | pi | 21.12 | 2.66 | 1.14 | 3.41 | 0.27 |
| thor | 32 | sparse | 18.67 | 1.18 | 0.45 | 0.80 | 0.21 |
| thor | 32 | alternating | 1.61 | 15.37 | 8.03 | 11.52 | 0.29 |
| thor | 32 | pi | 115.29 | 2.72 | 1.03 | 1.71 | 0.25 |
| thor | 64 | sparse | 37.24 | 1.16 | 0.39 | 0.36 | 0.13 |
| thor | 64 | alternating | 1.64 | 29.35 | 13.64 | 10.36 | 0.20 |
| thor | 64 | pi | 364.26 | 2.82 | 0.87 | 0.80 | 0.17 |
| dell | 8 | sparse | 7.64 | 0.77 | 1.33 | 2.58 | 0.33 |
| dell | 8 | alternating | 4.78 | 1.12 | 2.96 | 4.93 | 0.35 |
| dell | 8 | pi | 9.17 | 0.94 | 1.93 | 3.54 | 0.34 |
| dell | 16 | sparse | 10.73 | 0.89 | 1.47 | 1.37 | 0.26 |
| dell | 16 | alternating | 4.79 | 1.82 | 6.28 | 4.71 | 0.31 |
| dell | 16 | pi | 26.13 | 1.31 | 2.82 | 2.39 | 0.29 |
| dell | 32 | sparse | 16.91 | 0.87 | 1.69 | 0.67 | 0.21 |
| dell | 32 | alternating | 4.79 | 3.22 | 13.90 | 4.58 | 0.30 |
| dell | 32 | pi | 107.07 | 1.75 | 3.53 | 1.33 | 0.25 |
| dell | 64 | sparse | 29.26 | 0.83 | 1.55 | 0.29 | 0.14 |
| dell | 64 | alternating | 4.78 | 6.61 | 28.20 | 4.45 | 0.27 |
| dell | 64 | pi | 296.30 | 2.08 | 3.77 | 0.68 | 0.20 |

Bilan du nombre de cellules où la méthode ANF/bit-slicing dépasse son témoin :

- Dell : composition ANF **0/12** ; restriction ANF **7/12** ; permutation ANF **12/12** ; bitplanes prépackés **9/12** ; bitplanes incluant transposition **0/12**.
- Thor : composition ANF **0/12** ; restriction ANF **12/12** ; permutation ANF **7/12** ; bitplanes prépackés **9/12** ; bitplanes incluant transposition **0/12**.

## Exemple : largeur 64, famille π

| Opération | Dell (ns/opération ou ns/lane) | Thor (ns/opération ou ns/lane) |
|---|---:|---:|
| Compose LUT AND | 3.99121 | 1.82617 |
| Compose ANF AND | 1182.58496 | 665.19629 |
| Restreindre une variable LUT | 79.85449 | 81.18945 |
| Restreindre une variable ANF | 38.48242 | 28.76270 |
| 64 requêtes par LUT scalaire | 1.08765 | 0.74939 |
| 64 requêtes avec bitplanes prépackés | 1.60577 | 0.93359 |
| 64 requêtes avec coût de transposition | 5.53674 | 4.44080 |

## Constat, mécanisme et incertitudes

1. Pour une composition ET isolée, le mot LUT coûte beaucoup moins cher que la convolution des monômes ANF. Ce n'est pas un paradoxe : la composition est une opération algébrique spécialisée, à `O(popcount(A)×popcount(B))` dans l'implémentation.
2. La restriction ou la permutation de variables peut être plus économique directement sur les coefficients, **selon la famille et la machine**. Conserver les cas négatifs, notamment certaines permutations Thor et certaines restrictions Dell.
3. Le bit-slicing prétransposé gagne dans 9/12 cellules sur chaque machine, mais **aucun** chemin incluant la transposition n'est gagnant. Le goulot est plausiblement le coût de préparation des bitplanes : hypothèse à vérifier par une nouvelle expérience de *réutilisation réelle* à coût de préparation amorti.
4. Les banques de fonctions et adresses sont petites, déterministes et répétées : ces mesures ne caractérisent ni un mélange réaliste d'experts, ni l'inférence sur séquences, ni la qualité, ni les coûts des allocations, ni le cache du modèle, ni l'énergie.
5. Des ratios internes à la même machine sont utiles pour la sélection d'un prochain témoin, mais ne prouvent pas une accélération générale ou le remplacement de l'attention.

## Décision GB-6

Préenregistrer **avant** de mesurer un banc `64 positions × R experts distincts` qui paie **une fois** la transposition pour `R∈{1,2,4,8,16,32}`, compare à `R` LUT scalaires et inclut également un témoin à bitplanes déjà préconstruits. Varier les familles et les adresses gelées ; vérifier la parité bit à bit pour tous les résultats. Qualifier un point de bascule seulement si une mesure native reproduite le confirme, **pas** avec l'extrapolation algébrique des chiffres ci-dessus.

Après ce gate, ouvrir indépendamment : mémoire réelle/énergie avec capteurs valides, SIMD/NEON et matériel spécialisé, puis qualité prédictive et baselines attention sous budget apparié. Aucune promotion sans TEST tenu hors entraînement, baselines et CI de code verte sur SHA final.
