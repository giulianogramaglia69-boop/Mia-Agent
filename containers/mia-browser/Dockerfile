# mia-browser — server MCP per il web (stdio).
FROM rust:1.88-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p mia-browser

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m mia
COPY --from=builder /build/target/release/mia-browser /usr/local/bin/mia-browser
USER mia
ENTRYPOINT ["mia-browser"]
