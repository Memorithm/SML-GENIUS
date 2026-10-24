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
- Partition de réglage : les derniers 1/16 des articles TRAIN, dans l'ordre du manifeste, forment `TRAIN-HOLD`. Ils servent uniquement aux choix ci-dessous et ne sont jamais vus par la descente de gradient.

## Traces attendues du run P1 (contrat minimal)

Pour chaque position prédite de TRAIN et de DEV, le run P1 exporte, dans l'ordre du manifeste :
- `split`, `document_id`, `position` ;
- l'octet cible ;
- l'état SBG lu par les banques, en bits empaquetés `u64` à largeur fixe `S` déclarée ;
- les 8 bits prédits par les banques P1 ;
- l'identité des poids P1 (SHA-256) et la source SML épinglée.

Le format exact est figé avec le fil GB-11 avant tout code de lecture. Un doublon `document_id + position`, ou un même `document_id` présent dans deux splits, rend le run invalide.

## Bras (mêmes positions DEV)

- **A0 — booléen seul.** Bits P1 convertis en distribution sur 256 octets par une table de confusion par bit, apprise sur TRAIN avec lissage de Laplace (α = 1). Sans cette calibration, des bits durs donneraient un BPC infini.
- **A0c — table booléenne de même taille.** Ce bras reprend A0, avec en plus une table de comptes sur TRAIN. Elle est indexée par un hachage fixe de l'état SBG et des bits P1, et sa taille en octets est égale à celle de la tête A1. Elle fusionne avec A0 par interpolation, avec un poids choisi sur TRAIN-HOLD. C'est le contrôle de mémoire égale, sans neuronal.
- **A1 — hybride option B.** Ce bras reprend A0, avec en plus une tête `TernaryLayer` (`ternary_bitplane`) qui lit l'état SBG et les 8 bits P1 (activations sur 1 bit) et produit 256 corrections de logits en `i32`, à l'échelle fixe 1/8.
  - Les poids maîtres sont en f32, uniquement à l'entraînement : entropie croisée, une époque sur TRAIN, ordre déterministe.
  - L'export ternaire se fait au seuil τ ∈ {0,0625 ; 0,125 ; 0,25}, choisi sur TRAIN-HOLD.
  - L'inférence est entièrement entière.
- **W — témoin S5h** `--stream-50m-msb` : BPC 3,66, rapporté tel quel. Il sert de repère, pas de critère.
- **H-ALT (facultatif)** : si le fil H-ALT (#676/#679) branche sa tête lecture seule sur les mêmes traces, elle est rapportée à côté, sans changer les critères.

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
