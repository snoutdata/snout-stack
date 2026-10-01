# snout-stack's image: the stack's own binary (init, setup, gateway, functions) over Deno's
# distroless image, because `snout-stack functions` bundles each function with `deno bundle`.
#
#   docker build -t snout-stack .                          # in this repository
#   docker build -f selfhost/Containerfile -t snout-stack .  # from the stack's workspace
#
# The context holds the lockfile. The binary is static (musl), so it needs
# nothing from the image under it; Deno is the same release the functions runtime is built on, so
# what it bundles is what the runtime runs. Root by default, because the functions step writes the
# manifest the runtime reads as its own root (0600: it holds the service key); compose runs the
# gateway and setup as an unprivileged user.
# The Deno release the functions runtime is built on (the platform's builder pin; a test holds
# the two together). Declared before the first FROM so the last one can use it.
ARG DENO_TAG=distroless-2.9.7

FROM docker.io/library/rust:1.98.1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --locked --release -p snout-stack \
	&& cp target/release/snout-stack /snout-stack

FROM docker.io/denoland/deno:${DENO_TAG}
COPY --from=build /snout-stack /usr/local/bin/snout-stack
ENV SNOUT_STACK_DENO=/bin/deno DENO_DIR=/tmp/deno
EXPOSE 8000
# How SnoutData Desktop's "Find databases" knows this container is part of the SnoutData stack
# (docs/desktop/DISCOVERY.md): by label, never by guessing from the image name.
LABEL com.snoutdata.stack="1" com.snoutdata.component="stack"
ENTRYPOINT ["/usr/local/bin/snout-stack"]
CMD ["gateway"]
