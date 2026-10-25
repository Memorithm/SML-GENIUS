# HYB H-1d-cost — re-mesure du coût de la tête H-1d avec un noyau fusionné : préenregistrement

**Gelé le 25 octobre 2026, avant toute mesure.** Le propriétaire a choisi, le 25 octobre 2026, l'option « Noyau fusionné, re-mesure du coût ».

Ce document fait suite à `RESULTS-HYB-H1D-S5H-BANK-HEAD.md` (verdict GO_DIAGNOSTIC_COST). H-1d a passé tous ses critères sauf le coût : 1,74× pour une limite de 1,5× (P1 0,870 µs, tête 0,647 µs par position). H-1d-cost ne change ni les têtes ni leurs sorties. Seul le temps est mesuré à nouveau.

## Ce qui change : le noyau, pas la tête

`FusedBankHead` (`hyb_h1d_bank_head.rs`) calcule les mêmes corrections que `BankHead`, mais :

- la partie du mot haut commune aux 8 décisions (registre de mot, confiance) et le préfixe de cible inversé sont calculés une fois par octet ;
- les thermomètres de banque viennent d'une table de 256 entrées, au lieu de 48 comparaisons par décision ;
- la division par 8 avec arrondi vers le bas est un décalage arithmétique.

Il n'y a ni `unsafe` ni option de compilation particulière : le binaire est construit comme pour H-1d. En local (Xeon 2,8 GHz, mesure indicative), le surcoût de la tête passe d'environ 620 ns à environ 190 ns par position.

## Ce qui est mesuré

- **Têtes.** Les deux têtes A1d publiées par H-1d (`head-a1d-seed11.tsv` SHA-256 `b77e3f13…`, `head-a1d-seed23.tsv` SHA-256 `3c6192e8…`). Le programme refuse toute autre tête.
- **Données.** La trace EVAL3 du run H-1d et son fichier compagnon, déjà sur le Dell. EVAL3 est relu, mais seulement pour vérifier l'identité :
  - sur chaque enregistrement et pour les deux têtes, les corrections du noyau fusionné doivent être égales à celles de `BankHead` (0 écart) ;
  - le BPC EVAL3 des deux têtes, recalculé avec le noyau fusionné, doit être celui publié par H-1d, à 9 décimales (2,435427109 et 2,442609158).
  
  Rien n'est choisi ni jugé sur EVAL3 : aucun paramètre ne dépend de cette lecture.
- **Temps.** La méthode est la même qu'en H-1d. Le temps tête est le temps de scoring A1d (graine 11) moins le temps du coût de décision commun (`a0_cost`), par position. L'ancien noyau est chronométré dans la même boucle, comme contrôle, et l'ordre des deux noyaux alterne d'un enregistrement à l'autre.
- **Temps P1.** C'est celui que P1 a mesuré pendant H-1d (`eval3_us_per_position` dans `eval3.json`, 0,8696 µs). Il n'est pas re-mesuré.

## Critère (figé)

- Ratio = (temps P1 + temps tête fusionnée) / temps P1.
- Si le ratio est inférieur ou égal à 1,5, le verdict de coût est `COST_PASS`, et le verdict H-1d devient **GO**. Sinon, le verdict de coût est `COST_FAIL`, et H-1d reste GO_DIAGNOSTIC_COST.
- Un écart d'identité arrête le run sans verdict.
- **Rapport (`cost.tsv`).** Tout est publié : le temps de l'ancien noyau re-mesuré (contrôle), le temps du noyau fusionné, le ratio, les deux BPC et les SHA-256 des têtes.
- **Lecture.** Le temps de contrôle de l'ancien noyau dit si la machine se comporte comme pendant H-1d (environ 0,65 µs). S'il s'en écarte fortement, le rapport le signale. Le verdict ne change pas pour autant.

## Ce que H-1d-cost ne démontre pas

- Ni un nouveau gain de BPC : ce sont les têtes et les chiffres de H-1d.
- Ni le routage MOR (H-2), qui demandera encore un préenregistrement et l'accord du propriétaire, ni un gain moteur natif, ni un gain d'énergie.
- Ni que le temps P1 serait le même en dehors du run H-1d : il n'est pas re-mesuré.

Research only. TEST reste fermé. NOT_PROMOTED.
