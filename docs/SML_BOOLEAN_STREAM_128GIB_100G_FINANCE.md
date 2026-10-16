# SML Boolean stream — capacité 128 Gio pour un corpus financier de 100 Gio

Date: 2026-10-16. Le modèle demeure la banque booléenne de 8 192 compteurs conditionnels, **pas** SML 25B, ANF récurrent ni modèle de prévision boursière homologué.

Cette évolution relève la limite explicite du mode `--stream` de **32 Gio à 128 Gio (137 438 953 472 octets de texte utiles)**. La cible de 100 Gio de texte tient dans cette enveloppe : **107 374 182 400 octets**. Les précontrôles existants vérifient `u64` et les débordements, le SHA-256 global, chaque document, le manifeste strict de huit colonnes, les séparateurs documentaires, le plafond de 8 Mio par document et de 20 millions de pièces et la cohérence des checkpoints atomiques.

La mémoire de lecture reste bornée à **1 Mio plus deux octets de contexte**, et non à la taille du corpus. Le calcul est une passe séquentielle; il n'implique pas de garder 128 Gio en mémoire.

Invariants conservés: refus des symlinks, classe déclarée de droits, SHA source fourni par l'appelant, reprise interdite si SHA source/manifeste a changé, compteur complet de positions et poids identiques avec les mêmes octets, DEV distinct, TEST non consulté. Les modes historiques des dépôts Memorithm conservent les anciennes limites locales de 256 Mio et 2 Mio/fichier.

Tests: porter le test de capacité de 25 à **100 Gio** sans allouer un tel fichier, laisser tourner la CI Rust du dépôt, et conserver les tests de frontières de chunks, de checkpoints et de droits.

**Statut d'évaluation :** le run réel 10 Gio sur Dell [RemoteOps #38927896689](https://github.com/Memorithm/RemoteOps/actions/runs/38927896689) est vert : 10 737 765 087 octets utiles, 35 022 documents et SHA source `c890b0cd08e9d67d4ea4ca9a88e760282c6de6c1a8f74ebfad6c0d1a2c740ed9`. La nouvelle capacité de 128 Gio est une extension validable par compilation/tests, **pas** une exécution de 100 Gio. Les données physiques admissibles, l'espace disque et les sorties mesurées devront être prouvés avant toute annonce de fin de formation.

Origine visée pour la campagne suivante: `TeraflopAI/SEC-EDGAR` sur Hugging Face, 295 Go de Parquet compressé affichés (10-Q 132 Go, 10-K 91,8 Go). L'étiquette du dataset Apache-2.0 n'homologue pas automatiquement le droit d'auteur des fichiers d'entreprises. Les documents crypto (dépôts SEC de crypto-sociétés, Bitcoin/Ethereum) et les séries de transactions publiques de blockchain doivent être identifiés séparément; les historiques de transactions Binance Vision sont restreints au non-commercial selon les termes du fournisseur publiés en août 2026 et ne seront pas incorporés au corpus homologable sans autorisation.

Jamais de confusion entre **100 Go de fichiers Parquet, 100 Gio de texte UTF-8 dédupliqué et 100 milliards de paramètres**.
