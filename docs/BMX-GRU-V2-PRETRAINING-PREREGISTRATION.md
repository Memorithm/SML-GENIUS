# BMX-GRU-v2 — préenregistrement de la reprise sur Dell (2026-10-15)

État : **protocole proposé avant entraînement v2 ; données v2 non admises ; TEST v2 fermé.**
Autorité scientifique : Memorithm/SML-GENIUS ; contrôle matériel/exécution : Memorithm/RemoteOps.
Ce document ne transforme pas le panel GRU-v1 déjà ouvert en nouveau TEST indépendant.

## Référence immuable et hypothèse

Contrôle figé : GRU-v1 Dell (RemoteOps training run `38486923303`), checkpoint d'origine
SHA-256 `b570f6a81778843e53e494a46396cba07ad80b2f183b2102691d3bf7cee56efe`,
export natif `6ad7d53991d0b99dfce47522e3e33dd1a0ddbed83a2f7d67e8a6e7b9e865357c`.
Le protocole de comparaison initial et son panel TEST figurent dans
`BMX-GRU-INDEPENDENT-PROTOCOL.md`; ils sont **exposés**, et ne doivent
pas guider le choix du checkpoint, des poids, du mélange ou des hyperparamètres v2.

L'hypothèse v2 est qu'un corpus complètement traçable, de vraies sources Rust
et un meilleur choix de checkpoint sur validation permettent d'obtenir un
GRU conditionnel au moins aussi bon que v1 **à architecture et coût égaux**.
C'est une hypothèse ; aucun résultat v2 n'est encore mesuré.

## Garde préalable G0 — manifeste et droits

Avant tout job d'entraînement, publier sur un commit scientifique immuable
un manifeste versionné `memorithm.bmx-gru-v2-dataset/v1`, son SHA-256 exact
et l'empreinte du code de préparation. Chaque document source doit décrire :
ID, domaine FR/EN/RS, auteur ou dépôt/source, identifiant de groupe, licence,
URI/revision immuable, hash et taille du brut, chemin/hash/taille des octets
effectivement consommés et split TRAIN/VAL/TEST.

- Les chemins sont relatifs à un répertoire de staging privé ; aucun lien
  symbolique ni traversée n'est admissible. Les octets sont hachés sans
  normalisation implicite ; chaque transformation doit être versionnée.
- Une œuvre ou un dépôt entier appartient à **un seul split**. Rejeter les
  répétitions exactes entre splits, les segments ou transformations connus,
  les duplicats de hash et les corpus avec origine/licence inconnue.
  Ajouter un audit des duplications proches et des exclusions des familles
  Gutenberg / dépôts déjà exposées. L'ancien corpus Dell MMD + stdlib + crates
  n'ayant pas de manifeste source complet, son absence de chevauchement ne
  peut pas être attestée rétrospectivement.
- Interdire explicitement la réutilisation du panel GRU-v1 (Sand, Daudet,
  Huysmans, Loti, Dolbear, Tyndall, Lodge, Maxwell, tokio, rayon, serde,
  regex), et les sources ouvertes par ENG-1c/ENG-1e, pour les **nouvelles
  unités TEST**. Un contrôle sur noms ne remplace pas l'audit de contenu.
- TRAIN doit contenir des **octets Rust réels** (v1 déclarait `rs=0`);
  les appartenances de domaine et les proportions sont auditées.
  Budget cible G0 : TRAIN ≥40 MiB au total, chaque domaine FR/EN/RS ≥2 MiB,
  VAL et TEST ≥1 MiB chacun, avec au moins deux groupes distincts par split.
  Ces seuils sont des exigences d'admission, non une preuve de qualité.
- Ne jamais inclure les octets TEST dans le répertoire/arguments accessibles
  au processus d'entraînement. Le vérificateur d'intégrité peut hacher les
  octets TEST sans calculer de prédiction. L'accès TEST pour score est autorisé
  seulement après gel des candidats et du verdict.
- Un échec d'empreinte, une source disparue, une licence ambiguë, un corpus
  incomplet ou une contamination interdit G1. Aucune substitution silencieuse.

## G1 — apprentissage borné et comparaison équitable

Architecture témoin inchangée : vocabulaire 256 octets, embedding 64,
une couche GRU de 512, projection 256, soit 1 035 520 paramètres.
Le GRU-v1 reste figé ; v2 est un autre poids, pas une réécriture du témoin.
La recette initiale (voir `RemoteOps/ops/bmx-gru/train.py`) utilisait
Adam, OneCycleLR, séquences de 256 octets, batch 128, seed 1 (torch) et
3 (NumPy), six époques. Une nouvelle recette ne doit pas s'attribuer
la reproductibilité v1 sans manifeste/toolchain et empreintes d'exécution.

Avant le premier fit v2, enregistrer sur une révision distincte :
optimizer et learning rate, seeds, taille et ordre des lots, nombre maximal
de pas/époques, intervalle de validation, seuil/patience et départage du
meilleur checkpoint, dimensions, enveloppes Dell (GPU/CPU/RAM/disque/temps)
et gestion de crash. Pour la première ablation, **ne varier qu'une chose** :
checkpoint du meilleur VAL (contre checkpoint final au même entraînement).
Ne pas choisir la politique à partir du TEST ou du panel GRU-v1 connu.
Conserver tous les checkpoints comparés et les durées ; aucune valeur de
`val_bpc` intermittente n'est une preuve de généralisation.

Le job doit exiger l'identité immuable du code, l'empreinte du manifeste,
le compte non nul des octets Rust, et un espace d'exécution non-root borné.
Aucun téléchargement `pip` flottant, `git clone --depth 1` sans SHA,
corpus provenant automatiquement de `~/.cargo`, ou dépendance non épinglée
n'est admis dans G1. Un moteur d'apprentissage Python/PyTorch/CUDA hérité
n'est **pas** une qualification du chemin de formation Rust-only / pilote
GPU seul ; cette souveraineté demeure un jalon distinct.

Les artefacts publiables sont des hashes, checkpoints explicitement autorisés,
métadonnées de provenance et courbes compactes ; ni données corpus brutes,
ni secrets, ni contenu TEST dans les logs. Le workflow Dell doit rester manuel,
avec concurrence exclusive et plafonds de ressources. Thor n'est pas un repli
automatique (échec CUDA Thor dans le run historique).

## G2 — gel avant TEST et verdict

Une fois G1 terminé, figer exactement les poids v2, le choix du checkpoint,
les binaires de conversion et de lecture Rust, les comparateurs et les jeux
TEST indépendants. Le GRU-v1 et BMX-v5 seuls sont les témoins ; le placebo
uniforme identifie l'effet de l'ajout d'une entrée au mixeur.

Mesure principale : gain de longueur conditionnelle
`(v5.model_bits - (v5+gru-v2).model_bits)/TEST_bytes`, en bits/octet.
Rapporter séparément les bits du modèle, la taille physique incluant les
poids, l'empreinte RAM et la latence. Pour décider une amélioration **par
rapport à v1**, calculer un contraste apparié v2–v1 sur les mêmes nouveaux
TEST et avec le même chemin d'inférence ; ne pas comparer des panels
différents comme si les observations étaient appariées.

Avant l'ouverture des nouveaux TEST, publier le roster, les tailles/splits,
les unités d'inférence (œuvre ou dépôt, pas bloc d'octets), le seuil pratique
et la règle bootstrap/permutation. La valeur exploratoire `+0.249141`
bit/octet de v1 sur douze unités connues sert seulement de contexte, pas
de seuil rétroactif du nouveau panel. Rapporter les échecs, domaines en
régression, sources manquantes et vérifications d'aller-retour du codeur.
Les poids ne sont jamais promus sur un résultat DEV seul.

## G3 — contrôle de publication

La publication organisationnelle de `main` utilise maintenant le
signataire `memorithm-sml-production` (PR #565) ; le workflow vert
`38663322412` a signé et fait admettre une **page booléenne de démonstration
de 65 octets**, pas des poids GRU. L'admission de poids v2 réels est une
qualification séparée exigeant format signé, identité d'architecture,
SHA du checkpoint/export, reproduction du décodage et reçu externe.
L'identité authentifiée ne prouve ni qualité scientifique ni droit de merge.

## Critères de fin de préparation

- [ ] G0 : inventaire complet sourcé, licence/exclusions, SHA, seuils, splits
      et audit de chevauchement ; validateur et tests négatifs verts.
- [ ] G1 : recette/toolchain verrouillées, isolation/budgets et sorties d'un
      entraînement v2 borné sur Dell, checkpoint best/final conservés.
- [ ] G2 : candidats et nouveau panel indépendant préenregistrés avant
      toute lecture des pertes TEST ; mesure appariée et coûts complets.
- [ ] G3 : preuve d'admission des **vrais** poids si promotion décidée.

Aucun de ces quatre critères n'est réputé rempli par la seule fusion
du présent document.
