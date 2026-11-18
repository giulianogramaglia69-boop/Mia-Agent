# mia-notes — server MCP per le note (stdio).
# Build: contesto = radice del workspace (il crate è un membro del workspace).
FROM rust:1.88-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p mia-notes

FROM debian:bookworm-slim
RUN useradd -m mia && mkdir -p /data && chown mia:mia /data
COPY --from=builder /build/target/release/mia-notes /usr/local/bin/mia-notes
USER mia
VOLUME ["/data"]
ENTRYPOINT ["mia-notes", "/data"]
