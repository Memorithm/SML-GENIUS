# A6-C5b — Extension de capacité : parités récurrentes à trois et quatre phases

**État : expérience exécutée et mesures obtenues ; PR #485 en brouillon.**
Cette expérience complète A6-C5. Elle ne démarre **pas** A6-C6.
Dépendance : la PR #484 (A6-C5) est désormais fusionnée ;
la PR #485 cible directement `main`.

## Question scientifique

A6-C5 avait identifié une limite vérifiable : les deux bits d'état disponibles
ne pouvaient mémoriser exactement les trois parités de phases d'un flux
binaire indépendant. A6-C5b cherche à déterminer si cette incapacité
disparaît à **trois bits d'état**, si un **quatrième bit** suffit à une
parité à quatre phases, et si le problème se reproduit à cinq phases
lorsque le budget est plafonné à quatre bits.

**Méthode : apprentissage uniquement par la perte sur la sortie finale.**
Les cibles auxiliaires des portes, les phases réelles et les fonctions
de référence sont absentes de l'API d'entraînement.

## Architecture et protocole contraint

Pour une largeur `N` parmi 1, 2, 3 et 4 :

```text
s0(t+1) = f(s_feedback(t), x_raw(t))
si(t+1) = ui(s_(i-1)(t))  pour i=1..N-1
prediction(t) = x0(t) XOR s0(t+1)
```

L'apprenant énumère sur **entraînement uniquement** :
la source `x_raw` parmi les quatre canaux disponibles ;
le bit d'état `s_feedback` ;
les **16 tables booléennes** du portail récurrent `f` ;
et **les 4 tables booléennes unaires** de chacun des registres intermédiaires
`ui`. Les fonctions retenues sont transformées en coefficients ANF GF(2)
à l'aide de la transformée de Möbius existante, puis re-simulées dans
`LocalAnfNetwork` comme **oracle indépendant** des scores calculés en
`u64` bit-slicés. La comparaison des erreurs d'entraînement
doit être exactement égale entre les deux moteurs.

Chaque largeur conserve le **meilleur modèle selon l'entraînement**.
La validation ne modifie pas les coefficients et sert uniquement à
sélectionner la largeur minimisant l'erreur ; en cas d'égalité,
le modèle avec moins de bits d'état est conservé. Une proposition n'est
acceptée que si elle améliore strictement la baseline `x0` sur validation.

**Limitation structurelle déterminante :** le graphe de décalage
`s0→s1→s2→s3` est un **échafaudage imposé** ; seules ses tables
de vérité et la position du retour vers `s0` sont apprises.
Ce n'est ni une recherche libre d'architecture, ni une preuve que les
portes d'une mémoire arbitraire s'organiseraient spontanément.

### Contrôles et budgets

- Baseline sans mémoire : sortie `x0`.
- Quatre capacités fixes, 1/2/3/4 bits, évaluées sur les mêmes données
  avec leur meilleur modèle entraîné.
- Sélection de capacité 1 à 4 bits, déterminée sur validation uniquement.
- **Contrôle fort A6-C5 original** : l'apprenant complet à **deux bits**
  doté de sa grammaire générale de 57 600 couples de portes,
  distinct du modèle annulaire à deux bits.

Le moteur annulaire évalue respectivement **64, 512, 3 072 et
16 384** hypothèses d'entraînement selon la largeur, soit
**20 032** évaluations au total par cas. L'apprenant A6-C5 original
parcourt **57 600** hypothèses conjointes et effectue en outre
ses contrôles de descente coordonnée et de redémarrages.
**Ces budgets ne sont pas égalisés** : la comparaison démontre une
frontière de capacité, et non une accélération intrinsèque générale.

### Données et séparation

Six tâches définies à l'avance :

1. **Direct** : `y(t)=x0(t) XOR x1(t)`, aucune mémoire temporelle nécessaire.
2. **Stride 1** : `y(t)=x0(t) XOR [x1(t) XOR x1(t-1) XOR ...]`.
3. **Stride 2** : XOR des valeurs de `x1` séparées par 2 pas.
4. **Stride 3** : XOR des valeurs de `x1` séparées par 3 pas.
5. **Stride 4** : XOR des valeurs de `x1` séparées par 4 pas.
6. **Stride 5** : témoin négatif — mémoire de cinq phases non
   représentable exactement avec au plus quatre bits d'état.

Deux panneaux : données intactes et corruption de labels indépendante
`Bernoulli(1/16)`.

**5 graines × 6 tâches × 2 panneaux = 60 entraînements.** Chaque cas
contient 64 pas d'entraînement et 64 pas de validation, sur 64 voies
booléennes (`4 096` observations de chaque type par graine).
**Tous les 60 modèles sont gelés avant la génération de données de test**.
Chaque cas utilise ensuite deux nouveaux flux indépendants de
**512 et 4 096 pas**, également sur 64 voies.

Le modèle de base ne reçoit que les entrées courantes et
les bits d'état antérieurs ; il n'existe aucun accès aux cibles futures
ni supervision intermédiaire cachée.

## Résultats mesurés : horizon de 4 096 pas

Taux d'erreur binaire moyen sur cinq graines, en %. **Plus bas = meilleur.**
Les résultats sont des mesures **sur problèmes synthétiques**,
sans rapport direct avec la précision d'un modèle de langue.

| Tâche | Conditions | A6-C5 2 bits complet | Anneau 2 bits | Anneau 3 bits | Anneau 4 bits | Capacité sélectionnée |
|---|---|---:|---:|---:|---:|---:|
| Direct | Sans bruit | **0,000** | **0,000** | **0,000** | **0,000** | **0,000** |
| Direct | Bruit 1/16 | 6,251 | 6,251 | 6,251 | 6,251 | **6,251** |
| Stride 1 | Sans bruit | **0,000** | **0,000** | **0,000** | **0,000** | **0,000** |
| Stride 1 | Bruit 1/16 | 6,265 | 6,265 | 6,265 | 6,265 | **6,265** |
| Stride 2 | Sans bruit | **0,000** | **0,000** | **0,000** | **0,000** | **0,000** |
| Stride 2 | Bruit 1/16 | 6,244 | 6,244 | 6,244 | 6,244 | **6,244** |
| **Stride 3** | **Sans bruit** | **50,087** | 49,983 | **0,000** | **0,000** | **0,000** |
| **Stride 3** | **Bruit 1/16** | **50,023** | 50,047 | **6,231** | **6,231** | **6,231** |
| **Stride 4** | **Sans bruit** | **49,953** | 49,923 | 49,923 | **0,000** | **0,000** |
| **Stride 4** | **Bruit 1/16** | **49,975** | 49,885 | 49,930 | **6,289** | **6,289** |
| Stride 5 (négatif) | Sans bruit | 50,027 | 49,971 | 50,061 | 50,060 | **50,047** |
| Stride 5 (négatif) | Bruit 1/16 | 49,998 | 49,986 | 49,955 | 49,949 | **49,941** |

Les résultats à **512 pas** confirment la même transition de capacité :
les modèles à trois bits sont exacts pour stride-3 sans bruit, les
modèles à quatre bits sont exacts pour stride-4, et le témoin stride-5
ne généralise pas.

### Structure des fonctions effectivement apprises

Sur les cinq graines sans bruit de chacun des cas utiles :

| Fonction | Largeur sélectionnée | Retour de s0 | Fonction récurrente f | Registres de délai |
|---|---:|---|---|---|
| Direct | 1 bit alloué, aucun bit de mémoire effectivement utilisé | Aucun retour effectif | Passage de x1 | Aucun |
| Stride 1 | 1 | s0 | XOR(s0, x1) | Aucun |
| Stride 2 | 2 | s1 | XOR(s1, x1) | Copie s0 |
| Stride 3 | 3 | s2 | XOR(s2, x1) | Copie s0, puis s1 |
| Stride 4 | 4 | s3 | XOR(s3, x1) | Copie s0, s1 et s2 |

Toutes ces fonctions ont été choisies par les scores d'entraînement ;
**leurs coefficients ANF et l'indice de retour n'ont pas été injectés
manuellement** dans le moteur. Les registres possèdent un choix de table
de vérité supervisé uniquement par la sortie finale, mais leurs
emplacements dans une ligne de délais sont imposés par la grammaire.

La sélection obtient la largeur et la période attendues dans
**50 cas représentables sur 50**, avec et sans bruit.
Les modèles retenus sont acceptés sur validation dans ces 50 cas.

### Témoin négatif à cinq phases

La parité stride-5 nécessiterait cinq bits indépendants de phase.
Avec quatre bits, il existe au plus 16 configurations récurrentes,
contre 32 configurations distinguables nécessaires pour conserver
exactement cinq parités de phase indépendantes.

Le taux d'erreur sur 4 096 pas reste proche de **50 %** sous
les deux panneaux. **Les 10/10 modèles négatifs sont pourtant acceptés
sur validation** au regard de l'amélioration sur les données courtes.
Ce résultat avertit contre l'utilisation de l'acceptation sur
validation comme substitut à une preuve de capacité mémoire.

Ce témoin indique une limite **de la famille d'automates examinée**,
non une borne universelle sur les capacités de SML-GENIUS.

## Ressources, temps et reproductibilité

- **Commits effectivement mesurés :**
  `645e804bea8c3045e91e5a554a36e2fd1a05cdf8` (code)
  et `bd32f00ff382cc8d8a2f60592a937ac7e9447954`
  (même code, rapport scientifique ajouté).
- **Bancs spécialisés verts :**
  [première exécution](https://github.com/Memorithm/SML-GENIUS/actions/runs/38159146526/job/114528253673)
  et [rejeu sur le rapport complet](https://github.com/Memorithm/SML-GENIUS/actions/runs/38159483800/job/114529215272).
  Les 24 mesures de taux d'erreur et les sélections de capacité sont
  identiques entre ces deux exécutions.
- Plateforme : **GitHub-hosted Ubuntu x86_64, 4 processeurs logiques,
  Rust 1.89.0**. **Aucune mesure n'a été prise sur le Dell.**
- Contrôles de formatage, compilation, Clippy, tests du module
  et tests du benchmark : **tous réussis** sur le SHA mesuré.
- Vérification des compteurs : **60** modèles gelés avant test,
  **120** lignes de mesure de test, **24** lignes de synthèse.
- Temps moyen de recherche annulaire : environ **11,9–12,2 ms**
  par graine/panneau/tâche lors de la première exécution ;
  environ **7,3–7,6 ms** dans le second replay pour les mêmes
  20 032 hypothèses, sur un autre runner. Le contrôle A6-C5
  original varie de **28 à 58 ms** sur ces deux exécutions,
  selon la tâche, le bruit et le runner. Ces temps soulignent
  la variabilité matérielle de GitHub Actions et ne justifient
  **pas** un facteur d'accélération algorithmique général,
  puisque les deux méthodes ont des grammaires et budgets
  d'évaluation distincts.
- Un automate à trois bits représente 3 bits d'état par séquence
  (3 mots `u64` pour 64 séquences bit-slicées) ; quatre bits
  représentent 4 mots `u64`. Ces nombres **excluent** le coût
  total des modèles, des trajectoires, des tables, des allocations
  et du pipeline d'entraînement.

Reproduction après récupération du commit :

```bash
cargo fmt --all -- --check
cargo test --locked -p sml-core anf_phase_capacity
cargo test --locked -p sml-core --bin sml-phase-capacity-bench
cargo run --locked --release -p sml-core --bin sml-phase-capacity-bench -- 5
```

## Conclusion et décision

**PASS A6-C5b** : le seuil de capacité mémoire prévu est confirmé
et les portes ANF associées sont apprises via supervision terminale,
avec généralisation exacte jusqu'à **64 fois la longueur d'entraînement**
sur les fonctions booléennes représentables.

**FAIL volontaire** pour le témoin stride-5, qui démontre le plafonnement
en capacité et les limites d'un simple critère de validation locale.

Il reste à établir la capacité à acquérir ces mécanismes dans
des graphes moins contraints, sans échafaudage de délai prédéfini,
à gérer du bruit de processus plutôt que seulement du bruit de label,
et à optimiser le coût de recherche sur de larges banques ANF.

Ce résultat ne démontre ni amélioration de précision de prédiction
textuelle, ni mémoire générale de 4 096 tokens, ni performances
GPU/SIMD ou passage à l'échelle SML-25B.

**Procédure :** diffuser les résultats et leurs limites au demandeur,
puis obtenir son feu vert avant toute autre expérience.
La PR #485 reste en brouillon jusqu'à ce point.
