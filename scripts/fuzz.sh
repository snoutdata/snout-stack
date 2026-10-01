#!/usr/bin/env bash
# Fuzz every parser of untrusted input (X13), in a container with a pinned nightly (cargo-fuzz
# needs nightly for libFuzzer's sanitizer flags; everything else uses the stable pin).
#
#   bash scripts/fuzz.sh [seconds per target, default 60] [target ...]
#
# Targets live in fuzz/fuzz_targets/, added by the component step that writes the parser
# (the data API's filter grammar, Realtime's frames, storage's multipart and TUS, auth's tokens and
# SAML). A crash leaves its input in fuzz/artifacts/<target>/ and fails the script.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
engine="${STACK_ENGINE:-docker}"
seconds="${1:-60}"
shift || true
targets=("$@")

if [ ${#targets[@]} -eq 0 ]; then
	if [ -d "$root/fuzz/fuzz_targets" ]; then
		for f in "$root"/fuzz/fuzz_targets/*.rs; do
			[ -e "$f" ] && targets+=("$(basename "$f" .rs)")
		done
	fi
	if [ ${#targets[@]} -eq 0 ]; then
		echo "no fuzz targets yet: no component has a parser of untrusted input. Nothing to run."
		exit 0
	fi
fi

hash_of() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
bash "$here/dev.sh" true # builds the dev image when its Containerfile changed
dev="snout-stack-dev:$(hash_of "$root/container/Containerfile" | cut -c1-12)"
image="snout-stack-fuzz:$(cat "$root/container/Containerfile" "$root/container/fuzz.Containerfile" | hash_of | cut -c1-12)"
if ! "$engine" image inspect "$image" >/dev/null 2>&1; then
	"$engine" build --build-arg DEV_IMAGE="$dev" -t "$image" -f "$root/container/fuzz.Containerfile" "$root/container" >&2
fi
src="$root"
command -v cygpath >/dev/null 2>&1 && src="$(cygpath -w "$root")"

MSYS_NO_PATHCONV=1 "$engine" run --rm -v "$src:/work" -v snout-stack-fuzz-target:/cache/fuzz-target \
	-v snout-stack-registry:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/cache/fuzz-target \
	-w /work/fuzz "$image" bash -c '
	set -euo pipefail
	seconds="$1"; shift
	cargo +"$STACK_NIGHTLY" fuzz build -O >&2
	bin="/cache/fuzz-target/$(rustc +"$STACK_NIGHTLY" -vV | sed -n "s/^host: //p")/release"
	for t in "$@"; do
		mkdir -p "corpus/$t" "artifacts/$t"
		"$bin/$t" "corpus/$t" -max_total_time="$seconds" -artifact_prefix="artifacts/$t/" \
			-print_final_stats=1 >"artifacts/$t.log" 2>&1 &
	done
	failed=0
	for t in "$@"; do wait -n || failed=1; done
	for t in "$@"; do
		if [ -n "$(find "artifacts/$t" -type f \( -name "crash-*" -o -name "oom-*" -o -name "timeout-*" -o -name "leak-*" \))" ]; then
			echo "FAIL $t: $(ls "artifacts/$t")"; failed=1
		else
			echo "ok   $t: no crash in $seconds s"
		fi
	done
	exit $failed
' fuzz "$seconds" "${targets[@]}"
