#!/usr/bin/env bash
# The security checks, run LOCALLY rather than in CI: CI only builds releases, so these are a
# script, and the fast ones also run from a pre-push hook. Fuzzing is scripts/fuzz.sh, run per
# step for its time budget.
#
#   bash scripts/check.sh          cargo-deny (licences, bans, sources, advisories),
#                                  cargo-audit, and gitleaks over this directory
#   bash scripts/check.sh --fast   the same without cargo-audit (the pre-push hook's set).
#                                  cargo-deny's advisories check reads the same RustSec
#                                  database, so --fast still refuses a known vulnerability.
#                                  What only the full run sees: an unmaintained or unsound
#                                  crate deep in the graph (deny.toml scopes those to our own
#                                  direct dependencies; cargo-audit --deny warnings does not),
#                                  so run the full check before a release
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
	# Run from a git hook, GIT_DIR names the repository being pushed, and git would then take this
	# directory for the top of ITS work tree and list the wrong files. Find the checkout from here.
	unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX
	if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
		git ls-files -co --exclude-standard
	else
		# Not a checkout (a copied tree): every file but build output.
		find . -type f -not -path './target/*' -not -path './.work/*' | sed 's|^\./||'
	fi
) | while IFS= read -r f; do
	# An if, not `[ ] && printf`: a loop's status is its last command's, so a last path that is
	# not a file (deleted, not yet staged) would end the script here, silently, under set -e.
	if [ -f "$here/../$f" ]; then printf '%s\n' "$f"; fi
done >"$here/../.work/publishable"

# snout-lepis's L2 gate: its code against pinned PgDog and Citus checkouts.
# Full run only: the first run fetches both trees.
if [ -z "$fast" ]; then
	echo "== lepis clone check"
	bash "$here/../lepis/scripts/clone-check.sh"
fi

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
		# Run from functions/ so cargo-audit reads functions/.cargo/audit.toml: the advisories
		# functions/deny.toml ignores, and the informational ones only cargo-audit reports, each
		# with its reason and review date there.
		(cd functions && cargo audit --db /cache/advisory/rustsec --deny warnings --file Cargo.lock)
	fi
	echo "== gitleaks"
	# --no-git: the directory as it will be mirrored, which is what a reader of the public
	# repository sees. The whole repository history is the repo-wide scan, scripts elsewhere.
	rm -rf /tmp/publishable && mkdir -p /tmp/publishable
	tar -cf - -T .work/publishable | tar -xf - -C /tmp/publishable
	gitleaks detect --no-git --source /tmp/publishable --redact --exit-code 1 --no-banner
'
