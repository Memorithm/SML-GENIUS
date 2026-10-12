# A6-I6 — Résultats : mémoire surveillée d'ordre quatre et échecs adversariaux hors périmètre

**12 octobre 2026. Première exécution native complète : les quatre régimes préenregistrés ont été mesurés et satisfont exactement aux prédictions, y compris les échecs obligatoires.**

Le [préenregistrement](A6-I6-BOUNDED-FOURTH-SUPPORT-PREREGISTRATION.md) a été commité **avant tout code et tout test A6-I6**, au SHA `d0d5d9cf07b6096b25eda7755204cb4ac7cf0390`, sur `main` fusionné à la PR #507. Premier essai complet [GitHub Actions A6-I6 run 38271664108 / job 114887135677](https://github.com/Memorithm/SML-GENIUS/actions/runs/38271664108/job/114887135677), commit source `f998b79ec6ea71e94b8c2c1b01f8cfcb75156403`, **SUCCESS** : formatage, compilation, Clippy strict, tests unitaires et 168 ajustements natifs avec 86 016 décisions TEST. Vérifier de nouveau la CI dédiée et générale après les commits documentaires avant de fusionner.

## 1. Problème et choix : une mémoire TARGETÉE, pas une garantie universelle

A6-I5/PR #507 a montré qu'une mémoire **exactement exhaustive des triplets** ne détecte pas une interaction cachée de degré 4. Pour k=1024, une couverture exhaustive de tous les quadruplets impliquerait **45 545 029 376 combinaisons de quatre positions** (sans compter les 16 motifs par tuple), et n'a pas été validée dans les budgets du programme.

La nouvelle primitive Rust publique `anf_budgeted_four_support::AnfBudgetedFourSupportGuard` surveille uniquement des quadruplets d'indices **explicitement déclarés avant l'apprentissage**. Une liste d'interactions surveillées est **un prior structurel externe** (par exemple une connaissance historique des sous-systèmes à risque), et non une récupération des vraies dépendances cachées. Le garde stocke pour chacun des tuples jusqu'à seize motifs d'entrée (1 bit par motif dans un `u16`), ne lit **aucun label TEST**, et accepte la prédiction d'un **vrai `PassiveCommittee`** seulement si trois conditions sont vérifiées : confiance sélective ≥0.90, meilleure erreur de banque sur TRAIN ≤1/8 et aucune configuration **surveillée** jamais vue.

Les erreurs de largeur, de tuple invalide/dupliqué, de dépassement des échantillons TRAIN (maximum512) ou des listes (maximum16), du scellement, et des historiques committee/garde désynchronisés échouent fermées. Le module ne change ni les coefficients ANF, ni le posterior du comité, ni le chemin PVP6/MOR/Delta-KV existant.

## 2. Population : 168 comités gelés et 86 016 TEST

**Sept fan-in** `k=8,16,32,64,128,256,1024`, trois graines, deux panels de labels TRAIN (propre / flips Bernoulli1/16 indépendants) et quatre régimes synthétiques = **168 politiques**. Chaque politique reçoit **512 échantillons PASSIFS** `(input,label)` et une liste **pré-déclarée de exactement un tuple** `W=[r4,r5,r6,r7]` issue des indices de rôles publics, permutés par graine. Ces positions surveillées sont connues du garde; les positions de la vraie fonction manquante ne lui sont pas données.

Le comité contient quatre vraies portes `WideAnfGate`, `h0=r0 XOR r1`, `h1=h0 XOR r0r1`, `h2=h0 XOR r2`, `h3=h0 XOR r3`, prior gelé `[4,2,2,1]/9`, likelihood de bruit1/16. Dans le régime WATCHED_INCLUDED seul, le vrai professeur est `h0` déjà présent. Dans les trois autres il est `h0` avec un monôme de degré quatre ou cinq supplémentaire, **absent de la banque**.

La censure est stricte : aucun TRAIN ne satisfait le terme caché supplémentaire; donc **aucun algorithme passif ne peut distinguer l'enseignant de `h0` depuis ces labels de TRAIN**, même en présence du même bruit. Chaque régime a **42 fits ×512 TEST =21 504 prédictions OOD**. Les TEST, 512 entrées par politique, sont générés **après gel des 168 politiques** ; la moitié des entrées activent le terme masqué et la moitié le désactivent. Les deux panels de bruit utilisent des entrées appariées; WATCHED_ABSENT/WATCHED_INCLUDED également, soit **63 flux d'inputs TEST uniques**, pas 168 tirages indépendants du monde réel.

Les rôles 0..7 sont permutés comme positions physiques et leur bit TRAIN explore systématiquement chaque motif de huit rôles admissible dans le domaine censuré. Sous WATCHED_* il manque exactement le motif **1111 de `W`** (15/16 observés); sous UNWATCHED_* les **16/16 motifs de W** sont observés, y compris 1111, tandis que l'interaction cachée est censurée ailleurs.

## 3. Les quatre résultats obligatoires, sans masquer les contre-exemples

| Régime / vérité | Comité natif : acceptations / erreurs | Garde : acceptations / erreurs | Contextes nouveaux observés dans W |
|---|---:|---:|---:|
| **WATCHED_ABSENT**, vrai `h0 XOR r4r5r6r7` absent de banque | 21 504 / **10 752** | **10 752 / 0** | 10 752 |
| **WATCHED_INCLUDED**, vrai `h0` déjà dans banque | 21 504 / 0 | **10 752 / 0**, plus **10 752 abstentions inutiles** | 10 752 |
| **UNWATCHED_ABSENT**, vrai `h0 XOR r0r1r2r3` absent de banque | 21 504 / **10 752** | **21 504 / 10 752 erreurs confiantes** | 0 |
| **FIFTH_UNWATCHED_ABSENT**, vrai `h0 XOR r0r1r2r3r4` absent de banque | 21 504 / **10 752** | **21 504 / 10 752 erreurs confiantes** | 0 |

Tous les 168 fits restent `bank_adequate=true` sur TRAIN (`0/168` alarmes de mauvaise adéquation), ce qui est attendu car les enseignants manquants ont été rendus **observationnellement indistinguables** pendant la formation des postérieurs. Le module ne revendique pas de prédiction correcte des erreurs qu'il ne voit pas.

**H1 bénéfice mesuré :** dès qu'un tuple surveillé a un motif inconnu, le modèle s'abstient dans les 10 752 contextes où l'enseignant absent diffère de la banque, sans erreur sur les 10 752 réponses restantes.

**H2 coût réel :** si l'enseignant était déjà dans la banque, cette même abstention fait perdre **10 752 réponses qui étaient correctes**. La nouveauté d'entrée ne signifie pas nécessairement que la fonction est nouvelle.

**H3 et H4, négatifs indispensables :** la vraie fonction ajoute respectivement un terme de degré 4 ou 5 **hors de la watchlist**. Les entrées TEST ont des valeurs entièrement connues sur W et aucune nouveauté n'est déclenchée; le garde annonce **21 504 réponses**, dont **10 752 erreurs confiantes** dans chacun des deux régimes. C'est la démonstration que la politique ne peut pas vérifier l'exhaustivité de la banque par une mémoire de coordonnées cibles. Ces échecs sont obligatoires pour considérer le protocole réussi.

La présence d'une vérité absente de la banque ne peut pas être inférée dans les régimes H3/H4 à partir des TRAIN censurés : les données autorisées ne contiennent pas d'information permettant de départager les générateurs. Une « intuition » capable de garantir simultanément toutes les fonctions serait une impossibilité informationnelle dans ce cadre.

## 4. Temps, stockage, budget et généralisation

À toutes les largeurs k8...k1024, le **payload logique de la mémoire du garde** pour la seule entrée `WatchedTuple { indices:[usize;4], seen:u16 }` mesure **40 octets** sur ce runner **x86_64** (déduit de `std::mem::size_of`), avant les en-têtes/capacités des `Vec` et de la structure porteuse. Ce coût **n'inclut pas** la mémoire des quatre `WideAnfGate`, du `PassiveCommittee`, de leurs signatures, ni les vecteurs d'input et le RNG. Aucune table de taille `C(k,4)` n'est calculée : la complexité du garde est **O(nombre de tuples surveillés)** par TRAIN/TEST, à la réserve près de la validation du vecteur d'input complet par les portes du comité.

En contrepartie, ce budget fixe détermine sa **zone aveugle**. Il ne faut pas attribuer aux 40 octets la capacité d'évaluer tous les quadruplets possibles, ni invoquer une borne universelle de risque. Dans les données actuelles les quatre régimes ont un `g` construit de façon transparente; le garde reçoit une watchlist pré-déclarée mais alignée avec la vérité dans le régime favorable. Les régimes négatifs utilisent des termes cachés hors watchlist et restent tous indétectés.

La majorité des coûts de bout en bout appartiennent à la collecte passive des 512 exemples, au `PassiveCommittee`, à la prédiction des quatre modèles, et à la construction de la vérité synthétique. Le temps de `seal` ne mesure que la fermeture d'un petit tableau déjà constitué et ne doit pas être présenté comme un débit end-to-end du modèle.

## 5. Intégrité scientifique / CI

Avant le premier run complet, deux familles de corrections ont été apportées : **39 modifications de formatage `rustfmt`** réparties sur les deux nouveaux fichiers Rust et `lib.rs`, puis **une seule correction Clippy de boucle de test** et le remplacement d'une assertion de test tautologique par une comparaison réelle avec une fonction ANF absente. Aucune de ces corrections ne change les quatre scénarios, le programme de censure, les graines, les seuils, les poids de banque ou la politique du garde. Les journaux des premières tentatives CI échouées restent accessibles.

**Premier run complet sur source `f998b79ec6ea71e94b8c2c1b01f8cfcb75156403` :** [Actions dédié `epistemic-four-watch`](https://github.com/Memorithm/SML-GENIUS/actions/runs/38271664108/job/114887135677), **SUCCESS**, y compris `cargo fmt --check`, `cargo check --locked -p sml-core --all-targets`, `cargo clippy -D warnings`, unit tests Rust du garde, unit tests du bench, binaire RELEASE et tests GitHub de census exact. 168 lignes `A6_I6_FROZEN`, 168 `A6_I6_TEST`, 4 `A6_I6_SUMMARY`, une ligne `A6_I6_COMPLETE` avec `all_preregistered_conditions_passed=true`. La CI générale et la CI dédiée doivent être **de nouveau vertes au SHA documentaire final**.

Aucune dépendance NVIDIA SDK, code `unsafe`, runner Memorithm privé, service permanent ou accès aux labels TEST au moment de la décision. GitHub-hosted Ubuntu24.04, Rust1.89 épinglé, checkout exact SHA sans credentials persistants et permissions read-only.

## 6. Décision et prochaine gate

H1–H4 **confirmés dans le cadre préenregistré**, y compris deux échecs de sécurité importants. Ce résultat apporte une **surveillance d'hypothèses de risque ciblées et scalable en largeur**, pas une intuition générale : les fonctions ANF absentes de la banque restent non identifiables sans expériences supplémentaires lorsque les variables responsables n'ont pas été enregistrées ou activées dans les observations.

La prochaine étape utile est un banc **d'interventions actives à budget explicite sous banque incomplète**, en mesurant non seulement la précision après intervention mais aussi **le coût de sélection des questions, le délai de feedback, les cas où aucune question autorisée ne peut discriminer les fonctions**, et la calibration après changement de contexte. Elle ne doit pas devenir une simple lecture du monôme caché déguisée en intuition.
