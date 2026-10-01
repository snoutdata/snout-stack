# Security

Report a vulnerability privately, never as a public issue:

- a [private security advisory](../../security/advisories/new) on this repository, or
- email **security@snoutdata.com**.

The disclosure policy is [snoutdata/.github SECURITY.md](https://github.com/snoutdata/.github/blob/main/SECURITY.md)
(also linked from [snoutdata.com/.well-known/security.txt](https://snoutdata.com/.well-known/security.txt)).

Only the latest release is supported. This component runs in SnoutData Cloud, so a fix ships to
the hosted service first and to a release here in the same change.

Every release is checked with `cargo deny` and `cargo audit` (dependencies, licences, advisories),
gitleaks, and fuzzing of every parser that takes untrusted input (`scripts/check.sh`,
`scripts/fuzz.sh`). There are no example or default secrets anywhere in this code: a server that
is not given its secrets refuses to start.
