# Résultats ENG-1c BETA-DEV — grille complète sur les douze DEV

Auteur : Memorithm / Tarek Zekriti. Campagne du 13 octobre 2026.
Statut : **exploration DEV achevée ; supériorité générale non établie**.

Le gain moyen du candidat sélectionné est **+0,166394270002 bit/octet**
sur DEV : français +0,175760751508, anglais +0,157027788495. Les douze
œuvres choisissent β=8, borne haute de la grille enregistrée. Dix choisissent
la profondeur 7 ; Descartes et Darwin choisissent 12. Ces gains sont des
pertes de prédiction sur les données de réglage, avec 24 possibilités
contre quatre pour le comparateur. Ils ne constituent pas une confirmation
sur des œuvres indépendantes. Aucun TEST réel n'a été scoré dans cette campagne.

## Contexte, autorité des résultats et chronologie

La porte adaptative FR129/EN64 avait été sélectionnée après plusieurs
campagnes sur les VAL de Dumas/Maquet et Austen. Le premier panel TESTTEST
indépendant a donné un résultat négatif authentique, déjà archivé dans
`RESULTS-ENG-1C-TESTTEST-FIRST-PANEL.md`. Ce diagnostic d'estimateur ne
remplace pas ce verdict. Le dossier `ENG-1C-TESTTEST-AGENT-HANDOFF.md`
conserve le contexte complet du banc initial, les campagnes de seuils,
les sources, les règles statistiques et leurs limites.

Le protocole présent, `ENG-1C-BETA-DEV-GRID-PROTOCOL.md`, a été publié au
commit `db2c3ddb952c0441001b3095dfd2f7c23a454fce` (PR 547), avant toute
nouvelle mesure β sur ces DEV. La première implémentation a été publiée
au commit `e03e97ae67595bcad06208db8a6af75a33ac4f4f`.

Pendant le travail, PR 548 a intégré un `EscapePolicy` Rust et un diagnostic
Python ENG-1d. Le commit mesuré
`493467e0bed69cc3dcf480aa330d1bc7ac3715c1` intègre cette version de main et
ajoute une comparaison bit à bit entre notre lecture β complète par lot
et la lecture `ScaledWittenBell` de ce moteur. Aucune grille supplémentaire
ENG-1d n'a été exécutée ici. Le script cité `/tmp/claude-0/wbx/` n'était pas
accessible. Les valeurs Opticks rapportées par Claude ne sont ni réexécutées
ni présentées comme des mesures de cette campagne.

## Estimateur, comparateur et tous les réglages

À chaque ordre observé, y compris l'ordre zéro :

`pβ(y|c) = n_y/(N + βU) + βU/(N + βU) × pβ(y|suffix(c))`

N compte les événements TRAIN du contexte et U les octets suivants distincts.
Le calcul part de 1/256. Un contexte absent laisse la distribution du
suffixe inchangée. Un même β s'applique à tous les ordres. La profondeur
compte des **octets**, et les symboles sont les 256 octets, sans tokeniseur.
β supérieur à 1 augmente la masse confiée au suffixe. Les comptes TRAIN
et les contextes stockés sont inchangés.

| Réglage | Valeur effective |
| --- | --- |
| β, ordre de sélection | 1 ; 0,25 ; 0,5 ; 2 ; 4 ; 8 |
| Profondeurs, ordre de sélection | 4 ; 7 ; 12 ; 32 |
| Candidats principaux | 24 couples β × profondeur |
| Comparateur classique | Minimum DEV parmi les quatre profondeurs à β=1, par œuvre |
| Candidat | Minimum DEV parmi les 24 couples, par œuvre |
| Bras secondaire | Ancienne porte FR129/EN64, inclusive, à β=1 |
| Départage | Comparaison stricte des bits f64 non arrondis ; premier bras conservé à égalité |
| Borne β | 0,25 ou 8 signalé ; aucune extension de grille |
| Arithmétique | f64 ; probabilités positives finies ; tolérance de normalisation 1e-10 |
| Lecture par lot | Au plus 16 β ; six effectivement utilisés |
| Entraînement | CAP2b paginé, profondeur maximale 32, limites existantes inchangées |
| Processus | Rust 1.89 release, CPU x86_64, aucun GPU |
| Parallélisme local | Un ouvrage à la fois par langue, deux workers maximum |
| Plafond virtuel par processus | 262144 Kio |
| Délai par ouvrage | 3000 secondes ; SIGTERM puis SIGKILL après cinq secondes |
| Sorties existantes | Refus d'écrasement |

Cette grille reprend les quatre profondeurs du comparateur et inclut des
β inférieurs et supérieurs à 1 pour examiner les deux directions du repli.
Elle n'est pas une recherche exhaustive ni une affirmation d'optimalité.
Le classique est inclus dans le candidat : un gain sélectionné non négatif
est garanti mathématiquement. Son ampleur, son profil et sa stabilité sont
ici des diagnostics ; l'optimisme dû à la sélection reste non mesuré.

## Données, découpage et fermeture de TEST

Les douze auteurs et les deux œuvres par genre/langue du panel initial
sont repris exactement, sans ajout ni exclusion selon les scores. Le roster
est `scripts/eng1c/testtest-roster.tsv`. Le manifeste a pour SHA256
`ffd768bfe236eb1c7e9cee689d84c50702d551315dfe3327ed9dced10074e990`.
C'est un panel historique de convenance FR/EN, sans tirage représentatif.

Pour N octets du corps normalisé, TRAIN est le préfixe
`[0, min(floor(3N/5),600000))`. DEV commence à `floor(3N/5)` et conserve
32768 octets sur chaque œuvre. Les volumes TRAIN présents sont ceux du
panel initial : cette campagne ne produit pas de TRAIN 128/256/512 Kio.

Le prétraitement conserve les octets UTF-8 intérieurs, les notes et préfaces,
la casse, les accents, les blancs et les LF ; seuls les CR sont supprimés.
Le lecteur vérifie le titre, l'auteur et la langue du header Gutenberg,
puis s'arrête exactement au préfixe normalisé nécessaire pour finir DEV.
Il vérifie la longueur brute et les deux empreintes TRAIN/DEV enregistrées.
Il ne lit ni ne hache la fin du corps et ne construit aucun fichier TEST.
L'intégrité actuelle de cette fin non lue n'est donc pas vérifiée ici.

Localement, les sources déjà téléchargées ont servi à préparer les deux
splits ; aucune nouvelle source n'a été ajoutée. La CI retélécharge le brut
complet puis utilise le même lecteur de préfixe. Le programme de grille
accepte uniquement des fichiers TRAIN et DEV dont les longueurs et hashes
concordent avant entraînement. Il ne propose aucune commande de score TEST.
Les anciens fichiers de scores TEST ne participent pas au diagnostic.

TRAIN et DEV sont répartis en 64 lanes contiguës. La longueur d'une lane
est floor(longueur/64), avec un éventuel reste TRAIN inutilisé. L'historique
est réinitialisé à chaque lane et seul t>=1 est compté. DEV a donc 512
octets par lane et **32704 cibles par œuvre**, soit 392448 cibles pour le
panel. Chaque perte est sommée par lane, puis entre lanes, comme dans les
anciens sceaux DEV. Les choix de la porte utilisent les comptes TRAIN,
jamais le label à prédire. Les β et profondeurs sont choisis après calcul
de la perte DEV agrégée.

Les strates TRAIN sont fixées avant mesure : petit <262144 octets,
moyen de 262144 inclus à 524288 exclus, grand >=524288. Ces volumes
varient avec l'œuvre, la langue et le genre. Leur comparaison ne permet
pas d'isoler l'effet causal du volume d'entraînement.

## Résultats principaux sur DEV

Toutes les valeurs suivantes sont en bits par octet cible. Gain positif
signifie perte du classique moins perte du candidat. Les moyennes donnent
le même poids à chaque auteur ; les dénominateurs sont identiques, donc
elles coïncident avec les moyennes pondérées par cible.

| Langue | Auteurs | Classique sélectionné | Candidat sélectionné DEV | Gain | Gain ancienne porte |
| --- | --- | --- | --- | --- | --- |
| ALL | 12 | 2.052727320555 | 1.886333050553 | 0.166394270002 | -0.001130432238 |
| FR | 6 | 1.999280202685 | 1.823519451178 | 0.175760751508 | 0.001187884081 |
| EN | 6 | 2.106174438424 | 1.949146649929 | 0.157027788495 | -0.003448748558 |

### Tous les auteurs

| Auteur | Langue | TRAIN octets | Strate | Classique β1 / d4 | β / profondeur | Candidat DEV | Gain DEV |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Victor Hugo | FR | 405974 | moyen | 2.138162322851 | 8 / 7 | 1.972426517097 | 0.165735805754 |
| Émile Zola | FR | 600000 | grand | 1.969472308322 | 8 / 7 | 1.785857314732 | 0.183614993589 |
| René Descartes | FR | 418981 | moyen | 1.724787524649 | 8 / 12 | 1.542648463129 | 0.182139061520 |
| Jean-Jacques Rousseau | FR | 150401 | petit | 2.201487420330 | 8 / 7 | 2.045237445180 | 0.156249975150 |
| Claude Bernard | FR | 381848 | moyen | 1.762981402636 | 8 / 7 | 1.580851794647 | 0.182129607990 |
| Jean-Henri Fabre | FR | 349751 | moyen | 2.198790237325 | 8 / 7 | 2.014095172282 | 0.184695065043 |
| Charles Dickens | EN | 462904 | moyen | 2.164776514202 | 8 / 7 | 2.005635661174 | 0.159140853028 |
| Mary Wollstonecraft Shelley | EN | 252924 | petit | 2.115954368531 | 8 / 7 | 1.971613249682 | 0.144341118849 |
| Ralph Waldo Emerson | EN | 247743 | petit | 2.433987264942 | 8 / 7 | 2.276738622388 | 0.157248642554 |
| John Stuart Mill | EN | 183888 | petit | 2.069876082323 | 8 / 7 | 1.932599289784 | 0.137276792539 |
| Charles Darwin | EN | 560697 | grand | 1.876394636391 | 8 / 12 | 1.679871294579 | 0.196523341812 |
| Michael Faraday | EN | 134036 | petit | 1.976057764157 | 8 / 7 | 1.828421781965 | 0.147635982191 |

Le classique choisit 4 sur les douze œuvres. Le gain individuel sélectionné
s'étend de +0,137276792539 (Mill) à +0,196523341812 (Darwin).
β=8 est choisi partout, donc aucun optimum intérieur de β n'est observé
dans cette grille. Une valeur au-delà de 8 pourrait gagner ou perdre :
cette campagne ne répond pas à cette question.

La porte ancienne a une perte moyenne supérieure de 0,001130432238 au
classique sur ces DEV : son gain moyen est négatif. Ce bras est secondaire.
Ce score DEV ne change pas son verdict négatif TESTTEST déjà archivé.

### Strates de taille TRAIN

| Groupe | Auteurs | Classique DEV | Candidat DEV | Gain DEV | Gain ancienne porte |
| --- | --- | --- | --- | --- | --- |
| ALL_small | 5 | 2.159472580057 | 2.010922077800 | 0.148550502257 | -0.002063317276 |
| ALL_medium | 5 | 1.997899600333 | 1.823131521666 | 0.174768078667 | -0.000593809015 |
| ALL_large | 2 | 1.922933472356 | 1.732864304656 | 0.190069167701 | -0.000139777703 |
| FR_small | 1 | 2.201487420330 | 2.045237445180 | 0.156249975150 | 0.000008993217 |
| FR_medium | 4 | 1.956180371865 | 1.777505486789 | 0.178674885077 | 0.001834990246 |
| FR_large | 1 | 1.969472308322 | 1.785857314732 | 0.183614993589 | -0.000221649715 |
| EN_small | 4 | 2.148968869988 | 2.002343235955 | 0.146625634033 | -0.002581394899 |
| EN_medium | 1 | 2.164776514202 | 2.005635661174 | 0.159140853028 | -0.010309006060 |
| EN_large | 1 | 1.876394636391 | 1.679871294579 | 0.196523341812 | -0.000057905691 |

L'effet sélectionné apparaît aussi parmi les petits TRAIN. Le groupe
français petit ne contient que Rousseau ; les groupes grands ne contiennent
qu'un auteur par langue. Ces effectifs doivent accompagner chaque lecture
des moyennes. Ils ne constituent pas une démonstration d'effet par taille.

## Grille complète, sans écarter les bras défavorables

Chaque cellule donne la moyenne DEV du bras fixé, sans sélection par œuvre.
Les matrices sont descriptives et utilisent les mêmes DEV déjà observés.
Les 300 lignes prédictives individuelles, leurs bits non arrondis et leurs
métadonnées sont archivés dans `evidence/eng1c-beta-dev-first-run/*-grid.txt`.

### Panel entier

| β | d4 | d7 | d12 | d32 |
| --- | --- | --- | --- | --- |
| 1 | 2.052727320555 | 2.275169852812 | 2.674950777467 | 2.801065432567 |
| 0.25 | 2.188145208894 | 2.940975571503 | 3.908247917306 | 4.201710797197 |
| 0.5 | 2.112611483163 | 2.576553210924 | 3.232109141613 | 3.433100499440 |
| 2 | 2.016078590142 | 2.055924629679 | 2.266239858588 | 2.337547326630 |
| 4 | 2.010104536374 | 1.927575534688 | 2.012913647794 | 2.048022603723 |
| 8 | 2.040796648501 | 1.886938478729 | 1.899220011359 | 1.912591994303 |

### Français

| β | d4 | d7 | d12 | d32 |
| --- | --- | --- | --- | --- |
| 1 | 1.999280202685 | 2.181126957803 | 2.589508984088 | 2.730994370816 |
| 0.25 | 2.106275000354 | 2.798295521949 | 3.797845961015 | 4.127726776780 |
| 0.5 | 2.046345364881 | 2.460272175513 | 3.135120026809 | 3.360913483045 |
| 2 | 1.971483090309 | 1.978687894517 | 2.189808432776 | 2.269568606454 |
| 4 | 1.969427013047 | 1.860979139704 | 1.942344315788 | 1.981358852862 |
| 8 | 1.998638954243 | 1.824653324722 | 1.830788387046 | 1.845422425871 |

### Anglais

| β | d4 | d7 | d12 | d32 |
| --- | --- | --- | --- | --- |
| 1 | 2.106174438424 | 2.369212747821 | 2.760392570846 | 2.871136494318 |
| 0.25 | 2.270015417434 | 3.083655621056 | 4.018649873598 | 4.275694817613 |
| 0.5 | 2.178877601445 | 2.692834246336 | 3.329098256418 | 3.505287515835 |
| 2 | 2.060674089975 | 2.133161364841 | 2.342671284399 | 2.405526046807 |
| 4 | 2.050782059701 | 1.994171929673 | 2.083482979800 | 2.114686354584 |
| 8 | 2.082954342758 | 1.949223632736 | 1.967651635673 | 1.979761562734 |

Le bras commun β=8 / profondeur 7 donne 1,886938478729 sur le panel,
soit +0,165788841826 face aux classiques sélectionnés par œuvre.
Le gain supplémentaire de la sélection des profondeurs par œuvre vaut
0,000605428176 en moyenne. Ces deux calculs utilisent DEV ; choisir un
bras commun après les avoir vus ne le transforme pas en validation indépendante.

## Validation et reproductibilité

Les contrôles locaux obligatoires de format, Clippy strict sur tout le
workspace et suite complète de tests passent après intégration de main.
Un manque de disque a d'abord interrompu la génération des exécutables,
puis des droits d'exécution manquants ont interrompu deux lancements.
Les caches et indices temporaires inutiles ont été supprimés, les droits
des seuls exécutables ELF générés ont été restaurés et les contrôles ont
été intégralement relancés jusqu'au succès. Aucun test n'a été désactivé.

Les tests ajoutés couvrent l'oracle indépendant de fréquences TRAIN,
les 256 probabilités, β=1 bit à bit, lot contre scalaire, compatibilité avec
ENG-1d, contextes courts/absents, paramètres invalides, invariance des
comptes, ordre et égalités de sélection, hashes de splits, refus des commandes
TEST, lecteur borné n'accédant pas à la fin et agrégats DEV d'un vrai
entraînement synthétique paginé.

Sur les douze œuvres réelles, les **48 totaux DEV classiques** sont identiques
bit à bit aux sceaux précédents. Les **24 empreintes store/index TRAIN**
concordent également. `legacy-dev-parity.tsv` conserve les valeurs f64
hexadécimales appariées. Ce contrôle porte sur DEV et les comptes, sans
rouvrir les scores TEST. Les 300 lignes de résumé par groupe et bras ont
aussi été recalculées indépendamment en Python avec `math.fsum` depuis
les bits bruts : écart inférieur à 1e-12 avec les valeurs Rust publiées. Le total des durées par worker est 255,731311
secondes FR et 240,309605 EN, avec deux workers simultanés. Ces durées
sont un relevé de cette exécution, sans comparaison de débit entre moteurs.

Identités de la campagne locale :

- Source : `493467e0bed69cc3dcf480aa330d1bc7ac3715c1`.
- Binaire SHA256 : `b68a14de21c897cdf2aa8ea785c26174edf3eb8e9865ded2a77dfd08e20b0e14`.
- Protocole SHA256 : `878d198885365959fb7a1b3fbfe7193a06b0a1135b7a519382ea65e52580851b`.
- Métadonnées : `environment-LOCAL.txt`, grilles, temps, résumé, parité et
  `manifest.sha256` dans le répertoire d'évidence.

La [campagne CI 38404751855](https://github.com/Memorithm/SML-GENIUS/actions/runs/38404751855)
a refait les douze entraînements et les grilles sur le commit mesuré exact.
Les cinq contrôles actifs `rust`, `fmt`, FR, EN et `beta-dev-panel` sont
verts. Les **300 lignes de politique et 12 sélections** concordent octet
par octet avec les mesures locales ; le résumé est également identique
octet par octet. Les comptes/index et toutes les métadonnées scientifiques
concordent ; les empreintes binaires sont conservées séparément. L'archive
CI a été téléchargée et son digest SHA256 vérifié. `hosted-ci/` conserve les
sorties complètes et `ci-replay.json` la preuve de parité et les contrôles.
Aucun job de score réel TESTTEST n'a été déclenché sur cette branche.
PR 549 a été intégrée dans main au commit
`25c13e9aac923ac12ab13d48d42bee21363ee2b4`, après le succès de ces cinq
contrôles sur son head exact.

Reproduction de la campagne dans un nouveau répertoire :

```bash
cargo build --locked --release -p sml-core --bin sml-eng1c-beta-dev
bash scripts/eng1c/run-beta-dev.sh /tmp/nouvelle-beta-dev ALL
```

Cette commande exécute les douze œuvres séquentiellement et produit le
résumé. La CI utilise un job par langue puis agrège leurs sorties ; le
relevé local utilisait également un worker par langue. Aucun texte corpus
ni store appris n'est publié avec les preuves compactes.

## Ce que l'on peut conclure et prochaine confirmation

Le levier β produit ici une amélioration DEV d'ampleur beaucoup plus grande
que les gains historiques de la porte d'occurrences. La direction apparaît
en français, en anglais et dans les petits TRAIN présents. L'exploration
permet de préparer un candidat ; **elle ne démontre pas sa supériorité générale**.
Le nombre de choix diffère entre les méthodes, les auteurs sont peu nombreux,
les volumes sont confondus avec les œuvres et tous les choix β atteignent
la borne haute. Aucun test de significativité ni verdict scientifique PASS
n'est calculé à partir de ces pertes de sélection.

Une confirmation suivante nécessite un enregistrement distinct avant score :
œuvres jamais exposées, exclusions de tous les corpus de développement et
TEST déjà vus, préfixes TRAIN emboîtés de 128/256/512 Kio sur chaque œuvre,
DEV et TEST identiques entre volumes, grille β/profondeur figée, classique
β=1 sélectionné sur DEV, candidat sélectionné sur DEV, ancienne porte en
bras secondaire. L'analyse doit tenir compte des mesures répétées par
auteur et fixer puissance/précision et effet pratique avant observation
TEST. Le seuil de l'ancien protocole n'est pas repris. Ces pertes DEV
sélectionnées ne suffisent pas à estimer sans biais un effet futur TEST ;
aucun nouveau seuil ni nouveau verdict TEST n'est inventé dans ce rapport.
