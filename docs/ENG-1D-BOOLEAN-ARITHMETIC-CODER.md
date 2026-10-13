# ENG-1d — codeur arithmétique booléen (BAC)

Statut : **outil de vérification**, aucune revendication de qualité de modèle.

## Principe

Chaque octet est codé comme 8 décisions binaires, du bit de poids fort au
bit de poids faible. La distribution 256 voies du modèle est décomposée
exactement par un arbre de sommes fixe (`ByteBitTree`) : le produit des 8
probabilités binaires redonne la probabilité de l'octet. La taille idéale
du flux est donc la perte logarithmique du modèle ; seule la quantification
de chaque décision s'y ajoute.

Codeur (`crates/sml-core/src/bool_coder.rs`, sans dépendance) :

- intervalle 32 bits, `low` 64 bits avec propagation de retenue octet par
  octet (disposition LZMA) ;
- probabilité de `1` en `u32` sur 2^32 ; point de coupure par produit exact
  64 bits `(range · p) >> 32`, sans perte due à un décalage préalable ;
- probabilités bornées à `[2^8, 2^32 − 2^8]` : avec `range ≥ 2^24` après
  normalisation, les deux branches restent non vides ; une décision coûte
  au plus 24 bits ;
- quantification déterministe : uniquement `+`, `/`, `*` IEEE-754 `f64` dans
  un ordre fixe, puis `round`. Encodeur et décodeur obtiennent les mêmes
  entiers à partir des mêmes sorties de modèle.

Pourquoi pas 128 ou 512 bits : la précision utile est bornée par le modèle
(`f64`, 53 bits) et le surcoût mesuré est déjà de quelques dizaines de bits
pour 64 Kio ; un état plus large ralentirait sans gain mesurable. Le
parallélisme utile est ailleurs : lanes entrelacées et évaluation booléenne
du modèle (étape ultérieure).

## Conteneur et garanties

En-tête : `SMLBAC01`, identité du modèle fournie par l'appelant, longueur,
SHA-256 du texte clair, puis le flux. Le décodage échoue (jamais de texte
faux) si l'identité du modèle diffère, si le flux est tronqué ou suivi
d'octets, ou si le SHA-256 du résultat ne correspond pas.

`sml-bool-codec` (binaire) pilote le codeur avec le modèle paginé :

```bash
cargo build --locked --release -p sml-core --bin sml-bool-codec
sml-bool-codec encode TRAIN WORK_DIR INPUT NEW_OUTPUT DEPTH POLICY
sml-bool-codec decode TRAIN WORK_DIR INPUT NEW_OUTPUT
```

`POLICY` : `wb`, `swb:<beta>` ou `dir:<tau>` (voir
`ENG-1D-ESCAPE-POLICY-REFERENCE.md`). L'identité enregistre le SHA-256 du
TRAIN, la profondeur et la politique ; le décodage exige le même TRAIN.
Le flux code le fichier en continu (historique = octets précédents), sans
remise à zéro par lane : son bit/octet n'est donc pas identique à celui
des bancs à 64 lanes.

## Mesure exploratoire (non probante)

*Opticks* de Newton (hors panel TESTTEST) : TRAIN = 60 % initiaux
(340 318 octets), entrée = 65 536 octets à partir de 80 %. Construction
release, conteneur Linux partagé, Rust stable.

| Profondeur / politique | bit/octet modèle | bit/octet flux | Flux (octets) | Décodage |
|---|---:|---:|---:|---|
| 4 / `wb` | 2,359769 | 2,354126 | 19 285 | identique, SHA-256 vérifié |
| 12 / `swb:16` | 2,089207 | 2,089355 | 17 116 | identique, SHA-256 vérifié |

L'écart flux − bits réellement appliqués est la vidange finale
(36 à 40 bits). Avec `wb`, le flux est même légèrement sous la perte du
modèle : la borne `2^-24` relève des probabilités que le modèle rend plus
petites, ce qui agit comme un lissage. Débit : 1,8 à 4,2 Kio/s, limité par
les lectures paginées du modèle, pas par le codeur.

Repère honnête : avec le même TRAIN fourni comme préfixe, le coût
conditionnel de `xz -9e` est 16 840 octets, `bzip2 -9` 15 530 et
`gzip -9` 20 700. Le modèle statique (comptes TRAIN figés, sans adaptation
au texte codé) reste donc derrière `bzip2` et `xz` ; c'est une limite du
modèle, que le codeur rend mesurable physiquement.
