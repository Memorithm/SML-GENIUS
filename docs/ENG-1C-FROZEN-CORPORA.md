# ENG-1c/0 — corpus FR/EN gelés par SHA-256 (AVANT entraînement)

État : **provenance vérifiée, non-évaluation de modèle**.
Protocole original préenregistré au commit
`6579234a94434a063aa62eec632fce2248cf19dc`,
avant toute implémentation du lecteur.
Les premières empreintes proviennent du workflow GitHub-hosted
[ENG-1c stage0, run 38315908889](https://github.com/Memorithm/SML-GENIUS/actions/runs/38315908889),
job `115032356812`, exécuté sur commit
`38d17db16075fbf5af63733b157e566f55c1093e`.
Seul un petit manifeste a été archivé :
[artifact 11776079636](https://github.com/Memorithm/SML-GENIUS/actions/runs/38315908889/artifacts/11776079636),
pas le texte des œuvres.

## Sources publiques et empreintes brutes

| Champ | Français | Anglais |
|---|---|---|
| Livre | Dumas / Maquet, *Le comte de Monte-Cristo, Tome I* | Austen, *Pride and Prejudice* |
| Gutenberg | **17989** | **1342** |
| URL immuable en politique, mais fichier distant susceptible d'évoluer | `https://www.gutenberg.org/cache/epub/17989/pg17989.txt` | `https://www.gutenberg.org/cache/epub/1342/pg1342.txt` |
| Taille source brute | **784 416 octets** | **772 386 octets** |
| SHA256 brute | `9ea7a8b127a9dfa36de0b0cf9c2e26293beea5c37a0fc988fb9308b63160f282` | `3f6bb9d6f78e0293b56acd4714dd68cb7d6d1d293402031ce9d5a216bcaf9d75` |
| Taille corps LF normalisé | **747 855 octets** | **737 950 octets** |
| SHA256 corps | `24ebc9a0037fc57bf93bf3ac6788c5ca64a9a6bbaf92b9fdbdfe6ca0f4114dde` | `e79303d04de9260489f9d5a2ad4aae5776680fcec484f2e732d6ded7411bf402` |

Les octets source sont lus par HTTPS. Prétraitement exact :
supprimer tous les CR, extraire les lignes strictement situées entre
les marqueurs Gutenberg START/END du nouveau format, conserver les
lignes du corps en LF sans autre transformation ni filtrage
linguistique. Les empreintes doivent être comparées à **chaque future
lecture**. Un changement de source distante mène à STOP ;
pas de mise à jour silencieuse ni de récupération d'un autre fichier.

## Splits de référence fixés (sans consultation de pertes ni labels par un modèle)

Découpage à `a=floor(0.8*N)`, `b=floor(0.9*N)`, suivi de
64 lanes contiguës par split, contexte remis à zéro à chaque lane.
Les événements structurels suivent la convention ngram t≥1 :
`64 × (floor(split_bytes/64)−1)`.

| Structure | Français | Anglais |
|---|---:|---:|
| TRAIN octets | 598 284 | 590 360 |
| VAL octets | 74 785 | 73 795 |
| TEST octets | 74 786 | 73 795 |
| Événements TRAIN t≥1 | **598 208** | **590 272** |
| Événements VAL t≥1 | 74 688 | 73 728 |
| Événements TEST t≥1 (non scorés) | 74 688 | 73 728 |
| TRAIN résidu non inclus | 12 | 24 |
| VAL résidu non inclus | 33 | 3 |
| TEST résidu non inclus | 34 | 3 |

Les effectifs TEST sont calculés uniquement à partir des longueurs de
tranches, sans lecture des octets par un prédicteur. Aucun bpb, perplexité,
exactitude ou résultat de génération n'est connu ou revendiqué.

## Blocage de capacité scientifique identifié

`ElasticNGramLong` et `ElasticNGramTrie` déclarent
`max_events <= 500000`, `max_nodes <= 250000`,
`max_follower_pairs <= 500000` et `max_depth <= 32`.

**Chaque TRAIN complet dépasse 500 000 événements** :
598 208 en français, 590 272 en anglais. Par conséquent,
les deux ensembles complets sont **non-admissibles** selon la
configuration actuelle du modèle avant même d'examiner le nombre de
contextes distincts. Il serait incorrect de prétendre pouvoir les
entraîner sans modifier les limites ou la population de travail.

Une PR ultérieure **distincte, avant tout entraînement/évaluation**
devra figer soit un plan de montée en capacité avec comptes/mémoire
vérifiés et limites réévaluées, soit un échantillonnage **déterministe
prédéfini**, respectant les frontières temporelles et les 64 lanes,
sans utiliser la perte VAL/TEST. Toute saturation d'index devra
produire `CAPACITY_STOP`, pas troncature silencieuse.

## Contrôle de passage à ENG-1c/1

1. Exécuter la même procédure avec les deux empreintes brutes et
   normalisées désormais fixées ; **chaque nombre** est vérifié.
2. Fusionner uniquement la provenance et les politiques de contrôle
   après succès du workflow dédié et de la CI générale sur le HEAD final.
3. Dans une nouvelle PR, figer populations, budgets et protocole
   256-classes multi-profondeurs avant exécution. VAL sélectionne la
   politique ; TEST n'est consulté qu'après gel de cette décision.
4. CTW/PPM sont des estimateurs distincts et demandent une
   préinscription indépendante.

Cet enregistrement **n'autorise aucune performance** ni adaptation de
profondeur dans le modèle SML ou le contrôleur ElasticXxx.
