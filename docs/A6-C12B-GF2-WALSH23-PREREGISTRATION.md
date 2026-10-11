# A6-C12b — Préenregistrement : Gauss GF(2), paires/triplets Walsh et coût n,k

**Date : 12 octobre 2026. PROTOCOLE GELÉ AVANT TOUT CODE ET TOUT RÉSULTAT A6-C12b.**

Dépendance scientifique : A6-C12a (PR #492 fusionnée, rapport `docs/RESULTS-A6-C12A-SPECTRAL-ADVERSARIAL.md`). Les deux tâches adversariales XOR3 et AND→XOR→XOR sont exactement représentables, mais le filtre Walsh+racine+peeling existant a échoué à retrouver leurs sources dans le top24 sur 0/20 configurations corrélées par tâche.

## Questions testées, sans modifier A6-C12a

1. **Parité propre** : une élimination de Gauss sur GF(2) appliquée aux observations TRAIN `p_t=z_t XOR R_(t-N)` reconstitue-t-elle trois sources sans corrélation faible degré ?
2. **Interaction non linéaire** : les corrélations Walsh sur **paires arbitraires de variables retardées** identifient-elles les sources XOR de `(R∧a) XOR b XOR c`, puis une factorisation ANF à trois portes retrouve-t-elle les trois portes ?
3. **Référence exhaustive d'ordre 3** : quelle est la qualité et le coût mesurés de l'énumération de **tous les triplets de sources** de l'espace préfixé ? Même méthode sur données propres/bruitées, sans prétendre résoudre le LPN générique.
4. **Échelle n,k** : comparer les nombres d'hypothèses et temps CPU observés avec n∈{32,64,128} pour les combinaisons d'ordre k=2/3. Ne **pas** extrapoler une constante mesurée vers 508, 512, 1024 ou des millions de fonctions. Reporter séparément les comptes théoriques C(n,k), les comptes admissibles à positions distinctes et les mesures.

**Important** : la présence d'un bruit indépendant `e_t` et l'emploi teacher-forcé de `R_(t-N)` bruité font intervenir `e_t XOR e_(t-N)` dans `p_t`, d'où un taux de corruption **15/128 = 11,71875 %**, soit une corrélation idéale `(1-2/16)^2 = 49/64 = 0,765625` pour la bonne parité, et **non 0,875** (qui ignore le bruit de l'observation passée). L'évaluation autonome TEST compare les réseaux au bruit des étiquettes seul, nominal 1/16.

## Grammaire et grille de sources préfixées

Comme A6-C12a, le générateur et l'apprenant utilisent un **anneau de 128 états natifs**, racine et lecture fixées ; seulement trois mutations booléennes internes sont autorisées. Trois positions **alignées sur une grille indépendante des données** sont fixées à j20, k68, l116, avec canaux bruts x2, x3, x4. Cet exemple **ne réutilise pas les positions exactes j19, j67, j108 du banc précédent** ; les erreurs des deux bancs ne sont donc pas directement comparables.

L'espace de recherche est annoncé à l'apprenant comme une grille de positions de porte : il ne reçoit pas les positions effectivement mutées, les canaux ou les tables. Les positions candidates :
- **n=32 caractéristiques** : 8 portes `4+16i`, i=0..7 ×4 canaux x1..x4 ;
- **n=64** : 16 portes `4+8i`, i=0..15 ×4 canaux ;
- **n=128** : 31 portes `4+4i`, i=0..30 et porte 127, chacune ×4 canaux.

Ces sous-ensembles sont imbriqués et contiennent tous les vrais canaux (par construction de la tâche alignée sur une grille), sans transmission des paramètres du générateur aux algorithmes. Les comparaisons mesurent la montée en coût **à hypothèse de grille connue**, pas la recherche libre parmi les 508 positions×canaux originelles.

Les quatre tâches (toutes représentables à trois portes) :
- `xor_xor_xor` = `R XOR a XOR b XOR c`, tables `[6,6,6]` ;
- `and_xor_xor` = `(R AND a) XOR b XOR c`, tables `[8,6,6]` ;
- `xor_and_xor` = témoin `[6,8,6]` ;
- `and_and_and` = témoin `[8,8,8]`.

Le générateur est **LocalAnfNetwork** natif et produit uniquement les étiquettes finales et les inputs aux apprenants. Aucun apprentissage sur états cachés, aucune fuite de vérités de portes, aucune fuite TEST.

## Méthodes comparées, fixées AVANT résultat

### GF2-Gauss : témoins propres uniquement

Pour chaque n, construire des colonnes de 16 pas d'observation ×64 lanes = 1024 équations binaires, t≥128, correspondant aux caractéristiques retardées. Résoudre exactement le système `A·theta=p` par élimination GF(2) avec provenance des colonnes ; ensuite **vérifier l'équation sur TOUS les pas TRAIN**. La méthode peut retourner une combinaison de taille arbitraire ; ne proposer une architecture que si la solution exacte a **exactement trois sources à positions distinctes**. Appliquer alors la même factorisation ANF sans révéler les tables vraies. Les tâches non affines sont contrôles négatifs de cette voie.

**Ne pas appliquer Gauss à des labels bruités comme si le système restait exact** ; produire `SKIPPED_NOISY`. Ce n'est pas un solveur LPN.

### W2-PAIR : toutes paires arbitraires puis factorisation

Calculer sur TRAIN, avec p=`z XOR Rpast`, le score `|sum(-1)^(p XOR feature_i XOR feature_j)|` de **toutes** les paires admissibles à positions distinctes. Classer par score décroissant puis lexicographie des sources (déterminisme). Garder **les 8 meilleures paires** ; pour chacune, ajouter tour à tour une troisième caractéristique de position distincte parmi les n candidats. Dédupliquer, évaluer chaque structure à trois portes par histogramme conditionnel 16 adresses + toutes les 520 fonctions composées possibles, puis exécuter les **16 meilleures propositions** en récurrence native TRAIN et sélectionner sur VALIDATION contre le contrôle identité. Cela garantit que la méthode ne connaît pas à l'avance les tables XOR/AND.

Report : rang (1-based) de la vraie paire des deux sources XOR (`b,c`) pour `and_xor_xor`, appartenance au top8, vraie structure dans les candidats proposés, nombre réellement factorisé, paramètres et erreur TEST.

### W3-EXHAUSTIVE : tous triplets admissibles puis factorisation

Calculer `|sum(-1)^(p XOR feature_i XOR feature_j XOR feature_k)|` pour **tous** les triplets de caractéristiques à positions distinctes. Trier sur score décroissant puis lexicographie ; retenir **16 meilleurs triplets**, factoriser par la même banque de 520 compositions et utiliser le même rescore natif/VALIDATION. Mesurer rang exact de la vraie structure (1-based), comparaison top16, nombre total de combinaisons évaluées et temps de classement. Ne pas masquer les erreurs sur les fonctions non affines.

À n=128, la référence brute `C(128,3)=341 376` comprend aussi les combinaisons invalides où deux canaux occupent une même porte. Après restriction commune de 32 positions×4 canaux, **317 440 triplets admissibles**. À l'échelle originelle 508 candidats, **C(508,3)=21 720 556** et **C(127,3)×4³=21 336 000** structures admissibles, **non exécutées dans ce protocole**.

Les coûts de filtrage, factorisation, simulation native et validation sont comptabilisés séparément ; budgets de calcul non égalisés entre méthodes. Les comparaisons de vitesse restent descriptives tant que les opérations effectives diffèrent.

## Jeux de données, arrêt et statistiques

- **4 tâches ×2 panneaux ×3 graines = 24 flux TRAIN/VAL distincts**, et **24 flux TEST indépendants** générés APRÈS toutes les décisions gelées.
- **512 pas TRAIN**, **512 VALIDATION**, **4096 TEST**, 64 voies bit-slicées. Pour le score spectral, seuls les 384 pas t≥128 disposent de la racine passée observée. Noise Bernoulli 1/16 avec PRNG de bruit indépendant du PRNG d'entrées.
- Par flux, n32/64/128 et W2/W3, plus Gauss propre ou SKIPPED_NOISY : **24×3×3=216 tentatives de méthode** (144 W2/W3 ; 72 Gauss, dont 36 sautées). Le nombre de modèles effectivement valides issus de Gauss est un résultat à mesurer et **ne doit pas être préfixé**.
- Les flux TEST sont communs aux méthodes comparées : **216 lignes de comparaison TEST**, dont des valeurs `NA` explicites lorsque la méthode n'a pas proposé de réseau. Les essais de tailles n différentes partagent les mêmes flux. Ces 216 lignes ne sont pas des expériences indépendantes.
- Scores et sélection figés avant TEST ; aucun réglage adaptatif, aucune sélection sur TEST. Toute correction de protocole après le premier RUN doit être datée comme amendement et séparée des résultats initiaux.

**Critères primaires préfixés** :
(i) parité propre : Gauss retrouve EXACTEMENT les trois sources (et la factorisation doit reproduire une dynamique native zéro erreur propre),
(ii) mixte : W2 et W3 trouvent la vraie structure parmi leurs ensembles proposés et le modèle sélectionné reproduit l'oracle sur TEST,
(iii) parité bruitée : W3 retrouve ses trois sources et obtient l'erreur TEST du générateur exact, avec décompte indépendant par graine ;
(iv) mesure `n→C(n,2), C(n,3), elapsed_ms` sans prétendre O(n^k) pour d'autres algorithmes.

Publier **tous les échecs** (y compris absence de source, échec de factorisation, refus sur validation, ou erreur TEST), taux exacts par tâche/panneau/graine et CI. Si la même fonction peut s'exprimer par tables différentes, séparer égalité des paramètres canoniques, exactitude des sources, et erreur fonctionnelle.

## Portée et garde de sécurité

Rust 1.89, runner GitHub-hosted isolé, checkout pin, permissions `contents:read`, aucun exécuteur Memorithm auto-hébergé pour code PR. Pas de CUDA ni runtime logiciel NVIDIA. N128 à topologie connue, aucune mesure SML-25B, modèle linguistique, GPU ou banque ANF massive. Cette campagne est un banc de **recherche falsifiable**, pas une intégration en inférence.

**Garde de fusion** : code et observations complet, tests unitaires et CI verts ; publier d'abord toutes les mesures et limitations avant merge.
