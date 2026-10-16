# SML-1 → SML-2 : noyau SGD réellement appris, pages localisées, expérimentation 50M

Date : 2026-10-16. Branche de recherche Rust. **Ce n'est pas encore une preuve d'un modèle SML-2 de 50M appris ni de SML-25B.**

## Point de départ vérifié

Le premier entraînement supervisé sur le Dell, [RemoteOps #38944419280](https://github.com/Memorithm/RemoteOps/actions/runs/38944419280), entraîne le modèle `sml_sparse_sgd_v1` sur un préfixe financier SEC à la source SHA-256 `c890b0cd08e9d67d4ea4ca9a88e760282c6de6c1a8f74ebfad6c0d1a2c740ed9`. **4 194 304 poids f32 présents**, **460 219 poids distincts effectivement mis à jour** après **330 608 792 opérations de gradient**. TRAIN : 6 documents / 2 430 959 octets / 4,042938350 bits/octet préquentiels. Tranche diagnostique hors documents TRAIN de la même année : 3 documents / 1 108 782 octets / 4,984848415 bits/octet. Ce n'est pas un score financier chronologique. L'[artefact 12064911830](https://github.com/Memorithm/RemoteOps/actions/runs/38944419280/artifacts/12064911830) conserve les sorties sans texte brut.

## Nouveau code

1. `train_document_grouped(document, batch_bytes)` fait le **même SGD sparse exact** que le moteur de référence, mais groupe les mises à jour par `page_id` dans des blocs de prédiction `1..=8192` octets. Le tri stable conserve l'ordre chronologique à l'intérieur de chaque page. Les pages différentes ne partagent aucun poids ; leurs gradients sont donc indépendants. Dans des conditions identiques, les maîtres f32 et le bitmap des poids modifiés doivent rester **identiques octet pour octet** à l'original malgré beaucoup moins de fautes de pages. Test de régression explicite sur un modèle 64 pages avec cache = 1.
2. `QuantizedSparseAtlas` lit le vrai export Q4 sérialisé (512 octets par page), signe les nibbles `[-8;7]` et déquantifie en valeurs f32 correspondantes par `/2` pendant la prédiction. **Aucun master f32 n'est chargé pendant le score Q4**. Limite de cache explicitement vérifiée, et poids Q4 immuables. Ce mécanisme mesure la dégradation réelle `Q4 - F32` sur les mêmes documents non entraînés.
3. `sml-sparse-gradient-fit --pilot-50m` crée effectivement **48 829 × 1 024 = 50 000 896 poids maîtres f32** dans un fichier de **200 003 584 octets**, avec export Q4 de **25 000 448 octets** et bitmap unique par poids. C'est **la capacité entraînable**, et non le nombre de poids touchés ni appris. Le nombre distinct de poids effectivement modifiés doit apparaître dans la sortie.
4. Chaque document conserve SHA et frontière propres, le corpus entier est comparé à son SHA d'origine ; les échantillons restent petits (~2 Mio TRAIN, ~1 Mio diagnostic distinct) pour valider la montée d'échelle des paramètres sans prétendre qu'un pré-entraînement de 50M a été accompli. L'entraîneur conserve la prise en charge `--pilot` 4M et son régime SGD non groupé pour comparaison.
5. Rapport machine mesuré : perte TRAIN, perte F32 et Q4 sur documents hors TRAIN (même source/année), nombre de pages chargées/évincées, temps, poids réellement touchés, proportion, dimensions physiques, `TEST=UNOPENED`, `G0=NOT_ADMITTED`.

## Limites scientifiques bloquantes

- **Les adresses sont encore des hachages déterministes des deux octets précédents et du bit à prédire**. Ce routage n'est pas MOR appris. Le noyau apprend 17 features logistiques par bit et par page ; pas encore un réseau ANF récurrent.
- Le 50M a une **capacité** de 50M mais n'expose qu'un ensemble partiel de paramètres aux exemples. Une augmentation du fichier sans hausse de performance held-out n'est pas une progression scientifique.
- L'apprentissage est en f32 ; la précision Q4 n'est mesurée qu'après le fit et n'est pas entraînée en quantization-aware mode. L'atlas 25B SML cible les mélanges booléens/ternaires/résiduels, pas 25B floats.
- Le jeu SEC reste exploratoire (droit d'utilisation sous-jacent non admis G0). Le TEST chronologique 2024 est fermé ; la tranche diagnostique de la campagne n'est pas un DEV financier indépendant.
- L'objectif **50M n'est admis** qu'après preuves de qualité sur un DEV indépendant, ablations contre le 4M à budget identique, comptage exact des poids touchés, profilage I/O, reprise en incident, puis MOR/ANF/récurrence avec gradient mesurable.

## Gates d'admission technique

Rust fmt, Clippy, tests unité, intégrité bit à bit du SGD groupé, test de perte Q4 sur données réellement quantifiées, checkpoint relecture, plafond de RAM pages actives, et essai Dell de `--pilot-50m` avec source figée. Publier master et bitmap sur Dell, mais seulement artefact Q4 (~25 Mo) et preuve sur GitHub. Ne pas fusionner avant CI verte et métriques physiques de Dell.
