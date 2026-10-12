# A6-E5 — Flaubert, *Madame Bovary* (français), corpus aveugle figé

**Empreinte obtenue le 12 octobre 2026 AVANT toute implémentation du modèle A6-E5.** GitHub-hosted [run 38304625486](https://github.com/Memorithm/SML-GENIUS/actions/runs/38304625486), job 114995558934 **SUCCESS**, a téléchargé le corps d'un livre et calculé **uniquement des SHA256 et des nombres d'octets**. Aucun résultat de log-loss, aucune hypothèse ANF, aucune distribution de cibles ni aucun TEST de modèle A6-E5 n'ont été calculés.

- Œuvre : Gustave Flaubert, *Madame Bovary* **en français**, [Project Gutenberg #14155](https://www.gutenberg.org/ebooks/14155). Ne pas utiliser le #2413, qui est une traduction anglaise.
- URL de source immuable dans les scripts : `https://www.gutenberg.org/cache/epub/14155/pg14155.txt`.
- SHA256 du fichier brut téléchargé : `93f0a2259dbaf5fde6e7b96adc22346d52f80eb1e9319336c8381a045efe40cd`.
- Normalisation : supprimer chaque octet `CR` puis retenir exactement les lignes entre les marqueurs `^\*\*\* START OF` et `^\*\*\* END OF` sans nettoyage éditorial, UTF-8 inchangé, conservation des LF.
- Corps normalisé : **717674 octets**, SHA256 `7aa8b5f8871e46d88d3715dcbfd4e8e46cc8ba89218b134454b0778df4942eb8`.
- Coupage `CharSplits` 80/10/10, `ChunkLayout` à 64 voies de longueur entière, prédictions t≥4 sur **256 symboles** : **573824 événements TRAIN, 71488 VAL, 71488 TEST**. Les éventuels derniers octets hors longueurs de voies entières sont exclus uniformément ; ne pas les réinjecter ultérieurement.

**Provenance vérifiable :** branche de fingerprint au commit `d86b69792be2e08e0d7e565a64572e6392d6beca`. Ne pas modifier la méthode ni ces constantes après observation des labels TEST. Si Gutenberg met à jour ses fichiers, le nouveau banc doit **échouer fermé** et préserver la référence existante ; l'utilisateur peut investiguer la dérive de source par un changement de protocole explicite, jamais par un auto-fallback non contrôlé.

**Séparation expérimentale :** Verne E3 a déjà été évalué ; Candide E4 a son propre protocole préenregistré ; Flaubert est destiné exclusivement au TEST confirmatoire nouveau d'A6-E5. Les modèles de référence et tous les bras utiliseront exactement la même découpe Flaubert. Le corpus est une œuvre littéraire, pas une population d'événements indépendants : la répétition des runs ou l'agrégation naïve de 71 488 octets ne suffit pas à estimer la variance hors document.
