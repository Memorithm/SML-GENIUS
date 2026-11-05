# DEC-6 — Boolean word codes from an open lexicon (pre-registration)

Frozen 2026-11-05, after DEC-5 (`docs/RESULTS-DEC-5-WORD-MEANING-CODES.md`,
`DEC5_SEEN_ONLY`: seen 0.828, new 0.525) and before any DEC-6 number on bench
data. The owner chose this direction on 2026-11-05 ("Lexique ouvert
(WordNet)").

## Question
DEC-5 context codes collapse opposites (beginning/end 2 bits apart out of 64)
and do not learn negation. If the word codes come instead from an open
lexicon, with synonyms sharing synsets and antonyms carrying each other's
synsets with opposite sign, does the Boolean head read the held-out wordings?

## Lexicon (pinned; built by RemoteOps #1260, stage 1)
- Open English WordNet **2025 edition** (`english-wordnet-2025.xml.gz`,
  CC-BY 4.0, https://github.com/globalwordnet/english-wordnet), SHA-256
  `9ca6d1dcb75f822fdd66617f7d9da48142ace38dd544d6ad5e2feca1674ad3fe`.
- Lexicon TSV (`ops/sml/dec6_lexicon/build_oewn_lexicon.py`): 64 889 single,
  lowercase, alphabetic lemmas; 6079 with antonyms. SHA-256
  `7f74b1d4d06bf00ba06ffc41cfd9f36038d64726e06d1df96671a2de4e179ac3`. It sits on
  Thor at `~/sml-corpora/oewn/2025-edition/lexicon.tsv`.
- The lexicon is independent of the bench. Stage 1 printed only lexicon
  statistics and probe rows, and no bench number.

## Model (`sml-decision::dec6_lexicon`, runner `examples/dec6_lexicon_heads.rs`)
- **Lexicon word code (64 bits):** V(w) = Σ r(synset) + ½ Σ r(hypernym) −
  Σ r(antonym synset), where r is a fixed ±1 sign vector from a hash of the
  synset id. Bit k is on iff V(w)[k] > 0. A word missing from the lexicon is
  looked up through the suffix rules `ies→y, es→, s→, ed→, ed→e, ing→, ing→e`,
  in that order, with the first hit winning.
- **Question code, features, bench, rows, wordings, heads, training, grid and
  DEV-only selection** are DEC-5's, unchanged. The DEC-5 context code is
  rebuilt from the TRAIN texts for the second arm.
- Arms:
  - **SML_BOOL_LEXICON** (primary): question seen only through its lexicon code;
  - SML_BOOL_LEXICON_CTX (descriptive): lexicon code plus DEC-5 context code;
  - integer heads (descriptive);
  - BLIND (control, must score 0.500).

## Criteria (SML_BOOL_LEXICON on TEST)
- `READS_SEEN`: accuracy on TEST `seen` ≥ 0.70.
- `READS_NEW`: accuracy on TEST `new` ≥ 0.60.
- **DEC6_GO**: both. **DEC6_SEEN_ONLY**: `READS_SEEN` only.
  **DEC6_NEW_ONLY**: `READS_NEW` only. **DEC6_NO_GO**: neither.
- **DEC6_INVALID** overrides all if BLIND is not exactly 0.500 on both sets.
- **NO_GO_DETERMINISM** overrides all if two processes give different
  `DEC6_CODES`/`DEC6_SCORE`/`DEC6_PRED` lines (timings excluded).
- Reported, not judged:
  - the other arms;
  - per-task and per-intent accuracy;
  - Hamming distances between probe words for both code types;
  - the codes of every wording.

## Run
Thor, on the pinned DEC-1 bench and the DEC-2 TSV export, with the pinned
lexicon (both SHA-256 values checked). Two processes, pinned to this SML
commit (RemoteOps PR).

## Known limits
- WordNet has no link from "not" to "false" or "wrong", so negation may still
  fail; the per-intent rows will show it. Irregular forms ("wrote") get no code.
- The lexicon is external knowledge (CC-BY 4.0, to be cited), not learned
  from SML text. The decision heads stay Boolean and learned.
