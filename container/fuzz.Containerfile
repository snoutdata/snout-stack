# The fuzzing environment: the dev image plus a pinned NIGHTLY toolchain and cargo-fuzz, because
# libFuzzer's sanitizer flags are nightly-only. The nightly is used for `cargo fuzz` and nothing
# else. The same pins as snoutdata/snouttime's.
ARG DEV_IMAGE=snout-stack-dev
FROM ${DEV_IMAGE}
ARG NIGHTLY=nightly-2026-09-15
ARG CARGO_FUZZ_VERSION=0.13.2
RUN rustup toolchain install "${NIGHTLY}" --profile minimal \
	&& cargo +"${NIGHTLY}" install --locked cargo-fuzz --version "${CARGO_FUZZ_VERSION}"
ENV STACK_NIGHTLY=${NIGHTLY}
RUN mkdir -p /cache/fuzz-target
