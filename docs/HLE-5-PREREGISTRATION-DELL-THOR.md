# HLE-5 — Préregistrement campagne Dell T430 / Thor

Figé avant tout lancement distant. Source SML épinglée par SHA dans le workflow
RemoteOps `sml-hle5-dell-thor.yml` ; aucune retouche après lecture des résultats.

- **Commande** : `bash scripts/hle5-bench.sh`, valeurs par défaut (TRAIN 1 Mo,
  3 graines 7/1007/2007, CAP 1024 slots = 7 168 o, fenêtre 1024, 4 têtes × 12 bits,
  24 itérations), corpus = `docs/*.md` du SHA épinglé ; découpage par hash de nom.
- **DEV S5h non utilisé** (ni lu ni réglé).
- **Hypothèses** (TEST, moyenne 3 graines) :
  - H1 : M4 < M3 − 0.01 (porte apprise > always à capacité égale).
  - H2 : G2 < G1 − 0.01 et G2 < B1.
  - H3 : R0 < B1 − 0.01 sur les deux architectures.
- **Règle de décision** : une hypothèse n'est retenue que si elle tient sur
  Dell (x86_64) **et** Thor (aarch64) avec un écart > écart inter-graines.
  Les résultats locaux prédisent H1 et H2 rejetées, H3 rejetée ou marginale.
- **Rapport obligatoire** : BPC, ns/octet, octets d'état, temps d'entraînement
  + recherche, nombre de paramètres de routage, empreinte (sha256) des journaux.
- Les résultats négatifs sont publiés tels quels.
