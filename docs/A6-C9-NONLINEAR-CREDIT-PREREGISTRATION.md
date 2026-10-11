# A6-C9 — Préenregistrement : crédit conjoint non linéaire ANF

Statut : protocole fixé le 11 octobre 2026, **avant première exécution**. Base : main après fusion de la PR #488.

## Question falsifiable

A6-C8 retrouve exactement les deux portes affines XOR/XNOR cachées, mais
échoue pour XOR+AND. A6-C9 teste si une recherche conditionnelle par
simulation causale exacte peut découvrir deux portes internes à tables
de vérité complètes (16 fonctions chacune), sans étiquettes auxiliaires.

Les objectifs XOR+AND, AND+XOR, AND+AND, XOR+OR sont des hypothèses
expérimentales, PAS des résultats garantis. Une tâche à trois portes
intermédiaires sert de contrôle délibérément hors capacité.

## Famille de réseaux et ce qui est appris

État booléen récurrent N=128 ou 256 bits par séquence, 64 séquences
parallèles packées dans des mots Rust u64. La racine reste
s0(t+1)=s_(N-1)(t) XOR x1(t), le readout est
y_hat(t)=x0(t) XOR s0(t+1). Chaque registre intermédiaire est
une porte identité s_i(t+1)=s_(i-1)(t), sauf au maximum DEUX positions
distinctes j et k choisies parmi 1..N-1. Pour une porte cachée,
s_j(t+1)=f(s_(j-1)(t),x_r(t)) ; le canal raw x_r appartient
à x1..x4 et f est une des 16 tables booléennes binaires.
Les choix de j,k,r,q,f,g sont inconnus de l'algorithme.

**Fixé par construction** : anneau, root XOR, readout XOR,
fan-in <=2, maximum de deux mutations. Le banc ne prétend pas
apprendre une topologie libre de 256 portes.

## Stratégie algorithmique inscrite au protocole

La recherche train utilise un calcul causal exact de la sortie finale
par trois segments circulaires u64, correspondant aux distances
racine→porte1→porte2→racine. Pour un couple de portes, ces segments
partitionnent N états ; la mise à jour est synchrone, avec états de
t uniquement. Une vérification indépendante reconstruit le même
modèle dans LocalAnfNetwork avec conversion Möbius en coefficients
ANF natifs.

Initialisations TRAIN SEULEMENT :

1. Meilleure porte unique issue du moteur A6-C7 (toutes les 16 tables),
   capable de prendre un crédit partiel sur une AND.
2. Paire XOR/XNOR issue du moteur A6-C8, essentielle pour les cas où
   les deux portes n'ont aucun crédit marginal individuel.
3. Baseline sans mutation.

Pour le meilleur modèle à une porte TRAIN :
énumérer chaque position de seconde porte distincte, ses quatre
canaux et ses 16 tables de vérité. Évaluer chaque couple par sortie
terminale sur TRAIN complet (N128: 8064 candidats ; N256: 16256).

**Amendement technique avant l'exécution du banc complet :**
le tout premier test unitaire XOR+AND a révélé que le meilleur
candidat isolé était une porte constante à zéro, qui effaçait
le signal de la seconde porte. Conserver uniquement le
meilleur minimum de départ détruisait donc une amorce
affine utile. Les deux amorces TRAIN (conditionnelle A6-C7
et affine A6-C8) sont dorénavant **raffinées séparément**.

Pour chaque amorce, tester **les deux ordres de
balayage des axes** (porte0→porte1, puis
porte1→porte0) avec l'ensemble des positions
distinctes, canaux et 16 fonctions. Ne conserver
que des améliorations strictes d'erreur TRAIN.
Retenir ensuite la meilleure paire selon TRAIN,
sans consulter VAL/TEST. Les réseaux gagnants
sont reconstruits et vérifiés bit-exacts contre
LocalAnfNetwork.

Nombre de scores de raffinement : **8×(N−2)×64**
si l'amorce A6-C7 est disponible (2 amorces ×2 ordres
×2 axes), ou **4×(N−2)×64** si seul
le contrôle affine est admissible. La recherche
conditionnelle initiale ajoute (N−2)×64 scores
et la paire affine est scorée une fois.
Cette modification algorithmique est fixée
**avant la campagne de 100 entraînements et
avant la génération d'un quelconque TEST** ;
les dix tâches, graines et critères préenregistrés
ne changent pas.

**La validation ne sert qu'au choix final** entre référence,
modèle A6-C7 à une porte, modèle A6-C8 affine à deux portes et
modèle A6-C9 à deux portes générales, avec priorité au moins
de mutations sur égalité. Aucune donnée TEST dans cet apprentissage.

Limites comparatives : budgets non égalisés entre A6-C7/8/9,
espace de recherche non exhaustif à deux portes, risque de minimum
local et d'identifiabilité équivalente. Tous ces risques sont à
quantifier, jamais à masquer.

## Dix tâches synthétiques fixées avant exécution

| Tâche | Bits état | Portes cachées utilisées uniquement par le générateur |
|---|---:|---|
| identité128 | 128 | aucune |
| single_and128 | 128 | j92/x3/AND(8) |
| xor_and128 | 128 | j31/x2/XOR(6), j88/x3/AND(8), ancien FAIL A6-C8 |
| and_xor128 | 128 | j33/x2/AND(8), j94/x3/XOR(6) |
| and_and128 | 128 | j41/x2/AND(8), j87/x3/AND(8) |
| xor_or128 | 128 | j29/x2/XOR(6), j105/x3/OR(14) |
| xor_and256 | 256 | j1/x2/XOR(6), j190/x3/AND(8) |
| and_and256 | 256 | j51/x2/AND(8), j181/x3/AND(8) |
| xor_xor256 | 256 | j1/x2/XOR(6), j191/x3/XOR(6), contrôle affine |
| negative_three128 | 128 | j19/x2/XOR(6), j67/x3/AND(8), j108/x4/XOR(6) |

Cinq graines indépendantes, panneaux clean et labels corrompus
par Bernoulli(1/16), donc **100 décisions TRAIN/VAL**. Chaque
TRAIN et VAL : 512 pas ×64 lanes, graines disjointes.
**Les 100 choix sont gelés AVANT génération de tout jeu de TEST**.
Deux TEST indépendants : 4096 et 16384 pas ×64 lanes,
soit 200 lignes de TEST et 40 résumés.

## Critères et contrôles

- Taux d'erreur terminale TEST par tâche/panneau, à 4096/16384 pas.
- Comparaisons directes sur données identiques : baseline identité,
  A6-C7 complet (une porte), A6-C8 affine (deux portes), A6-C9
  conditionnel non linéaire (deux portes), sélection validation.
- Récupération des positions/sources/tables, en signalant les
  équivalences fonctionnelles plutôt que de surestimer l'unicité.
- Ablation de chacune des deux portes et contrôle oracle natif.
- Recherche en candidates, CPU fit/validation, octets utiles d'état
  N×8 pour 64 lanes et coût de test natif bit-lanes/s.
- Le contrôle trois portes doit rester visible même si TRAIN/VAL
  présentent une amélioration fortuite.

Les critères sont préenregistrés et ne seront pas ajustés aux
résultats de TEST. Aucune revendication de précision linguistique,
d'entraînement libre de 256 portes, de débit LLM ou d'accélération GPU.
Rust 1.89 sur runner GitHub hébergé en lecture seule ; aucun logiciel
NVIDIA hors pilotes. Pas de mesure Dell.

**Arrêt : restituer les résultats A6-C9 et leurs échecs avant A6-C10.**
