# A6-E4 — Etape zéro : nouveau corpus indépendant et sélection conjointe sous budget (12 octobre 2026)

**Document d'entrée / gel de méthode AVANT code du futur benchmark A6-E4.** Ceci n'est PAS encore le protocole expérimental définitivement figé. Le corpus est identifié, mais sa taille normalisée, son SHA256 et les dénominateurs de TRAIN/VAL/TEST doivent être **mesurés et inscrits au dépôt par un commit de préenregistrement ultérieur, AVANT le premier code du benchmark, et avant la lecture de TEST**. Ne pas confondre un job d'inventaire de données et un résultat de modèle.

## Question issue des résultats A6-E3

La banque publique ANF128 a sélectionné seulement les **mêmes 8 bits bruts** que RAW32 avec une recherche gloutonne au premier ordre. La perte moyenne du mélange avec ngram4 s'est améliorée de 0.02509970 bit/octet, mais **sans avantage des 96 fonctions composées**. Une sélection gloutonne peut manquer une interaction collective forte même lorsque chaque porte ne présente qu'un intérêt marginal faible. En revanche, transformer de manière déterministe des bits bruts ne crée jamais de nouvelle information sur la cible conditionnellement à l'intégralité des bits originaux : le test doit porter sur une **efficacité de représentation à budget k fini**, non sur une création d'information.

Nouvel objectif : isoler l'effet d'une **recherche conjointe, non gloutonne** sur les fonctions XOR/AND/triple sélectionnées, à **nombre de portes, taille de tables et travail de sélection contrôlés**. Les bras obligatoires seront : RAW32 greedy, RAW32 conjoint, ANF128 greedy, ANF128 conjoint, ngram2 et ngram4 ; aucun accès à la vérité TEST pour choisir des fonctions, seuils, hyperparamètres ou budgets. La mesure doit comparer également les répétitions de groupes et leurs échecs. Le détail des algorithmes, règles de sélection, budget de calcul maximal, seuils de promotion et métriques sera figé dans **un second commit de protocole AVANT implémentation**. Aucune conclusion sur la performance A6-E4 ne peut être affirmée aujourd'hui.

## Séparation nette des deux corpus

**Corpus de diagnostic déjà observé (exploratoire uniquement) :** Jules Verne, Projet Gutenberg #800, corpus corporel SHA256 `b51f36fb2baf0847fd3632fbd6ff939b8858abc1d728b5f86c94d99e64766a74`. A6-E3 a déjà consulté son TEST : il ne peut plus fournir une confirmation nouvelle et aveugle pour une méthode conçue après ses résultats.

**Corpus de confirmation proposé (nouvel auteur/texte, pas consulté sur A6-E3) :** Voltaire, *Candide, ou l'optimisme*, Project Gutenberg eBook **#4650**, français (page : https://www.gutenberg.org/ebooks/4650), source texte :
`https://www.gutenberg.org/cache/epub/4650/pg4650.txt`.
Le catalogue Gutenberg indique une version française, éditeur source Carlo Traverso, mise à jour le 17 novembre 2022. Ce corpus reste différent de l'œuvre de Jules Verne, mais un seul second livre ne prouvera pas une généralisation large.

**Prétraitement figé pour le fingerprint :** HTTPS GET de l'URL précise, normaliser CRLF en supprimant les octets CR (`tr -d '\r'`) et ne conserver que les lignes *après* `^\*\*\* START OF` et *avant* `^\*\*\* END OF`, selon le script A6-E1/E2 existant ; conserver LF et tout le texte historique entre ces bornes, sans autre nettoyage éditorial. Les corps bruts/normalisés ne sont PAS commités. La branche doit archiver les deux SHA256, octets normalisés et nombres d'événements à contexte t≥4 sur 64 voies `CharSplits` (80/10/10), puis figer leur valeur avant code de modèle. Un éventuel changement ultérieur de fichier doit faire **échouer le job** plutôt que changer silencieusement la population.

Aucune exécution A6-E4, aucun TEST sur ce livre ni aucune supériorité sur ngram4 ne sont revendiqués dans cette étape de provenance.

## Gel de provenance effectué après ce document de préparation

La [campagne de fingerprint 38301904119](https://github.com/Memorithm/SML-GENIUS/actions/runs/38301904119) a effectivement réussi, **sans exécuter de modèle**, et les valeurs sont maintenant enregistrées dans [A6-E4-CANDIDE-CORPUS-FROZEN.md](A6-E4-CANDIDE-CORPUS-FROZEN.md) : corps normalisé **210655 octets**, SHA256 `48e04595f5fc681ef6c0498123180bce0125d8fbe2167f3bcaeef189b45bd4b9`, **168256 TRAIN / 20800 VAL / 20800 TEST**. Le script `scripts/fetch-candide-corpus.sh` refusera tout contenu différent ; le workflow valide désormais le même fingerprint par deux procédures.

Le protocole du futur sélecteur conjoint, ses budgets, ses alternatives et ses critères doivent **encore** être figés dans un commit spécifique avant tout code/modèle. Cette étape de métadonnées ne contient **aucune** performance de modèle A6-E4.
