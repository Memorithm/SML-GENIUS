# External weight publication and activation admission

The SMLW v1 header and CRC32C remain unchanged. CRC32C detects accidental
corruption; it establishes neither publisher identity nor authorization.
Local development files passed directly to `WeightArtifactCatalog::new` remain
an explicitly trusted development interface. Do not use that interface for
external downloads or infer authenticity from `from_retained_bytes` alone.

`sml-publication` is a separate optional operator/harness tool. It is not a
dependency of `sml-core`, `sml-runtime`, or the self-sufficient model. It invokes
the existing SciRust-Verify authentication owner, then checks the signed record
against an independently provisioned publication ACL before exposing a cold
runtime staging method. It does not select a deployment, upload weights,
authorize scientific promotion, or merge code.

## Operator trust inputs

Configure the exact SciRust-Verify executable, allowed-signers file, and policy
outside the producer dossier. These paths and their filesystem permissions
belong to the trusted operator, never to a download or candidate submission.
The policy is strict JSON:

```json
{
  "schema_version": 1,
  "code_sha": "<exact lowercase 40-character Git commit SHA>",
  "publication_id": "<independently selected release identity>",
  "publishers": ["<allowed authenticated principal>"],
  "verifier_sha256": "<SHA-256 of the trusted owner executable>",
  "allowed_signers_sha256": "<SHA-256 of independently approved allowed-signers bytes>"
}
```

The expected code SHA and release identity must come from the operator's
qualified deployment selection. Copying them, publisher names, or trust pins
from the producer manifest defeats the boundary. Changing an ACL/key or release
requires the operator's separate authorization. The adapter snapshots the
approved public trust bytes into a private temporary directory; the owner
authenticates that snapshot. No signer private key is needed by admission.

## Producer record

Create a SciRust-Verify dossier whose required custom check
`weight-publication` emits this JSON as stdout. The owner seals it as
`logs/custom-weight-publication.out.log`, then an admitted publisher creates the
owner's detached attestation. The full signed bundle and envelope travel with
the publication. Weight bytes are below `weights/`:

```json
{
  "schema_version": 1,
  "repository": "Memorithm/SML-GENIUS",
  "code_sha": "<exact code SHA>",
  "publication_id": "<release identity>",
  "publisher": "<signed owner principal>",
  "weights": [{"path": "page0.smlw", "sha256": "<SHA-256 of complete SMLW bytes>"}]
}
```

Names, code, release, principal, bundle, record and complete weight bytes must
all match. Unknown/duplicate authority fields, duplicate artifact paths or page
identities, traversal, symlinks, unsupported versions, missing authentication,
and exceeded bounds fail closed. Admission is limited to 64 artifacts and
64 MiB total weight bytes; JSON/public trust inputs and owner output are capped
at 256 KiB. The pinned trusted owner is run with a cleared environment and a
30-second deadline; it is not a hostile-code sandbox.

```sh
cargo run --locked -p sml-publication -- DOSSIER RUN OWNER_BINARY POLICY ALLOWED_SIGNERS
```

The command outputs an admission receipt only. Consumers call
`admit_external_weights`, then `AdmittedWeights::stage` and the existing runtime
commit API. Staging uses the retained immutable admitted bytes, never reopens
download paths, and refuses an already populated runtime. The receipt explicitly
reports `scientific_qualification=not_evaluated` and `merge_authorization=false`.
Authenticating a publisher does not qualify learned weights, license rights,
model quality, or the truth of its provenance assertions. Keep the existing
scientific and final evaluation gates.

## Qualification and production deployment gate

The hosted development workflow builds SciRust-Verify at
`662a30dc774dd19c68a79bed3889036a91930123` and explicitly executes the owner
integration test that is otherwise ignored by ordinary workspace tests. It uses
an ephemeral development signer for negative controls and RAM byte continuity.

The separate `Production weight publication` workflow is the organizational
path. It runs only for integrated `main` code or an explicit trusted dispatch,
loads the private identity from the repository Actions secret
`SML_PRODUCTION_SIGNING_KEY`, and admits only the committed
`memorithm-sml-production` public key. The private key is written with mode
0600 under the hosted runner's temporary directory, deleted at step exit, and
never uploaded.

That workflow independently constructs the policy from the exact SML commit,
publication identity, trusted owner digest, and allowed-signers digest. It pins
the external SciRust Hub consumer at
`d6b679e696f5e1b1d3c004c7c95c36c0ea1584d2`, then requires the versioned
`scirust-hub.sml-weight-admission-receipt/v1` receipt with the exact
organizational principal and immutable code revision. The retained artifact
contains the signed dossier and external receipt, never the private identity.

A publisher authentication receipt still reports
`scientific_qualification=not_evaluated` and `merge_authorization=false`.
Scientific promotion and deployment remain separate decisions.
