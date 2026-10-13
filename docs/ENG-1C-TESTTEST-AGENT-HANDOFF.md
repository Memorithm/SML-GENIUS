# Dossier complet de transmission — banc TESTTEST ElasticNGram

Auteur : Memorithm / Tarek Zekriti. Établi le 13 octobre 2026.
Ce dossier décrit les choix effectivement codés et mesurés, leurs raisons
et leurs limites. Les raisons sont des décisions expérimentales, pas des
faits établissant que ces choix sont optimaux.

## 1. Question de recherche et contexte antérieur

L’intuition est de choisir une profondeur n-gramme selon la quantité
d’évidence disponible dans les comptes TRAIN du contexte courant : plus
profond lorsque les occurrences sont suffisantes, sinon repli vers un
contexte court. Le modèle probabiliste est Witten–Bell sur des **octets**.
Une profondeur 4 signifie quatre octets de contexte pour prédire l’octet
suivant : ce n’est ni quatre mots ni nécessairement quatre caractères Unicode.

La campagne antérieure utilisait deux œuvres : Dumas/Maquet, Gutenberg
17989, et Austen, 1342. Les seuils 2,4,8 puis 16,32,64,128,256,512,1024,
FR140/EN75 et les grilles FR128..139/EN65..74 ont été examinés sur la même
VAL. La sélection finale y favorisait FR129 et EN64. En revanche, cette
sélection après plusieurs observations de VAL ne fournissait pas une
confirmation indépendante. Le TEST original de ces deux œuvres est resté
non scoré ; TESTTEST ne l’ouvre pas.

Repères de VAL antérieure :

| Corpus | Classique profondeur 4 | Adaptatif retenu | Seuil retenu | Gain bits/octet |
|---|---:|---:|---:|---:|
| Dumas/Maquet FR | 2,063036593417 | 2,061937590389 | 129 | +0,001099003027 |
| Austen EN | 1,876564702039 | 1,870807958960 | 64 | +0,005756743079 |

Sources locales : `docs/RESULTS-ENG-1C-VAL1-GRID-FR128-139-EN65-74.md`
et ses lignes brutes. [PR 544](https://github.com/Memorithm/SML-GENIUS/pull/544).
Ces valeurs sont historiques ; elles ne participent pas aux tests statistiques
du nouveau panel et ne sont pas ajoutées aux douze auteurs.

La question de TESTTEST est : **la politique FR129/EN64, figée avant de
nouveaux scores, améliore-t-elle la perte sur plusieurs auteurs/genres
contre un classique dont la profondeur bénéficie d’une vraie sélection DEV ?**
Le mot « générale » signifie ici élargissement à un panel historique FR/EN,
pas universalité, compréhension du langage ni supériorité sur tous les modèles.

## 2. Enregistrement, amendement et identité du code

1. Protocole initial publié au commit
   `73e73110ccc628435909cdf393a0330ed17993d3`.
2. Audit de l’historique : Verne 800 a déjà un TEST A6 exposé ; Flaubert
   14155 est réservé au protocole A6-E5. Remplacement par Hugo/Zola avant
   tout entraînement/score réel. Le code était en préparation et des tests
   synthétiques avaient tourné ; aucun score réel n’a motivé l’amendement.
3. Roster final, manifeste d’empreintes et amendement publiés au commit
   `0bf47640eb769f6b6ed7a4efeff7a98269880f3f`.
4. Première implémentation publiée avant la campagne réelle :
   `65d76ad7f229a544054f79da161cf43e49b907cd`.

Le SHA256 du manifeste admis est compilé dans le programme :
`ffd768bfe236eb1c7e9cee689d84c50702d551315dfe3327ed9dced10074e990`.
Un autre manifeste, même syntaxiquement correct, est refusé par `seal`,
`test` et `analyze`. Le binaire local mesuré a pour SHA256
`c6b5a94f190c82fbe1ecff9f61e085faf66583b8ab04596fcb92c0f53bde5c6f`.
Les commits de protocole précèdent le commit d’implémentation et la première
mesure réelle ; il ne faut pas prétendre que tout le code a été écrit après
le commit d’amendement.

## 3. Panel et sélection des sources

Douze auteurs distincts, un ouvrage par auteur, six par langue, deux par
genre/langue. Les genres sont des strates déclarées, pas une classification
automatique ni une affirmation de contenu exclusivement homogène.

| Langue | Genre | Auteur | Œuvre | Gutenberg |
|---|---|---|---|---:|
| FR | roman | Victor Hugo | Les misérables Tome I: Fantine | 17489 |
| FR | roman | Émile Zola | Germinal | 5711 |
| FR | essai | René Descartes | Discours de la méthode | 13846 |
| FR | essai | Jean-Jacques Rousseau | Les Rêveries du Promeneur Solitaire | 65434 |
| FR | sciences | Claude Bernard | Introduction à l’étude de la médecine expérimentale | 16234 |
| FR | sciences | Jean-Henri Fabre | Souvenirs entomologiques - Livre I | 16825 |
| EN | roman | Charles Dickens | A Tale of Two Cities | 98 |
| EN | roman | Mary Wollstonecraft Shelley | Frankenstein; or, the modern prometheus | 84 |
| EN | essai | Ralph Waldo Emerson | Essays — First Series | 2944 |
| EN | essai | John Stuart Mill | On Liberty | 34901 |
| EN | sciences | Charles Darwin | On the Origin of Species By Means of Natural Selection | 1228 |
| EN | sciences | Michael Faraday | The Chemical History of a Candle | 14474 |

L’URL unique est `https://www.gutenberg.org/cache/epub/ID/pgID.txt` ; les
catalogues `https://www.gutenberg.org/ebooks/ID` ont été consultés le
13 octobre 2026. Le roster et le manifeste contiennent les identités exactes.
L’audit disponible n’a trouvé aucune étude de ces œuvres dans les sources,
scripts et rapports de ce dépôt. Je ne sais pas si elles ont été évaluées
dans des expériences privées ou dans tous les autres dépôts Memorithm.

Justification : multiplier les auteurs, couvrir deux langues et trois types
de prose, conserver un coût CPU borné. **Choix de convenance**, sans tirage
aléatoire ni calcul de puissance préalable. Il n’y a pas de corpus web
moderne, de dialogue, de code ou de troisième langue. Deux auteurs par
strate et six par langue constituent une petite étude de confirmation.

## 4. Prétraitement exact et empreintes

- Source brute UTF-8 obligatoire, au plus 16 MiB par fichier.
- Suppression de tous les CR ; les LF et autres octets sont conservés.
- Un seul marqueur START et un seul END Gutenberg, dans le bon ordre.
- Exclusion des lignes de ces deux marqueurs et des en-têtes/licence extérieurs.
- Conservation exacte de toutes les lignes intérieures, y compris préfaces,
  tables, notes ou métadonnées intérieures ; aucune tentative de nettoyage
  linguistique, de normalisation des accents, de casse ou de blancs.
- Titre, auteur et langue exacts vérifiés dans l’en-tête extérieur.
- SHA256 et longueurs du brut, du corps et des trois splits.
- Corps dupliqué, auteur dupliqué, œuvre absente, source modifiée ou manifeste
  différent : arrêt de l’étude, aucune exclusion a posteriori.

Les empreintes sont calculées sans prédicteur. Lire TEST pour calculer son
hash ne sert pas à choisir une politique et ne constitue pas un score TEST.
La campagne locale a réutilisé les fichiers bruts de cette étape, avec
recontrôle complet à chaque phase. La CI retélécharge les URL puis applique
les mêmes vérifications. Aucun texte ni index appris n’est publié comme preuve.

Point à auditer : les préfaces/notes font partie du corps et peuvent modifier
le domaine réellement mesuré. Zola conserve notamment des métadonnées
intérieures. Les offsets sont des offsets d’octets et peuvent couper une
séquence UTF-8 ; ce n’est pas une segmentation linguistique.

## 5. Tous les réglages de découpage

Pour N octets du corps : a=floor(3*N/5), b=floor(4*N/5).

| Split | Intervalle effectif | Plafond |
|---|---|---:|
| TRAIN | [0, min(a,600000)) | 600 000 octets |
| DEV | [a, min(b,a+32768)) | 32 768 octets |
| TEST | [b, min(N,b+65536)) | 65 536 octets |

Chaque split doit contenir au moins 4 096 octets. Les plafonds sont des
choix de coût fixés avant les scores. Ils ne sont pas des tailles optimisées
par un calcul de puissance. Les gaps entre les intervalles plafonnés et les
queues omises sont rapportés, jamais réintroduits dans l’entraînement.
Le tableau exact des douze ouvrages figure dans le rapport des résultats.

Les tailles TRAIN réelles sont variables : **134 036 à 600 000 octets**.
L’ancienne campagne Dumas/Austen avait environ 590–598 milliers d’octets TRAIN
et un découpage 80/10/10, alors que TESTTEST utilise les règles ci-dessus.
La comparaison adaptatif/classique est appariée à taille identique au sein
de chaque ouvrage. En revanche, un échec de transfert de la politique ne
permet pas de séparer causalement effet de l’auteur, taille TRAIN, découpage
et domaine. C’est une limite majeure pour l’interprétation « générale ».

Chaque split est découpé en **64 lanes contiguës** de
q=floor(split_bytes/64) octets. Contexte remis à zéro à chaque début de lane.
Prédictions aux positions t=1..q−1, donc 64*(q−1) cibles ; le premier octet
de chaque lane et le résidu final split_bytes modulo 64 ne sont pas scorés.
Les lanes ne suivent ni les chapitres ni les paragraphes. Elles reprennent
le contrat de scoring et d’entraînement CAP2b existant.

TEST utilise le vrai préfixe déjà observé de sa lane pour prédire l’octet
suivant : évaluation autorégressive conditionnée sur le texte, pas génération
libre. Les octets futurs ne sont pas des features. Tous les bras prédisent
les mêmes **745 088 cibles TEST**. DEV fournit 32 704 cibles par auteur.

## 6. Modèle probabiliste et politique adaptative

Entraînement séparé sur TRAIN pour chaque ouvrage. Aucun pooling des auteurs,
aucun fine-tuning DEV/TEST, aucun compte modifié pendant le scoring.
Stockage paginé exact de tous les suffixes de profondeur 0 à 32, clefs
d’octets complètes. Les comptes ne sont pas remplacés par des hashes de clefs.
Les probabilities et sommes de pertes sont en `f64`.

Witten–Bell, lorsque le contexte c_d est observé :

`p_d(y|c_d) = n_d(y)/(N_d+U_d) + U_d/(N_d+U_d) * p_(d-1)(y|suffix(c_d))`

N_d est le total des événements TRAIN de ce contexte et U_d le nombre
d’octets suivants distincts observés. Contexte absent : repli. Point de
départ : uniforme sur les 256 valeurs d’octet. Aucun autre lissage appris.
Le readout contrôle aussi la finitude, positivité et normalisation des
distributions avec la tolérance existante `1e-10` sur leur somme.

Politique adaptative, exactement :

```text
T = 129 si FR, 64 si EN
pour profondeur dans [32,12,7] :
    si total exact d’événements TRAIN du suffixe courant >= T : choisir et arrêter
sinon : profondeur 4
```

Seuil inclusif, compte absolu de contexte TRAIN, pas compte du prochain
octet correct, pas fréquence relative, pas nombre d’occurrences dans TEST,
pas nombre de documents. Le courant octet cible n’est pas un argument du
gate. Le gate choisit une profondeur ; il ne mélange pas les quatre
distributions. Les suffixes effectivement disponibles sont ceux du préfixe
de la lane. Une profondeur non accessible a un compte de contexte nul.

Justification du seuil : minima exploratoires précédents FR129/EN64,
conservés sans réglage sur les nouveaux livres. Aucune prétention que ces
valeurs conviennent à toute taille TRAIN ou à toutes les langues. Le nombre
de livres n’est pas utilisé pour ajuster le seuil.

## 7. Comparateur classique et séparation DEV → TEST

Quatre bras classiques : profondeurs 4,7,12,32. Même modèle Witten–Bell,
même TRAIN et même index profondeur 32 que le candidat. Le classique a
l’avantage d’une sélection individuelle de profondeur sur DEV ; l’adaptatif
conserve son seuil hérité et n’utilise pas DEV pour le régler.

`seal` construit TRAIN et score **seulement les quatre classiques sur DEV**.
Choix par perte totale f64 minimale, dénominateur identique. Comparaison
strictement `<`; égalité : priorité 4 puis 7 puis 12 puis 32. Aucun refit.
Le sceau fixe identifiant, manifeste, exécutable, hashes bruts/splits,
quatre pertes DEV, nombre de cibles DEV, profondeur choisie, seuil adaptatif,
hashes du store et de l’index sparse. Il a une empreinte adjacente `.sha256`.

`test` est une commande distincte, qui vérifie le sceau avant de prédire TEST.
Il enregistre les quatre pertes classiques TEST et celle de l’adaptatif,
plus les compteurs de profondeur de l’adaptatif. Le classique primaire reste
celui choisi sur DEV, même si TEST favorise un autre classique. FIXED_4 est
un contrôle secondaire. Aucun oracle TEST ni sélection TEST ne remplace DEV.
Les noms de fichiers de sortie doivent être nouveaux ; overwrite refusé.

**Résultat de la sélection DEV : 4 pour les douze auteurs.** Le comparator
primaire et FIXED_4 coïncident donc ici par sélection, pas par décret après TEST.
Ce banc ne compare pas PPM, CTW, un tokenizer par mots, un Transformer ou
un modèle génératif entraîné sur plusieurs auteurs.

## 8. Métrique, agrégations et précision

Pour chaque politique : `bits = somme_t -log2(p(octet_t|préfixe_t))` ;
`bpb = bits / nombre_de_cibles`. Plus faible est meilleur.
Gain auteur : `(bits_classique_DEV_choisi - bits_adaptatif) / cibles_TEST`.
Gain positif = amélioration de l’adaptatif. Aucune métrique de génération,
compréhension, énergie, latence ou accuracy n’entre dans le verdict.

Primaire : moyenne arithmétique des douze gains auteur. Chaque auteur pèse
1/12 ; chaque langue 1/2 et chaque strate langue/genre 1/6 dans le panel.
Secondaire : gain pondéré par nombres de cibles TEST. Victoires/pertes
sont strictes ; zéro exact est une égalité. Rendu humain à douze décimales,
calculs et selection sur f64 non arrondis. Les fichiers bruts utilisent la
conversion Rust avec précision permettant le round-trip f64.

## 9. Tous les réglages statistiques

| Réglage | Valeur/code |
|---|---|
| Unité de réplication | Auteur, un ouvrage par auteur |
| Bootstrap | Apparié, auteur rééchantillonné avec remise |
| Réplications bootstrap | 9 999 |
| Générateur | SplitMix64 existant, déterministe |
| Graine | `0x5454_2026_1013_B001` |
| Bootstrap global | 6 tirages FR + 6 EN, moyennes des langues à poids égal |
| Bootstrap langue | 6 tirages dans cette langue |
| Bornes 95 % | Tri f64, positions zéro-indexées 249 et 9749 |
| Stratification bootstrap | Langue, **pas** genre |
| Sign-flip global | Toutes les 2^12 = 4 096 affectations de signes |
| Sign-flip langue | Toutes les 2^6 = 64 affectations |
| Alternative | Unilatérale : gain adaptatif positif |
| Queue | Sommes permutées >= somme observée, affectation observée incluse |
| Epsilon pour ties | Aucun, comparaisons sur sommes f64 |
| p global requis | <=0,05 |
| Correction langues | Holm sur 2 p-values : min<=0,025 et max<=0,05 |
| Effet minimal exigé | >=0,010000 bit/octet global et dans chaque langue |

Hypothèses : unités auteur suffisamment indépendantes et nullité à
différences symétriques pour le sign-flip. Le banc ne démontre pas ces
hypothèses ; les auteurs ne sont pas un échantillon aléatoire. Bootstrap
approximatif avec six unités par langue. Le minimum de p langue non nul
est 1/64 si toutes les permutations sauf l’observée sont moins extrêmes.
Sans epsilon, les égalités mathématiques de sommes peuvent être perturbées
par l’arrondi flottant : le contrat exact f64 est déclaré, et cette convention
doit être auditée. Les fixtures analytiques contrôlent des cas exacts simples.

Le seuil 0,01 reprend l’exigence d’effet pratique du programme ENG-1c.
**Ce n’est pas une conséquence du bootstrap ni un seuil dérivé d’une analyse
de puissance.** Un petit gain réel pourrait échouer ce critère.
Le bootstrap global conserve la langue mais pas chaque genre à chaque
tirage ; cette limite doit être discutée si la cible est une généralisation
équilibrée entre genres. Aucun intervalle séparé pour les deux auteurs de
chaque genre n’est revendiqué.

Références de méthode consultées le 13 octobre 2026 : SciPy developers,
`permutation_test` (paired samples / one-sample sign flips),
https://docs.scipy.org/doc/scipy/reference/generated/scipy.stats.permutation_test.html ;
R Core Team, `p.adjust`, référence Holm (1979),
https://stat.ethz.ch/R-manual/R-devel/library/stats/html/p.adjust.html.
L’implémentation et les tests de ce banc sont Rust, sans dépendance SciPy/R.

## 10. Condition exacte du verdict

`PANEL_CONFIRMATION_PASS` exige simultanément :

1. Les douze œuvres, six par langue, deux par strate, toutes les intégrités.
2. Gain moyen global et de chaque langue >=0,01 bit/octet.
3. Bornes bootstrap basses global/FR/EN strictement positives.
4. p global <=0,05 et les deux tests de langue passent Holm.
5. Au moins quatre auteurs strictement gagnants dans chaque langue.
6. Gain moyen non négatif dans chacune des six strates langue/genre.
7. Moyennes globale/FR/EN non négatives contre le contrôle FIXED_4.

Sinon : `PANEL_SUPERIORITY_NOT_ESTABLISHED`. Une source manquante, quota,
timeout ou perte non finie est une erreur d’exécution, pas un auteur retiré.
Une exécution scientifique négative complète est un succès logiciel.
Aucun arrêt anticipé favorable ni agrandissement après observation.

## 11. Résultat de la première campagne complète

| Périmètre | Gain moyen bits/octet | Intervalle bootstrap 95 % | p en faveur de l’adaptatif | Victoires / pertes |
|---|---:|---|---:|---:|
| Ensemble | −0,001379351145 | [−0,002671952455 ; −0,000299641456] | 0,94140625 | 3 / 9 |
| Français | +0,000412414643 | [−0,000466900698 ; +0,001293988281] | 0,203125 | 3 / 3 |
| Anglais | −0,003171116934 | [−0,005640210849 ; −0,001276587221] | 1 | 0 / 6 |

Verdict : **supériorité non établie pour FR129/EN64 sur ce panel**.
En anglais, les six œuvres régressent ; les trois genres anglais aussi.
En français, la moyenne est petite et l’intervalle inclut zéro. L’intervalle
global est négatif sous les hypothèses déclarées. On n’ajoute pas après
coup un nouveau test unilatéral pour annoncer une supériorité inverse.
Ce résultat n’établit ni que toute adaptation est inutile, ni que le
classique est universellement supérieur.

Voir `docs/RESULTS-ENG-1C-TESTTEST-FIRST-PANEL.md` : douze scores détaillés,
six strates, événements, gaps et résidus, lien vers les 24 records de
scellement/TEST et leurs hashes. Il ne faut pas transformer cette expérience
en nouveau tuning puis réutiliser ces TEST comme confirmation indépendante.

## 12. Ressources, stockage, versions et CI

| Paramètre | Valeur |
|---|---:|
| Langage/compilateur | Rust 1.89.0, release pour mesure |
| Plateforme locale | Linux x86_64 CPU |
| Concurrence | 2 workers langue maximum, 1 ouvrage par worker |
| Limite virtuelle par processus | 262 144 KiB = 256 MiB |
| Délai par phase | 3 000 secondes + TERM puis KILL après 5 secondes |
| Profondeur maximale store | 32 |
| Événements TRAIN autorisés par CAP2b | 650 000 maximum |
| Contextes | 16 000 000 maximum |
| Paires contexte/suivant | 16 000 000 maximum |
| Enregistrements par run de tri | 262 144 |
| Runs de tri | 96 maximum |
| Scratch logique | 1 073 741 824 octets maximum |
| Données logiques | 805 306 368 octets maximum |
| Total logique | 2 147 483 648 octets maximum |
| Entrées index sparse | 100 000 maximum |
| Taille brute source admise | 16 MiB maximum |
| Artefacts CI | Records/hashes/rapport, rétention 30 jours |

Les tailles logiques ne sont pas la RAM physique du modèle. Le classique
4 partage un index profondeur 32 pour cette comparaison de prédiction :
on ne mesure pas l’économie que donnerait son propre index court. Aucune
mesure native Thor, GPU, VRAM, énergie ou latence de service dans cette étude.

Le scoreur et les statistiques génériques sont :
`crates/sml-core/src/bin/sml-eng1c-testtest.rs`,
`crates/sml-core/src/paired_panel.rs`.
Les sources sont régies par `scripts/eng1c/testtest-roster.tsv` et
`scripts/eng1c/testtest-manifest.tsv`.
Runner : `scripts/eng1c/run-testtest.sh`.
CI : `.github/workflows/eng1c-testtest.yml` ; deux panels langue puis
adjudication globale des douze records. `workflow_dispatch` est l’entrée
de contrôle ; RemoteOps reste le plan de contrôle distant de référence.

Tests ciblés : six du binaire + trois des statistiques. Incluent oracle
n-gramme indépendant, trajet `seal` → `test`, DEV contre préférence TEST,
drift/corruption, panels incomplets/dupliqués, frontières du gate et
probabilités sign-flip analytiques. Hook fmt, Clippy `-D warnings` et tous
les tests workspace exécutés avant publication. Les SHA256 sont des
identités/checksums, pas signatures ni attestations contre un hôte hostile.
Les records de résultats doivent être archivés durablement dans Git ;
les artefacts CI temporaires ne remplacent pas cet archivage.

## 13. Reproduire sans modifier le protocole

```bash
git clone https://github.com/Memorithm/SML-GENIUS.git
cd SML-GENIUS
git checkout 65d76ad7f229a544054f79da161cf43e49b907cd
cargo build --locked --release -p sml-core --bin sml-eng1c-testtest
bash scripts/eng1c/run-testtest.sh /tmp/testtest-audit-first ALL
cat /tmp/testtest-audit-first/evidence/panel.tsv
```

Le dossier doit être nouveau. Si Gutenberg a changé une source, STOP, sans
réécrire le manifeste pour « faire passer » le banc. Une réplication peut
avoir un autre hash binaire selon les chemins/flags de compilation ; ses
sceaux et son analyse doivent alors utiliser son propre exécutable identique
sur les trois phases. Comparer séparément les identités et les métriques.
Ces TEST sont désormais exposés : cette commande reproduit l’expérience,
elle ne crée pas une nouvelle confirmation indépendante.

## 14. Prompt de mission pour l’agent sollicité

> Audite le banc TESTTEST de Memorithm/SML-GENIUS au commit
> 65d76ad7f229a544054f79da161cf43e49b907cd et les résultats associés.
> Lis d’abord AGENTS.md, la roadmap hors main, ce dossier, le protocole,
> l’amendement, les sources Rust et les records bruts. Ne modifie pas les
> paramètres pour obtenir un PASS. N’utilise pas les douze TEST exposés
> comme confirmation indépendante d’une politique que tu concevrais ensuite.
>
> Vérifie : absence de fuite cible/TEST dans le gate et la sélection DEV,
> dénominateurs et resets 64 lanes, identités des corpus et exclusion des
> études exposées, source réelle derrière les genres, exactitude Witten–Bell,
> parité du classique/adaptatif, intégrité des scellements, statistiques
> appariées et stabilité numérique des sign-flips. Cherche une erreur de
> banc pouvant expliquer le résultat, sans supposer qu’il y en a une.
>
> Évalue les limites : seulement six auteurs par langue, roster de convenance,
> TRAIN variable 134036..600000 octets alors que les seuils venaient de
> TRAIN ~590000, offsets/caps temporels, front matter, UTF-8 par octet,
> reset artificiel des lanes, bootstrap langue sans conservation des genres,
> seuil d’effet 0,01 sans analyse de puissance, absence de transfert entre
> auteurs et comparaison limitée aux profondeurs Witten–Bell 4/7/12/32.
> Distingue un bug, une limite de population, un réglage discutable et une
> observation authentique négative. Ne les confonds pas.
>
> Livre un audit sourcé avec calculs reproductibles, une liste priorisée
> des défauts vérifiés et un protocole suivant **avant ses nouvelles mesures**.
> Si une correction logicielle est démontrée, écris des tests indépendants,
> une PR et conserve la provenance des résultats invalidés/corrigés. Une
> adaptation au volume TRAIN, une sélection de seuil sur DEV ou une autre
> méthode sera une nouvelle politique, nécessitant un nouveau panel TEST
> non exposé. Propose aussi un dimensionnement/pouvoir statistique et des
> budgets identiques ou plusieurs volumes contrôlés par corpus ; n’annonce
> aucune supériorité générale à partir des observations actuelles.

