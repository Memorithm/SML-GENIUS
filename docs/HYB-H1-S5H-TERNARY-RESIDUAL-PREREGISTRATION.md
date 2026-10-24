# HYB H-1 — tête ternaire résiduelle sur le corpus S5h, couplée à GB-11 P1 : préenregistrement

**Préenregistrement gelé le 24 octobre 2026, avant toute exécution P1 et H-1, sans aucun accès au DEV S5h.**
Le 24 octobre 2026, le propriétaire a choisi de lancer GB-11 P1 et H-1 ensemble. Ce document suit `HYB-H0-TERNARY-BITPLANE-PREREGISTRATION.md` (option B, noyau `ternary_bitplane` fusionné en #677) et s'appuie sur GB-11 P1 (`GB11-RELAXED-SPARSE-ANF-LEARNING-PREREGISTRATION.md`). Le protocole P1 lui-même appartient à GB-11 ; H-1 ne fait que lire ses traces.

## Question

Sur du texte réel, une tête neuronale ternaire qui lit l'état SBG d'un modèle booléen appris apporte-t-elle du BPC par rapport :
- au booléen seul ;
- à une table booléenne de même taille ?

C'est la porte go/no-go de l'option B. Si la réponse est non, H-2 (experts de page neuronaux routés par MOR) n'est pas lancé.

## Données

- Corpus S5h (RemoteOps `docs/SML_50M_S5H_PES2O_MIX_TRAIN_20261022.md`) : TRAIN et DEV S5h tels que matérialisés sur le Dell. 1 article sur 32 va au DEV, plafonné à 32 Mio. Les données ne quittent pas le Dell.
- TEST reste fermé. Aucun hyperparamètre n'est choisi sur le DEV.

## Traces du run P1 (format figé avec GB-11 le 24 octobre 2026)

Le binaire `sml-gb11-p1-s5h` est lancé par le workflow RemoteOps `sml-gb11-p1-s5h-dell.yml`. Le manifeste doit être identique à celui du témoin (`966eb65f…`) : 60 237 946 positions DEV, et les positions prédites sont les mêmes (octets 2..len de chaque fichier).

Le run écrit ses traces dans `~/sml-artifacts/sml-gb11-p1-s5h-<sha>/` :
- `trace-dev.bin` contient toutes les positions DEV, calculées avec le modèle P1 gelé.
- `trace-train.bin` contient les fichiers TRAIN d'index % 20 == 0, avec des logits **préquentiels** (calculés avant la mise à jour).

Chaque enregistrement fait 40 octets, en little-endian :

| Champ | Type |
| --- | --- |
| doc_index | u32 |
| position | u32 |
| split | u8 |
| cible | u8 |
| registre W avant la cible | u16 |
| contexte c1..c8 | u64 |
| logits P1 des 8 décisions MSB du chemin vrai, en ln-odds × 256 | 8 × i16 |
| confiance, 4 bits par banque | u32 |

`trace.json` donne les SHA-256 (poids, manifeste, traces), le commit source, l'échelle et la règle de saturation des logits.

Avant tout bras, H-1 recalcule depuis les traces le BPC P1 rapporté par `trace.json`, à 1e-6 près. Le run est invalide si :
- ce recalcul échoue ;
- un couple `doc_index + position` apparaît en double ;
- un `doc_index` apparaît dans les deux splits.

`TRAIN-HOLD` désigne le dernier 1/16 des fichiers de `trace-train.bin`, dans l'ordre de `doc_index`.

## Bras (mêmes positions DEV)

Les logits P1 ne couvrent que le chemin MSB de la cible. Tous les bras factorisent donc la probabilité de l'octet en 8 décisions binaires, et le BPC vaut la somme sur d de −log2 σ(±ℓ_d).

- **A0 — booléen seul.** Les logits P1 tels quels, c'est-à-dire le BPC P1.
- **A0c — table booléenne de même taille.**
  - Ce bras reprend A0 et y ajoute une correction de logit par décision, lue dans une table de comptes TRAIN.
  - La table est indexée par un hachage fixe des entrées de A1, défini ci-dessous.
  - Sa taille en octets est égale à celle de la tête A1.
  - Le poids de la correction est choisi sur TRAIN-HOLD.
  - C'est le contrôle à mémoire égale, sans neuronal.
- **A1 — hybride option B.**
  - Pour chaque profondeur d ∈ 0..7, une `TernaryLayer` (`ternary_bitplane`) produit une correction `i32` δ_d, ajoutée à ℓ_d à l'échelle fixe 1/8 (en unités ln-odds × 256).
  - Ses entrées sont en activations 1 bit :
    - les 112 bits de c1..c8, W et confiance ;
    - les d bits de préfixe de la cible ;
    - le signe de ℓ_d ;
    - |ℓ_d| sur 4 seaux fixes (< 128, < 512, < 1024, ≥ 1024), en codage thermomètre.
  - Les poids maîtres sont en f32, uniquement à l'entraînement : perte logistique, une époque sur TRAIN hors TRAIN-HOLD, ordre déterministe.
  - L'export ternaire se fait au seuil τ ∈ {0,0625 ; 0,125 ; 0,25}, choisi sur TRAIN-HOLD.
  - L'inférence est entièrement entière.
- **W — témoin S5h** `--stream-50m-msb` : BPC f32 3,655 (q4 3,785), rapporté comme repère, pas comme critère.
- **H-ALT (facultatif)** : rapportée à côté si elle est branchée sur les mêmes traces, sans changer les critères.

Graines de la tête : 11 et 23. A0 et A0c sont déterministes.

## Critères (figés)

1. **Exactitude.**
   - Sur le Dell, les scores `i32` de A1 sont recalculés par `ternary_forward_oracle` sur toutes les positions DEV, avec 0 écart exigé.
   - Les poids exportés sont rejoués sur Thor sur des états synthétiques déterministes ; le SHA des sorties `i32` doit être identique entre Dell et Thor.
   - Le logarithme en f64 du BPC n'est pas soumis à l'identité binaire.
2. **Go.**
   - Pour les deux graines, il faut BPC(A1) < min(BPC(A0), BPC(A0c)) − 0,02.
   - Cet écart doit aussi dépasser deux fois l'écart entre les deux graines.
   - Sinon **no-go** : l'option B s'arrête à H-1. Le résultat négatif est publié tel quel.
3. **Coût.** Sur le Dell, on rapporte les µs par position et les octets résidents de A0, A0c et A1. Si le surcoût de A1 dépasse 1,5× A0, le go reste diagnostique : il est rapporté, mais H-2 n'est pas lancé sans nouvel accord.
4. **Rapport.** Tous les bras, les deux graines, le τ retenu, la taille de la tête et la fraction de poids nuls sont publiés, sans sélection a posteriori.

## Ce que H-1 ne démontre pas

H-1 ne démontre pas de routage MOR neuronal (H-2), ni de gain moteur natif (H-3), ni de gain énergétique (GB-7 : non mesuré). Il ne démontre pas non plus une généralisation au-delà du DEV S5h. Le DEV S5h est découpé par article, et non par auteur : une fuite entre TRAIN et DEV au niveau des auteurs reste possible et n'est pas mesurée.
