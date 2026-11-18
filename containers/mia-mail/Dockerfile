# mia-mail — server MCP per Gmail (stdio).
# Il token OAuth vive in /data/token.json (volume); l'autorizzazione
# si fa una volta con: docker compose run --rm mail auth
FROM rust:1.88-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p mia-mail

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m mia && mkdir -p /data && chown mia:mia /data
COPY --from=builder /build/target/release/mia-mail /usr/local/bin/mia-mail
USER mia
VOLUME ["/data"]
ENTRYPOINT ["mia-mail", "/data"]
