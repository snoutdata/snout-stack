#!/usr/bin/env bash
# Everything that must pass before a commit here, in the order that fails fastest:
# formatting, the dependency checks, lints, then the tests.
#
#   bash scripts/test.sh             the whole lot
#   bash scripts/test.sh <filter>    only tests whose name contains <filter>
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ $# -gt 0 ]; then
	exec bash "$here/dev.sh" cargo test --workspace --locked -- "$1"
fi

bash "$here/check.sh"
exec bash "$here/dev.sh" bash -c '
	set -euo pipefail
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
	cargo test --workspace --locked
'
