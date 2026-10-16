# SML — noyau scientifique différentiable MOR + ANF + récurrence (preuve locale)

Au 16 octobre 2026, `sml-core::learned_mor_anf_recurrent` assemble trois mécanismes réellement entraînables en Rust : un mélange de deux experts dont le **routeur apprend**, des caractéristiques booléennes **XOR et AND** issues des monômes ANF, et une **mémoire récurrente scalaire** dont le gradient traverse la séquence complète (BPTT). Ce n'est pas encore la fusion avec le modèle paginé 50M, ni une preuve de 25 milliards de paramètres.

Chaque événement d'une séquence (longueur bornée à 4096) reçoit les deux bits sources et la cible binaire. Les caractéristiques `[biais, a, b, a XOR b, a AND b, h_t]` nourrissent deux experts logistiques; le routeur donne le mélange différentiable `p = gate*p0 + (1-gate)*p1`. L'état est `h_t = tanh(w0*h_(t-1) + w1*(a XOR b) + w2*(a AND b))`. La descente stochastique utilise la vraie dérivée de l'entropie croisée, la dérivée du routeur et la **propagation rétrograde de l'erreur au travers de l'état précédent**. Les gradients sont normalisés, limités par norme globale puis validés contre NaN/Inf.

Le système n'a que **21 paramètres** ; il sert à prouver le chaînage des dérivées avant de développer les adresses de pages par le routeur MOR. Il interdit explicitement de le présenter comme un réseau ANF massif. Les features sont des fonctions booléennes fixes et **les coefficients continus apprennent** ; aucune découverte de monômes de degré élevé n'est démontrée ici.

Gates automatisés :
- la perte sur une séquence XOR déterministe décroît de 70 % ou davantage avec les experts et le routeur entraînés ;
- les coefficients du routeur et de la transition récurrente changent réellement ;
- le gradient du coefficient de récurrence est validé par différences finies sur une courte séquence ;
- les séquences invalides et les hyperparamètres non finis sont rejetés.

**Suite :** brancher le routeur appris sur les pages du vrai atlas de 50M, utiliser de multiples monômes ANF sélectionnés et apprendre les transitions avec des troncations BPTT contrôlées ; ajouter un banc financier chronologique 2023, un banc dialogué français/anglais et des ablations routeur/fonctions/récurrence. Les PR #583 et #584 prouvent déjà la modification effective de poids paginés, mais sans dérivées récurrentes ni MOR différentiable. Cette PR établit seulement le noyau algébrique et les tests qui manquaient pour assembler les deux.

Droits sources G0 toujours non admis pour le corpus SEC ; aucun texte tiers ni données TEST n'est introduit par cette PR.
