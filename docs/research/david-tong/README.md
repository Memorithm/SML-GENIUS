# David Tong → Memorithm : registre de sources et exercices dérivés (2026-11-19)

## Périmètre vérifié
Le catalogue [David Tong — Teaching](https://davidtong.org/teaching/) contient **23 cours** (et non 21). Le registre [sources.json](./sources.json) enregistre une URL de cours par entrée, le titre, la priorité, les concepts et les destinations de recherche. Trois liens PDF contrôlés sont enregistrés séparément. Les sommaires détaillés des cours ont été inspectés; cela ne constitue **pas** une lecture ni une validation intégrale de milliers de pages.

## Priorités d'exploitation

| Priorité | Cours | Usage hypothétique pour Memorithm |
|---|---|---|
| P1 | Statistical Physics (§5.2 Ising), Statistical Field Theory (§1 spins, §2 corrélations, §3 RG) | Apprentissage d'énergie, activations, corrélations, coarse-graining dans le cerveau booléen |
| P1 | Mathematical Biology (chap. 2–4) | Dynamique de systèmes, seuils, pattern formation, réaction-diffusion |
| P1 | Kinetic Theory (chap. 3–4) | Langevin, bruit, Fokker-Planck, réponse à perturbations |
| P1 | Classical Dynamics (chap. 2 et 4) | Actions, invariants, stabilité et trajectoires |
| P1 | Vector Calculus (chap. 3–6) | Opérateurs de champs, conservation, calcul tensoriel |
| P2 | General Relativity (§1–3), Fluid Mechanics (§1–5) | Géométrie et transport, stabilité de propagation; analogie **non** validée sur temps d'exécution |
| P2 | Gauge Theory, Quantum Hall, QFT, Solid State | Invariances, réseaux discrets, topologie; analyse de faisabilité seulement |
| P3 | Cosmology, Particle Physics, Standard Model, SUSY, String Theory, Solitons | Raisonnement scientifique transversal; pas de bénéfice algorithmique établi |

Les sujets sont bien présents dans les programmes de cours; les bénéfices IA sont **des hypothèses Memorithm**. Aucune accélération, non-infériorité ni amélioration de perplexité n'a été mesurée.

## Piste quaternaire et logique booléenne
1. État quaternaire logique représenté par deux spins signés : `q=(sigma,tau)`, chacun dans `{-1,+1}`.
2. Tester l'énergie à 2 et 4 spins d'Ashkin–Teller, avec une référence 4-state Potts seulement au point paramétrique approprié; consulter [Ashkin–Teller/Potts neural network, DOI:10.1103/PhysRevE.64.067102](https://doi.org/10.1103/PhysRevE.64.067102) et [Phase Diagram of Ashkin–Teller, DOI:10.1007/s00220-023-04925-0](https://doi.org/10.1007/s00220-023-04925-0).
3. Ajouter séparément un **masque booléen** sur une interaction plutôt que mélanger masque de routage et état de spin : c'est une nouvelle hypothèse Memorithm, **pas** une affirmation attribuable à Tong.
4. Évaluer une référence Ising binaire, la version quaternaire et la version à masque sur mêmes données, budgets et graines; conserver l'oracle exact sur petits graphes.
5. Ne pas invoquer une supériorité d'attention ni de routage avant une évaluation au niveau du modèle complet et du coût réel.

## Usage pour l'entraînement : lignes rouges
- [Exercices originaux JSONL](../../../datasets/david-tong-derived-v0.jsonl) : 16 cas nouvellement rédigés, questions/réponses et vérification mathématique, **candidats**, pas poids entraînés ni jeu validé.
- Les références externes sont conservées par identifiant dans les exemples, traçables à `sources.json`. Ne pas conserver les cours entiers, figures, transcription verbatim ou exercices de Tong dans le dépôt.
- **Droits non établis pour l'entraînement intégral.** Une [ancienne page de Tong (Statistical Field Theory)](https://www.damtp.cam.ac.uk/user/tong/sft/) autorise une redistribution attribuée, non modifiée et non lucrative, mais ne constitue pas une licence explicite pour transformer et entraîner les PDF. Ne pas lancer d'aspiration ni de training sur le matériau original avant clarification documentée des droits, de l'accès licite et du mode d'exploitation.
- Avant usage SFT, valider chaque réponse mathématiquement, dédupliquer au niveau du concept et des variantes, isoler un jeu d'évaluation entièrement tenu à l'écart, documenter modèle/tokenizer/seed, mesurer les erreurs de fidélité et vérifier l'absence de fuite. Les 16 cas ne suffisent pas pour tirer de conclusion sur l'apprentissage.
- Le moteur de SML-GENIUS ne doit pas dépendre d'un service externe résident. **RemoteOps est l'unique plan de contrôle distant** pour une éventuelle campagne de formation.
- Respecter les prérequis du dépôt SML-GENIUS (`AGENTS.md`, feuille de route hors main, tests et preuves exact-head avant fusion).

## Localisateurs prioritaires et sources
- [David Tong, *Statistical Field Theory* (notes 2017), chap. 1–3](https://davidtong.org/pdfs/teaching/statistical-field-theory/sft.pdf)
- [David Tong, *Statistical Physics*, chap. 5](https://davidtong.org/pdfs/teaching/statistical-physics/statphys.pdf)
- [David Tong, *Mathematical Biology*, chap. 2–4](https://davidtong.org/pdfs/teaching/mathematical-biology/mathbio.pdf)
- [David Tong, *Kinetic Theory*, chap. 3–4](https://davidtong.org/teaching/kinetic-theory/)
- [David Tong, *Classical Dynamics*, chap. 2 et 4](https://davidtong.org/teaching/classical-dynamics/)
- [David Tong, *General Relativity*, chap. 1–3](https://davidtong.org/teaching/general-relativity/)
- [David Tong, *Vector Calculus*, chap. 3–6](https://davidtong.org/teaching/vector-calculus/)

### Statut de cette livraison
**Catalogue inspecté :** 23/23 pages de cours. **PDF examinés (sommaires et extraits ciblés) :** 2/23 cours, plus le sommaire du PDF de Mathematical Biology. **Exercices créés :** 16 brouillons originaux. **Entraînement effectué :** aucun. **Droits d'entraînement sur PDF originaux :** non vérifiés.
