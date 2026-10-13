# ENG-1e — confirmation prospective de Witten–Bell avec β

Auteur : Memorithm / Tarek Zekriti. Enregistrement du 13 octobre 2026,
avant tout entraînement ou score sur ce nouveau panel. L'ancien résultat
négatif FR129/EN64 et l'exploration des douze DEV restent inchangés.
Objet : perte de prédiction d'octets sur un panel historique FR/EN, pas
supériorité universelle, vitesse ou promotion d'une architecture SML complète.

## Comparaison et sélection figées

Classique : β=1, profondeur DEV choisie parmi [4,7,12,32], par œuvre/volume.
Candidat : β/profondeur DEV choisis parmi β=[1,0.25,0.5,2,4,8] et les mêmes
profondeurs, 24 bras. Ordre écrit ; `<` strict sur bits f64 non arrondis,
premier bras conservé à égalité. β=8 reste la borne ; aucune extension.
Le candidat contient le classique, mais leurs pertes TEST sont indépendantes
de cette sélection DEV. Aucun choix sur TEST, aucune incorporation de DEV
au TRAIN. Ancienne porte FR129/EN64 à β=1 en bras secondaire, sans retuning.
Les mixeurs/codeurs booléens ENG-1d sont exclus de cette comparaison.

À chaque ordre observé, racine comprise :
`pβ(y|c)=n_y/(N+βU)+βU/(N+βU)*pβ(y|suffix(c))`.
N compte les événements TRAIN, U les suivants distincts. Base 1/256 ;
contexte absent : repli inchangé. Comptes immuables, f64 positives finies,
normalisation qualifiée par les oracles existants et nouveaux tests du banc.
Le chemin `target_ladders` applique exactement les readouts β qualifiés.

## Panel et provenance avant mesure

18 œuvres, 18 auteurs : neuf FR, neuf EN, trois par langue/genre
(roman/essai/science), roster `scripts/eng1e/confirm-roster.tsv`.
Catalogues Gutenberg vérifiés le 13 octobre 2026. Balzac contient Illusions
perdues ; Maupassant Bel-Ami. Science comprend médecine, chimie, sociologie,
géologie, biogéographie et méthode/enseignement scientifiques : strate
déclarée, pas garantie de contenu homogène. Panel de convenance, historique,
conditionné par la taille ; pas un échantillon représentatif de toutes les langues.

L'audit du dépôt principal avant ajout du roster ne trouve aucun de ces
auteurs dans ses sources/scripts/docs. Exclusions : auteurs du premier
TESTTEST, Dumas/Maquet, Austen, Verne, Flaubert, Voltaire et Newton/Opticks,
ainsi que sources exposées ou réservées. Cet audit ne couvre pas toutes les
expériences privées hors dépôt. Aucun remplacement après premier score.
Huxley 2940, corps 574798 octets, était trop court ; remplacement par 7150,
629472 octets, avant enregistrement et sans score. Les autres œuvres passent
le minimum 622592 octets. Aucun échec ne permet de retirer un auteur.

Fingerprint Rust sans modèle : UTF-8 <=16 MiB, CR supprimés, une seule paire
START/END Gutenberg ordonnée (THE ou THIS), marqueurs/en-têtes extérieurs
exclus. Tout le corps intérieur conservé : préfaces, notes, tableaux, casse,
accents, espaces et LF. Titre/auteur/langue du header vérifiés exactement.
Hashes bruts/corps/trois TRAIN/DEV/TEST publiés dans `confirm-manifest.tsv`
avant premier fit. Hashage TEST pour provenance n'est pas score TEST.
Aucun corpus ni store appris publié.

## Volumes contrôlés, mêmes labels par œuvre

| Split | Intervalle normalisé semi-ouvert |
| --- | --- |
| TRAIN 128 Kio | [0,131072) |
| TRAIN 256 Kio | [0,262144) |
| TRAIN 512 Kio | [0,524288) |
| DEV commun aux trois volumes | [524288,557056) |
| TEST commun aux trois volumes | [557056,622592) |

Préfixes TRAIN emboîtés ; pas de recouvrement DEV/TEST. Offsets fixes,
pas les anciens 60/80 %, pour contrôler le volume et conserver les labels.
Gaps des petits TRAIN et fin après TEST inutilisés. 64 lanes contiguës,
historique remis à zéro, t>=1. Aucun résidu ; 32704 cibles DEV, 65472 TEST,
131008/262080/524224 événements TRAIN. Les trois volumes d'une œuvre
réutilisent TEST : ce ne sont pas trois auteurs indépendants.

## Scellement, exécution et ressources

`seal` entraîne uniquement TRAIN, calcule les 25 bras DEV et conserve
leurs bits et choix, hashes source/protocole/binaire/splits/store/index.
`test` exige le sceau exact, même source, binaire et store, puis évalue
les deux choix immuables et la porte. Aucun « meilleur TEST » calculé.
Les trois DEV d'un auteur sont scellés avant tout TEST de cet auteur.
Roster complet, grille, code et verdict publiés avant premier fit ; leur
identité reste identique entre auteurs. Pas de pertes TEST intermédiaires
imprimées/interprétées, pas de réglage après ouverture. Agrégation uniquement
après 54 cas complets. Correction changeant données/prédicteur/verdict après
exposition : campagne exploratoire. Réparation d'exécution invariante
conserve toutes les sorties et n'autorise aucune nouvelle sélection.

Rust 1.89 release, CAP2b depth32/limites existantes, 262144 Kio virtuels,
3000 s par seal/test, TERM puis KILL après cinq secondes. Deux workers au
maximum, un auteur par langue ; ses trois stores conservés puis supprimés
après trois TEST. Aucun GPU/service résident/nouvel hôte payant.
RemoteOps reste le plan de contrôle ; SML garde l'autorité scientifique.

## Endpoint, effet pratique et planification

`gain_iv = classique_TEST_bpb - candidat_TEST_bpb`.
Primaire : moyenne des trois volumes par auteur, puis poids égal des 18
auteurs. Langue : neuf auteurs ; genre/langue : trois. Rapport de chaque
cas, volume/langue/genre, pertes, choix DEV, bornes β, victoires/égalités/
défaites et pire régression ; ancienne porte secondaire. Lanes/octets ne
sont pas des unités d'inférence.

Effet pratique **δ=0.05 bit/octet**, environ 30 % du gain DEV exploratoire
sélectionné 0.166394. Cible explicitement conservatrice pour une amélioration
substantielle, pas estimation sans biais de l'effet TEST. L'ancien seuil
0.01 n'est pas repris. Équivalent théorique de 6553.6 octets de longueur
de code sur 1 Mio de cibles, pas mesure d'un compresseur exécuté.

Planification normale indicative : gains moyens par auteur indépendants,
écart-type 0.10 (bien supérieur à la dispersion DEV), vraie moyenne 0.15.
Avec neuf auteurs par langue, puissance approximative 85 % pour dépasser
δ à α unilatéral 0.025 ; environ 72 % pour les deux langues indépendantes.
Pour moyenne 0.10 : environ 32 % par langue. Ce calcul n'est pas la puissance
du test exact/bootstrap et des gardes ci-dessous, et ne la garantit pas.
Le panel vise un effet important ; un résultat négatif peut être inconclusif
pour de petits gains. Nombre d'auteurs fixé, sans extension selon résultats.

## Incertitude et verdict avant TEST

Module Rust `paired_panel` : bootstrap apparié 9999 tirages, seed
`0x5454_2026_1013_B001`, auteurs resamplés dans les six strates langue/genre
puis poids égal des strates. Les trois volumes restent dans leur moyenne
auteur. Pour une langue : ses trois genres. Bornes 95 % aux indices triés
249/9749. Suppose auteurs approximativement indépendants par strate ;
ne rend pas le roster aléatoire ni le résultat universel.

Test exact unilatéral de signes sur gains moyens auteur **moins δ** :
2^18 assignations poolées, 2^9 par langue, somme >= observée, sans epsilon,
assignation observée incluse. Hypothèses : indépendance et symétrie autour
du nul δ. Pooled p<=0.05 ; Holm langues : petit p<=0.025, grand p<=0.05.

`HISTORICAL_FR_EN_PANEL_CONFIRMED` exige toutes les conditions :

- 54 cas valides, effectifs et identités figés, trois volumes par auteur ;
- moyennes pooled/FR/EN >=δ et bornes bootstrap inférieures >δ ;
- test exact pooled et Holm langues au seuil d'effet δ ;
- au moins six gains auteur strictement positifs dans chaque langue ;
- moyenne non négative dans chacune des six strates langue/genre ;
- gain moyen >=δ dans chacune des six cellules langue/volume.

Sinon `SUPERIORITY_NOT_ESTABLISHED`, avec chaque garde en échec explicitée.
Rapporter aussi borne supérieure pooled <0 ou <δ. Une non-confirmation
n'est pas preuve d'égalité/infériorité universelle. Aucun seuil, poids,
roster ou endpoint ne change après TEST pour obtenir un PASS.

## Qualification et réplication

Tests : DEV choisi contre un meilleur TEST, readouts β indépendants, refus
source/sceau/binaire/store altéré, volume inconnu, panel incomplet/dupliqué,
trois volumes regroupés par auteur, effets positifs/négatifs/sous δ, véritable
entraînement/scellement/TEST synthétique et TEST brouillé sans effet sur
choix DEV. Fingerprint qualifié indépendamment ; résultats défavorables
conservés. Hook fmt/Clippy workspace strict/tests workspace avant chaque
publication. Implémentation publiée avant mesure ; CI dédiée répète 54 cas
et agrégation. CI générale/scientifique vertes au head exact avant fusion.
Une science négative est un succès logiciel fusionnable. Répéter ces mêmes
TEST après première ouverture est réplication, pas confirmation indépendante.
