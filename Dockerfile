# syntax=docker/dockerfile:1

# Kubri in the browser: kubri-server with the UI, plus helm (for rolling back and uninstalling
# releases) and kubectl (which most k9s plugins call). See docs/WEB.md for how to run it.
#
#   docker build -t kubri .
#
# Builds for other architectures cross-compile, so `--platform linux/arm64` on an amd64
# machine (and the other way round) does not run the compiler under emulation.

ARG KUBECTL_VERSION=v1.37.2
ARG HELM_VERSION=v4.3.1

FROM --platform=$BUILDPLATFORM node:22-trixie-slim AS ui
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0
RUN corepack enable
WORKDIR /src/ui
COPY ui/package.json ui/pnpm-lock.yaml ui/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY ui/ ./
RUN pnpm build

FROM --platform=$BUILDPLATFORM rust:1-trixie AS server
ARG TARGETARCH
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) target=x86_64-unknown-linux-gnu; gcc=gcc-x86-64-linux-gnu ;; \
      arm64) target=aarch64-unknown-linux-gnu; gcc=gcc-aarch64-linux-gnu ;; \
      *) echo "Kubri's image is built for amd64 and arm64, not $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    if [ "$(dpkg --print-architecture)" != "$TARGETARCH" ]; then \
      apt-get update; \
      apt-get install -y --no-install-recommends "$gcc" "libc6-dev-$TARGETARCH-cross"; \
      rm -rf /var/lib/apt/lists/*; \
    fi; \
    rustup target add "$target"; \
    echo "$target" > /target
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    CARGO_PROFILE_RELEASE_STRIP=true
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
# A workspace member; only its manifest and sources are needed, it is not built.
COPY ui/src-tauri ui/src-tauri
# Both architectures build at the same time and share these caches; `locked` keeps their cargo
# runs from unpacking the same crates at once ("failed to unpack package ... File exists").
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --locked -p kubri-server --target "$(cat /target)" \
    && cp "target/$(cat /target)/release/kubri-server" /kubri-server

FROM --platform=$BUILDPLATFORM debian:trixie-slim AS tools
ARG TARGETARCH
ARG KUBECTL_VERSION
ARG HELM_VERSION
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /tools
RUN set -eu; \
    base="https://dl.k8s.io/release/$KUBECTL_VERSION/bin/linux/$TARGETARCH"; \
    curl -fsSLO "$base/kubectl"; \
    echo "$(curl -fsSL "$base/kubectl.sha256")  kubectl" | sha256sum -c -; \
    chmod +x kubectl
RUN set -eu; \
    archive="helm-$HELM_VERSION-linux-$TARGETARCH.tar.gz"; \
    curl -fsSLO "https://get.helm.sh/$archive"; \
    curl -fsSL "https://get.helm.sh/$archive.sha256sum" | sha256sum -c -; \
    tar -xzf "$archive" --strip-components=1 "linux-$TARGETARCH/helm"; \
    rm "$archive"

FROM debian:trixie-slim
LABEL org.opencontainers.image.title="Kubri" \
      org.opencontainers.image.description="A fast, simple and good-looking Kubernetes GUI, in the browser" \
      org.opencontainers.image.source="https://github.com/snaketech-tu/kubri" \
      org.opencontainers.image.licenses="Apache-2.0"
# tini reaps the processes plugins leave behind. The home directory belongs to the root
# group too, so the image also runs as another user (`docker run --user`, OpenShift).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --gid 0 --create-home --home-dir /home/kubri kubri \
    && chmod g=u /home/kubri
COPY --from=tools /tools/kubectl /tools/helm /usr/local/bin/
COPY --from=server /kubri-server /usr/local/bin/kubri-server
COPY --from=ui /src/ui/build /usr/share/kubri/ui
# HOME too, for users without an entry in /etc/passwd.
ENV KUBRI_ADDR=0.0.0.0:7878 \
    KUBRI_UI_DIR=/usr/share/kubri/ui \
    HOME=/home/kubri
USER 10001
WORKDIR /home/kubri
EXPOSE 7878
ENTRYPOINT ["/usr/bin/tini", "--", "kubri-server"]
