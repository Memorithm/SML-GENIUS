# H-ALT-2 — résidu neuronal ternaire non linéaire, voie indépendante S5h

**Préenregistrement** : 24 octobre 2026, **avant toute exécution H-ALT-2 sur le DEV S5h**. Branche `research/hyb-alt2-nonlinear-binary-residual-s5h`; aucune modification des modules Claude, Grok, PVP6, MOR ou de la mémoire SBG. L'objectif est de surpasser, si les données le permettent, une tête linéaire binaire à même budget, et non de réutiliser ses scores.

## Hypothèse concurrente

Le chemin HYB H-1 de Claude applique à chaque décision MSB une correction linéaire ternaire sur 124 caractéristiques. Le chemin H-ALT-2 garde une tête entière **en lecture seule**, mais teste si **des interactions logiques explicites** issues du contexte (AND, XOR) ajoutent une information conditionnelle que le résidu linéaire brut ne représente pas facilement.

Deux bases de caractéristiques, toutes deux de **124 bits**, avec la même banque de 8 sorties ternaires à 2 mots de poids plus et 2 mots de poids moins par sortie, plus biais : **288 octets de poids logiques** hors pointeurs, métadonnées, tables et allocations (ne pas appeler cela RSS du processus).

- **RAW** : 64 bits de contexte, 16 bits du registre de mot, 32 bits de confiance, 7 bits du préfixe vrai de la cible déjà prédit, 1 bit de signe du logit et 4 bits d'amplitude.
- **PAIR** : mêmes 64 bits de contexte et 16 bits de mot, mais remplace les 32 bits de confiance par **16 AND** des bits de contexte `(i, i+1)` pour i 0..15 et **16 XOR** des bits `(i, i+32)` pour i 0..15. Les 12 derniers bits restent identiques. Les choix de connexions sont gelés ; ce ne sont pas des connexions apprises.

La tête lit les traces GB-11 P1 (40 octets, `gb11_p1_stream::TraceRecord`), les mêmes logits de base et les mêmes cibles que H-1. Elle n'écrit jamais dans SBG ni les tables P1. Comparaisons indépendantes : A0 P1 figé, A0c table de compte booléenne **288 octets** de la référence H-1, A1-RAW, A1-PAIR.

## Entraînement et sélection

- Deux graines fixes `11,23`; masters `f32` distincts pour chaque graine×base, entraînés en une passe exclusivement sur `TRAIN-FIT`, sans accès DEV.
- `TRAIN-HOLD` = dernier 1/16 des documents présents dans la trace TRAIN, par index de document. Les mêmes documents ne peuvent servir au fit et au hold ; ce découpage reste un sous-ensemble de TRAIN, pas DEV.
- Gradient logistique natif (maîtres continus **TRAIN seulement**), erreur `sigmoid(baseline_ln_odds + residual) - target_bit`, pas de gradient dans P1. Taux fixe `1/128`.
- Quantification `w_i∈{-1,0,+1}` ; retenir par sortie au maximum les **12 coefficients de plus forte magnitude** dépassant le seuil τ. Départage des égalités par indice croissant. Biais arrondi en unités de la sortie quantifiée ; refus de toute valeur non finie.
- Sélection de `τ∈{0.125,0.25,0.5}` et `scale∈{16,32,64}` (unités stretch/poids) **sur TRAIN-HOLD seulement**, parmi RAW, PAIR, et contrôle exact `scale=0`. L'algorithme ne lit DEV que lorsque ce choix est irréversible.
- Tête exportée avec `TernaryRow::from_weights` H-ALT-0 ; scores `i32` = popcount des deux masques plus bias, avec oracle scalaire de référence distinct et addition de stretch contrôlée.
- Critère diagnostique DEV : pour **les deux graines**, réduction `BPC(A1) < min(BPC(A0),BPC(A0c)) - 0.02`, supérieure à deux fois l'écart inter-graines, et surcoût d'inférence ≤1.5× A0. Sinon **NO_GO**. Une amélioration de H-1 Claude sur le même DEV ne sera revendiquée que lorsque les deux rapports, versions de traces et budgets seront appariés.

## Sécurité expérimentale

1. Refuser les fichiers de trace tronqués, splits inversés, identifiants de documents dupliqués/mélangés, couples (doc_index,position) non triés, faible volume ou traces non conformes. Recalculer les coûts P1 à partir des deux traces et confronter au `trace.json`; ne pas recopier un score attendu inventé.
2. **Important** : la première campagne GB-11 P1 Dell à source `cd05f26` a retenu deux banques `search_q4`, or un amendement préenregistré impose une **réexécution corrigée** du contrat d'allocation des banques. Ne pas utiliser les anciennes traces comme preuve finale H-ALT-2. Garder le SHA complet des traces et du générateur corrigé.
3. Aucun accès à TEST. Le DEV est figé, non utilisé pour choisir les poids, la base, les seuils ou les hyperparamètres. Les données restent sur Dell, seuls métriques et checksums non sensibles sont publiés dans les logs privés.
4. Les données S5h restent des textes réels mais partagent éventuellement des auteurs à travers TRAIN et DEV ; aucune affirmation de généralisation OOD/language universelle.
5. Les expériences H-ALT-2 et Claude H-1 utilisent les mêmes logits P1 mais **des modèles de taille compacte**, et ne constituent pas un comparatif de modèles 25B, de GPU ni une preuve d'efficacité énergétique.

## Livrables

H-ALT-2a : noyau `hyb_alt_pairwise_residual`, test exact vs oracle, identité RAW et PAIR, zéro-signal, budget et erreurs. H-ALT-2b : binaire streaming fit / hold / DEV et contrôle de traces, rapport comparative A0/A0c/A1. H-ALT-2c : RemoteOps SHA-pinné sur Dell pour DEV S5h et replay sur Thor, uniquement après CI verte et contrat de provenance corrigé. Tout résultat absent est marqué **UNMEASURED**.
