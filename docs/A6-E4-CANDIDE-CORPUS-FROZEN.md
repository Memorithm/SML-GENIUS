# A6-E4 — corpus français indépendant fixé AVANT tout code du modèle (Voltaire, Gutenberg #4650)

**Date du gel : 12 octobre 2026. Statut : données gelées, expériences de modèle non lancées.** Le [workflow GitHub-hosted 38301904119](https://github.com/Memorithm/SML-GENIUS/actions/runs/38301904119), job `fingerprint` **114986579324**, a réussi en lecture seule sur le SHA `fc77a4d1d1ef84e55adc2ab0dc7821ec252fa866`. Il a vérifié les bornes Gutenberg, calculé des empreintes SHA256 et les nombres d'observations **sans entraîner ni tester aucun modèle**.

- Ouvrage : Voltaire, *Candide, ou l'optimisme*, Projet Gutenberg [ebook #4650](https://www.gutenberg.org/ebooks/4650), texte français. Source HTTPS unique : `https://www.gutenberg.org/cache/epub/4650/pg4650.txt`.
- **SHA256 fichier source original :** `1cc10ab3268a76aa01bd3f9722ae2ae0e5e36b869a18b0bb0832b75bc0327806`.
- Algorithme de normalisation : suppression de tous les octets CR (`tr -d '\r'`), extraction des lignes exclusivement comprises *après* le marqueur `^\*\*\* START OF` et *avant* le marqueur `^\*\*\* END OF`, en préservant les LF et tous les caractères historiques du corps. Aucune retouche manuelle, correction orthographique ni tokenisation. Cette convention est celle du script existant pour le corpus Verne.
- **Corps normalisé exact : 210655 octets ; SHA256 :** `48e04595f5fc681ef6c0498123180bce0125d8fbe2167f3bcaeef189b45bd4b9`.
- Découpage chronologique `CharSplits` 80/10/10, 64 voies non chevauchantes et prédiction uniquement pour t≥4 : **168256 événements TRAIN, 20800 VAL, 20800 TEST**. Le reliquat de `len/64` non exploitable en fin de split reste exclu comme dans A6-E3. Ces nombres ont été calculés sans consulter les étiquettes de TEST.

**Règle de reproductibilité :** le chargeur refuse toute taille ou empreinte incorrecte. Ce second corpus est une population linguistique différente de Jules Verne #800 dont le TEST A6-E3 a déjà été analysé ; il est choisi pour une évaluation nouvelle. Deux exécutions du **même** corpus n'apportent pas deux populations indépendantes. La future grammaire/apprentissage/recherche jointe doit être préenregistrée dans un autre commit **avant tout code expérimental** et sans changer ces hashes à partir des résultats.

Références : [workflow GitHub Actions 38301904119](https://github.com/Memorithm/SML-GENIUS/actions/runs/38301904119), [Project Gutenberg, Candide #4650](https://www.gutenberg.org/ebooks/4650).
