# ENG-1e — exécution du panel enregistré

Le protocole et toutes les empreintes ont été publiés au commit
`4c89cdd81399b769e8ad08ffad2d8b6df63748cd`, PR 553, avant tout fit réel.
Le programme de confirmation doit ensuite être publié et qualifié avant
de mesurer les nouvelles œuvres. Les fixtures synthétiques ne constituent
pas une ouverture des TEST du roster.

## Préparation déjà effectuée, sans modèle

18 sources téléchargées depuis les URLs uniques du roster. Fingerprinter
Rust qualifié par ses tests SHA256 et de normalisation/header ; recalcul
indépendant des 18 sources/corps et 90 hashes de splits concordant.
Manifeste SHA256 :
`f8c19a79296b5191a881d5b26c4ba076b1c13f9d8e3cded4924d17ce41e22df0`.
Roster SHA256 :
`5ed558197ce8ce414d8ea298952d527c5b3a0c7e2942fc93f566b8bf0b2498d0`.
Le CLI contient ces hashes : modifier les données sans nouveau protocole
ne permet pas de poursuivre cette campagne. Le raw/body SHA est recontrôlé
avant chaque seal et chaque test, ainsi que les cinq empreintes de splits.

Toutes les tailles sont en octets/Kio binaires. Les trois TRAIN ont des
lanes de 2048, 4096 et 8192 octets ; DEV 512, TEST 1024. Les préfixes de
texte sont emboîtés mais les frontières des 64 lanes TRAIN changent avec
leur longueur : les ensembles de contextes ne sont pas strictement emboîtés.
Le contraste de volume porte sur ce pipeline fixé, frontières comprises.
DEV/TEST et leurs frontières restent exactement identiques entre volumes.

## Commandes et ordre obligatoire

```bash
cargo build --locked --release -p sml-core --bin sml-eng1e-confirm
bash scripts/eng1e/run-confirm.sh /tmp/eng1e-nouveau ALL
```

`ALL` est séquentiel. Pour la campagne parallèle, deux commandes du même
script sur deux répertoires neufs, FR et EN, exécutent chacun neuf auteurs.
L'argument optionnel final fournit un cache de raws déjà téléchargés ; les
hashes restent obligatoires. Ne jamais réutiliser un répertoire de résultats
ou écraser un sceau. La CI retélécharge les sources au lieu d'utiliser ce cache.

Pour chaque auteur : trois seal 128/256/512 Kio, puis trois test consommant
les sceaux. `test` vérifie les trois sceaux DEV, même quand il n'évalue qu'un
volume. Il réouvre exactement le store de son TRAIN, compare les hashes
store/index, et utilise exclusivement les indices classique/candidat du
sceau. Il ne calcule aucun argmin TEST. Après les trois scores, les stores
et le raw privé sont supprimés ; seules les preuves compactes restent.

Les sceaux ont 25 lignes `dev_bits`, toutes conservées. Les fichiers TEST
ont trois totaux : classique DEV-sélectionné, candidat DEV-sélectionné et
ancienne porte. Leur denominator est 65472 ; le sceau a 32704 cibles DEV.
Les logs ne publient que la progression et l'intégrité avant agrégation.

Réunir les deux répertoires `evidence` en vérifiant l'absence de collision,
puis `sml-eng1e-confirm analyze EVIDENCE_DIR NEW_REPORT`. Le programme exige
exactement 54 sceaux et 54 TEST, les IDs/volumes prévus, même binaire de
mesure et source/protocole/manifeste figés ; une archive incomplète est une
erreur, pas un verdict scientifique. L'analyse peut être refaite avec le
même code scientifique sur les archives d'un autre build, mais les
enregistrements d'une campagne doivent tous avoir le même hash de binaire.

## Lecture du rapport

Premières trois lignes : ALL/FR/EN sur 18/9/9 auteurs, moyenne de leurs
trois volumes ; intervalle bootstrap par strates et p-values exactes décalées
de δ=0.05. Ensuite six genres/langues, six volumes/langues, 18 auteurs et
54 cas, incluant choix DEV, borne β et ancienne porte. La ligne DECISION
restitue les conditions enregistrées, sans jugement sur les losses DEV.
Les trois répétitions de TEST ne sont jamais comptées comme trois auteurs.

Un `HISTORICAL_FR_EN_PANEL_CONFIRMED` porte uniquement sur cette comparaison
et ce panel de convenance. `SUPERIORITY_NOT_ESTABLISHED` n'affirme pas
l'absence de petit gain ; les raisons et bornes supérieures permettent de
distinguer régression, cible pratique non atteinte et manque de précision.
Les données corpus et modèles appris ne sont pas joints aux preuves Git.
La réplication CI n'est pas un second panel indépendant.
