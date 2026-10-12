# ENG-1b/N1 — cinq mesures natives CPU sur Thor (source gelée)

**Date d'exécution :** 12 octobre 2026 UTC.
**Statut :** qualification fonctionnelle native et diagnostic de ressources
**PASS sur les cinq séries**. Ce résultat **n'autorise pas** l'activation
par défaut du trie, l'adaptation de profondeur, un remplacement de SML-25B,
ni l'activation de transitions physiques ElasticXxx.

## Protocole et provenance, avant toute exécution native

- **Préenregistrement antérieur :** `docs/ENG-1B-THOR-NATIVE-PREREGISTRATION.md`,
  premier commit `ea27ca2021ad02809a93abc881ded32d52e27cd0`.
- PR du harnais : [#529](https://github.com/Memorithm/SML-GENIUS/pull/529),
  fusionnée à `835d6a643c41fe61de2dfec661fab3dc4fa03f15`.
- Source réellement vérifiée et exécutée :
  `835d6a643c41fe61de2dfec661fab3dc4fa03f15` (intégration sur `main`).
- Blob Rust ENG-1b immuable :
  `e058f6a7078b84e8e6d5eb344dc19d715daad646`.
- Blob du banc Rust immuable :
  `dd9e59374e4ccaf137ff2e209df8820afc92c825`.
- Exécution effective : [GitHub Actions #38314983846](https://github.com/Memorithm/SML-GENIUS/actions/runs/38314983846),
  job natif `115029361907` — **completed/success**.
- Archive des données brutes (15 processus, 5 rounds, décisions et
  pré/post ressources) :
  [artifact #11776472119](https://github.com/Memorithm/SML-GENIUS/actions/runs/38314983846/artifacts/11776472119)
  (12 412 octets compressés annoncés par GitHub, archive
  non re-téléchargée pour hachage indépendant).
- Hôte détecté `tarek`, `aarch64`, **14 CPU logiques**,
  `rustc 1.89.0` ; CPU utilisé, aucun SDK NVIDIA propriétaire.
  La vérification du nom d'hôte est une condition d'admission, pas une
  preuve cryptographique de l'identité matérielle.
- **Charge synthétique inchangée** : 64 lanes × 96 octets, 6 080
  observations TRAIN ; 96 733 contextes et 101 212 couples
  (contexte, octet suivant) dans les deux représentations
  à profondeur 32 ; 256 échauffements et 2 048 prédictions
  de 256 classes mesurées par processus.
- Charge machine observée : `loadavg` 1 minute
  **8,96 avant** et **6,93 après** ; disponible RAM pré/post
  114 027 420 / 114 696 624 KiB. Ces chiffres ne certifient
  **aucune exclusivité du CPU**, absence de DVFS ou isolation
  des autres tâches ; ils doivent accompagner les mesures.

## Résultats bruts appariés : chaque exécution dans un nouveau processus

| Série | VmHWM TRIE (KiB) | VmHWM LONG (KiB) | p95 TRIE (ns) | p95 LONG (ns) | Seuil préenregistré |
|---:|---:|---:|---:|---:|---|
| 1 | 15 104 | 21 376 | 6 046 | 18 009 | PASS |
| 2 | 15 104 | 21 376 | 6 074 | 17 324 | PASS |
| 3 | 15 104 | 21 376 | 6 046 | 17 351 | PASS |
| 4 | 15 104 | 21 376 | 6 074 | 17 352 | PASS |
| 5 | 15 104 | 21 376 | 5 963 | 17 482 | PASS |

L'ordre est alterné TRIE/LONG/TRIE/LONG/TRIE au début de chaque série.
L'ancien moteur depth=7 est exécuté comme contrôle historique, mais
**n'est pas comparable** à depth=32 sur la perte, la mémoire ou le
temps par contexte.

| Indicateur médian sur cinq processus | TRIE depth32 | LONG depth32 |
|---|---:|---:|
| Pic RSS processus (VmHWM) | **15 104 KiB** | 21 376 KiB |
| RSS juste après apprentissage | **14 976 KiB** | 21 248 KiB |
| Construction TRAIN | **21 879 µs** | 70 990 µs |
| Latence de consultation p50 | **4 926 ns** | 14 555 ns |
| Latence de consultation p95 | **6 046 ns** | 17 352 ns |
| Octets logiques de clés/liens/étiquettes | 1 068 533 | 2 010 371 |
| Octets logiques de compteurs | 791 780 | 791 780 |
| Empreinte calculée sur les mêmes prédictions | `5c4ad424180a35ae` | `5c4ad424180a35ae` |

Pour chaque série, l'empreinte prédictive des deux modèles est identique
et concorde avec le résultat gelé du runner x86-64. Les tests Rust
natifs ENG-1b et ENG-1a sont passés avant le chronométrage.

**Écart de pic mémoire de processus :** −6 272 KiB, soit **−29,34 %**.
**Écart de p95 médiane :** −11 306 ns, soit **−65,16 %**.
**Écart de temps de construction médian :** environ **−69,18 %**.
Les médianes des ratios *appariés*, telles que calculées par le banc,
sont `VmHWM_TRIE/LONG=0.706587` et
`p95_TRIE/LONG=0.348453`. La médiane des ratios n'est pas
nécessairement identique au ratio des médianes.

## Verdict préenregistré

Pour chacune des cinq séries :

```
VmHWM_TRIE <= 0.90 * VmHWM_LONG
p95_TRIE   <= 1.10 * p95_LONG
```

**Résultat effectif :**
`ENG1B_THOR_RESOURCE_VERDICT=PASS_NATIVE_SYNTHETIC_DIAGNOSTIC_ONLY`,
`ENG1B_THOR_RESOURCE_PASS_ROUNDS=5`, verdict PASS 5/5,
`ENG1B_THOR_PRODUCTION_PROMOTION=NOT_AUTHORIZED`.
Un résultat STOP aurait été conservé plutôt que de modifier le seuil
ou d'invalider artificiellement les tests de correction.

## Limites et prochaine admission

1. Cinq processus par représentation sur la même machine au cours
   de **la même session** et le même corpus synthétique ne représentent
   pas cinq populations textuelles ou cinq environnements indépendants.
2. `VmHWM` est le pic RSS du **processus entier** : allocateur, runtime,
   tables et autres données ; ce n'est ni le volume exact des objets
   N-grammes ni la RAM du GPU. L'index n'a pas été qualifié comme
   codec compact physiquement reconfigurable à chaud.
3. Le p95 couvre une prédiction de 256 probabilités sur cette
   distribution de contextes, pas une requête de génération
   SML-25B ni un débit de tokens. CPU, cache, fréquence, thermique,
   interférences et compilation spécifique rendent la mesure locale.
4. Aucune évaluation linguistique indépendante (bits/octets, perplexité
   sur français/anglais), aucun CTW/PPM, aucune variation de profondeur
   pilotée en ligne et aucun test de rollback physique ElasticXxx
   n'est inclus.
5. L'étape suivante est de figer puis évaluer de nouveaux corpus
   français/anglais au titre de
   [ENG-1c #530](https://github.com/Memorithm/SML-GENIUS/issues/530),
   avec les référentiels strictement appariés.
6. [ElasticXxx #244](https://github.com/Memorithm/ElasticXxx/issues/244)
   reste ouverte : `Reencode`/ressources/rollback nécessitent un
   contrat source-borné, faisabilité métier, preuve d'équivalence
   après l'action et reprise sur erreur.

**Conclusion :** ENG-1b dispose désormais d'une confirmation
*dans le périmètre natif Thor CPU et fixture synthétique* de la
supériorité de représentation observée sur x86-64. Ce gain ne
constitue **pas** une preuve que la profondeur 32 améliore les
prédictions linguistiques, ni une autorisation d'inférence de
production.
