# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e

ARG RUST_BASE=rust:1.95.0-bookworm@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb
ARG BIFROST_BASE=rust:1.96.0-bookworm@sha256:c993d32d95cc146bd12c84d66f0b924a6a96f3988325f39c144f2f9893dea120
ARG GO_BASE=golang:1.26.0-bookworm@sha256:4f7e5f23bfacf4c2934ba70c132532742b6a53f01a4209e2c2eb7bd06c16f0bc
ARG RUNTIME_BASE=debian:bookworm-slim@sha256:63a496b5d3b99214b39f5ed70eb71a61e590a77979c79cbee4faf991f8c0783e

FROM ${RUST_BASE} AS usagebench-builder
WORKDIR /src/usagebench
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY schema ./schema
RUN cargo build --locked --release --bin usagebench

FROM ${BIFROST_BASE} AS bifrost-builder
ARG BIFROST_REVISION
WORKDIR /src
RUN test -n "${BIFROST_REVISION}" \
    && git init bifrost \
    && git -C bifrost remote add origin https://github.com/BrokkAi/bifrost.git \
    && git -C bifrost fetch --depth 1 origin "${BIFROST_REVISION}" \
    && git -C bifrost checkout --detach FETCH_HEAD \
    && test "$(git -C bifrost rev-parse HEAD)" = "${BIFROST_REVISION}"
WORKDIR /src/bifrost
RUN cargo build --locked --release --bin bifrost

FROM ${GO_BASE} AS gopls-builder
ARG GOPLS_VERSION
ARG GOPLS_MODULE_CHECKSUM
ENV GOTOOLCHAIN=local
RUN test -n "${GOPLS_VERSION}" \
    && test -n "${GOPLS_MODULE_CHECKSUM}" \
    && actual="$(go mod download -json "golang.org/x/tools/gopls@v${GOPLS_VERSION}" \
      | sed -n 's/^[[:space:]]*"Sum": "\([^"]*\)",/\1/p')" \
    && test "${actual}" = "${GOPLS_MODULE_CHECKSUM}"
RUN mkdir -p /out \
    && CGO_ENABLED=0 GOBIN=/out go install "golang.org/x/tools/gopls@v${GOPLS_VERSION}"

FROM ${RUNTIME_BASE} AS runtime-base
COPY --from=usagebench-builder /src/usagebench/target/release/usagebench /usr/local/bin/usagebench
ENV HOME=/tmp \
    RUST_BACKTRACE=1
WORKDIR /work
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/usagebench"]

FROM runtime-base AS bifrost
ARG USAGEBENCH_RELEASE
ARG ENVIRONMENT_VERSION
ARG DEFINITION_DIGEST
ARG BIFROST_REVISION
COPY --from=bifrost-builder /src/bifrost/target/release/bifrost /usr/local/bin/bifrost
LABEL org.opencontainers.image.title="UsageBench Bifrost reference environment" \
      org.opencontainers.image.source="https://github.com/BrokkAi/usagebench" \
      org.opencontainers.image.version="${USAGEBENCH_RELEASE}-env${ENVIRONMENT_VERSION}-bifrost" \
      ai.brokk.usagebench.environment.version="${ENVIRONMENT_VERSION}" \
      ai.brokk.usagebench.environment.definition-digest="${DEFINITION_DIGEST}" \
      ai.brokk.usagebench.runner.revision="${BIFROST_REVISION}"

FROM runtime-base AS gopls
ARG USAGEBENCH_RELEASE
ARG ENVIRONMENT_VERSION
ARG DEFINITION_DIGEST
ARG GOPLS_VERSION
COPY --from=gopls-builder /usr/local/go /usr/local/go
COPY --from=gopls-builder /out/gopls /usr/local/bin/gopls
ENV PATH="/usr/local/go/bin:${PATH}"
LABEL org.opencontainers.image.title="UsageBench gopls reference environment" \
      org.opencontainers.image.source="https://github.com/BrokkAi/usagebench" \
      org.opencontainers.image.version="${USAGEBENCH_RELEASE}-env${ENVIRONMENT_VERSION}-gopls" \
      ai.brokk.usagebench.environment.version="${ENVIRONMENT_VERSION}" \
      ai.brokk.usagebench.environment.definition-digest="${DEFINITION_DIGEST}" \
      ai.brokk.usagebench.runner.version="${GOPLS_VERSION}"
