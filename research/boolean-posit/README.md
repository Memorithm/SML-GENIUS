# BPB-0 — Boolean Posit Block: protocole expérimental (2026-11-06)

Statut : **prototype isolé ; aucune modification du runtime ou des poids SML**.

## Hypothèse réfutable

Une représentation de 64 coefficients quantifiés via plans de bits, avec un exposant dyadique commun, peut calculer exactement un produit scalaire sur les valeurs quantifiées, sans multiplication flottante ni conversion flottante dans la boucle interne. Le gain en trafic mémoire et en latence exige une qualification expérimentale distincte.

ATTENTION : « Boolean Posit » est un nom de travail. BPB-0 n'est PAS un posit conforme au standard de 2022 : pas de régime unaire individuel, pas de précision effilée, pas de calcul arithmétique posit générique. Son rôle est celui d'un contrôle simple et auditable.

## Format binaire figé BPB-0

- N=64 coefficients, entiers signés dans [-3,3].
- E est un exposant commun sur 8 bits signés.
- Trois masques u64 : S, M0 et M1 (signes et deux plans de magnitude).
- Valeur i = (-1)^S[i] * (M0[i] + 2*M1[i]) * 2^E.
- Zéro négatif rejeté à la désérialisation.
- Wire format **25 octets** : E sur 1 octet ; S, M0, M1 en little-endian sur 8 octets chacun.
- 25*8/64 = **3,125 bits/valeur**. L'alignement de la structure en RAM peut entraîner un coût supérieur : à mesurer.
- Produit scalaire : somme_i(a_i*b_i) * 2^(Ea+Eb), avec résultat (mantissa: i32, exponent: i16).
- Par borne : abs(somme_i(a_i*b_i)) <= 64*3*3 = 576.
- Aucun facteur d'échelle FP16/FP32 dans la boucle interne ; l'entraînement et la quantification à partir de FP restent hors du contrat initial.

Pour D=SA XOR SB, Pjk=Mj(A) AND Mk(B), C(P,D)=popcount(P)-2*popcount(P AND D).
Le coefficient entier du produit est C(P00,D)+2(C(P01,D)+C(P10,D))+4C(P11,D).
Le noyau utilise quatre intersections de plans, huit POPCOUNT, un XOR des signes, des additions et des décalages.

## Vérification requise

Commande reproductible :

    rustc --edition=2021 --test research/boolean-posit/bpb0.rs -o /tmp/bpb0-tests && /tmp/bpb0-tests

Tests inclus :
- 49 couples de coefficients constants (7x7), sur 64 voies chacun ;
- 500 paires déterministes de vecteurs non uniformes, comparaison avec oracle scalaire en i32 ;
- encodage/décodage et sérialisation réversible ;
- rejet du zéro négatif ;
- bornes de magnitude et d'exposant.
Ces tests ne démontrent pas une accélération physique ni une équivalence de qualité d'un réseau.

## BPB-1 : extension proposée, encore NON implémentée

1. Ajouter des régimes/exposants variables par sous-bloc ou coefficient, décodeur borné, masques et index comptabilisés.
2. Allouer la précision à partir de la sensibilité à la perte (TRAIN/DEV), en distinguant importance sémantique et amplitude du coefficient.
3. Comparer au même budget d'octets les contrôles uniformes, les contrôles guidés par amplitude et les contrôles guidés par sensibilité.
4. Définir le comportement exact de la somme de plusieurs blocs à exposants différents, des arrondis et des dépassements.
5. Preregistrer mesures à même modèle, données, matériel, et qualité : erreur de quantification, log-loss, vitesse, pages lues, octets effectivement transférés, RSS, défauts de pages, CPU cycles et énergie uniquement avec un compteur fiable.
6. Inclure MX/OCP, GGUF/ggml et quantification ternaire comme antériorités ; la mutualisation d'échelles dyadiques n'est pas nouvelle.
7. N'exécuter les mesures Dell/Thor/macOS que via les mécanismes autorisés de Memorithm/RemoteOps. Aucun SDK NVIDIA requis.

## Barrières de promotion

- BPB-0 : exactitude des valeurs QUANTIFIÉES, contrôlée par oracle.
- BPB-1 : évaluation prospective, qualité appariée, coûts complets y compris métadonnées et déquantification.
- SML : intégration séparée après validation selon AGENTS.md, feuille de route off-main, docs/ROADMAP.md et souveraineté PVP. Aucun benchmark de composant ne prouve un gain de qualité de modèle.

## Antériorités et sources

- Posit Working Group, Standard for Posit Arithmetic (2022-03-02) : https://posithub.org/docs/posit_standard-2.pdf
- Rouhani et al., OCP Microscaling Formats (MX) v1.0 (2023-09) : https://www.opencompute.org/documents/ocp-microscaling-formats-mx-v1-0-spec-final-pdf
- ggml-org, Tensor Encoding Schemes : https://github.com/ggml-org/llama.cpp/wiki/Tensor-Encoding-Schemes
- Wang et al., BitNet: 1-bit Pre-training for Large Language Models, JMLR 2025 : https://www.jmlr.org/beta/papers/v26/24-2050.html
