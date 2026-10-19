FROM rust:1.99-slim AS builder

RUN apt-get update \
    && apt-get install --yes --no-install-recommends musl-tools ca-certificates busybox-static \
    && mkdir -p /out/bin \
    && cp /bin/busybox /out/bin/busybox \
    && ln -s busybox /out/bin/sh \
    && for applet in cat echo ls mkdir pwd rm touch wc head tail grep sed awk sort; do ln -s busybox "/out/bin/$applet"; done \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY specs ./specs
RUN cargo build --locked --release --target x86_64-unknown-linux-musl \
    -p ms-agent -p ms-tools

FROM scratch
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/microswarm-agent /microswarm-agent
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/microswarm-tools /microswarm-tools
COPY --from=builder /out/bin /bin
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
USER 65532:65532
WORKDIR /session
ENTRYPOINT ["/microswarm-agent"]
