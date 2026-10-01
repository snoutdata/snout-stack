#!/usr/bin/env bash
# The security checks X13 asks for, run LOCALLY rather than in CI.
#
# X13 first said "in CI". The GitHub Actions budget is why .github/workflows only builds
# releases (security-scan.yml lost its triggers on 2026-07-17 for the same reason), so these
# are a script, and the fast ones also run from scripts/hooks/pre-push on any push that
# touches packages/stack/. Fuzzing is scripts/fuzz.sh, run per step for its time budget.
#
#   bash scripts/check.sh          cargo-deny (licences, bans, sources, advisories),
#                                  cargo-audit, and gitleaks over this directory
#   bash scripts/check.sh --fast   the same without cargo-audit (the pre-push hook's set).
#                                  cargo-deny's advisories check reads the same RustSec
#                                  database, so --fast still refuses a known vulnerability
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

fast=""
[ "${1:-}" = "--fast" ] && fast=1

# What gitleaks scans: the files git would publish (tracked, and untracked but not ignored), listed
# here because the container sees this folder and not the repository's .git. A harness's generated
# keys and a fuzz corpus are gitignored and never mirrored, so they are not what a reader sees.
mkdir -p "$here/../.work"
(
	cd "$here/.."
	if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
		git ls-files -co --exclude-standard
	else
		# Not a checkout (a copied tree): every file but build output.
		find . -type f -not -path './target/*' -not -path './.work/*' | sed 's|^\./||'
	fi
) | while IFS= read -r f; do [ -f "$here/../$f" ] && printf '%s\n' "$f"; done >"$here/../.work/publishable"

exec bash "$here/dev.sh" bash -c '
	set -euo pipefail
	echo "== cargo deny"
	cargo deny --locked check licenses bans sources advisories
	# snout-functions is a workspace of its own (functions/Cargo.toml says why), with its own
	# lockfile and policy (cargo-deny reads functions/deny.toml, beside the manifest).
	echo "== cargo deny (functions)"
	cargo deny --manifest-path functions/Cargo.toml --locked check licenses bans sources advisories
	if [ -z "'"$fast"'" ]; then
		echo "== cargo audit"
		cargo audit --db /cache/advisory/rustsec --deny warnings
		echo "== cargo audit (functions)"
		# The same four advisories functions/deny.toml ignores, each with its reason there.
		cargo audit --db /cache/advisory/rustsec --deny warnings --file functions/Cargo.lock \
			--ignore RUSTSEC-2026-0285 --ignore RUSTSEC-2026-0118 --ignore RUSTSEC-2026-0119 --ignore RUSTSEC-2023-0071
	fi
	echo "== gitleaks"
	# --no-git: the directory as it will be mirrored, which is what a reader of the public
	# repository sees. The whole repository history is the repo-wide scan, scripts elsewhere.
	rm -rf /tmp/publishable && mkdir -p /tmp/publishable
	tar -cf - -T .work/publishable | tar -xf - -C /tmp/publishable
	gitleaks detect --no-git --source /tmp/publishable --redact --exit-code 1 --no-banner
'
