ARG RUST_VERSION=1.95
FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION}@sha256:00c3c07c51d092325df88f0df2d626cd4302e12933f179ba154509cc314d6c2a AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json

COPY . .
RUN cargo build --release --locked --bin midden

FROM debian:trixie-slim

RUN apt-get update \
 && apt-get install --yes --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 10001 midden \
 && useradd --system \
      --uid 10001 \
      --gid 10001 \
      --create-home \
      --home-dir /var/lib/midden \
      --shell /usr/sbin/nologin \
      midden

WORKDIR /var/lib/midden

COPY --from=build /app/target/release/midden /usr/local/bin/midden
COPY --from=build /app/midden.example.toml /etc/midden.toml

ENV MIDDEN__SERVER__BIND=0.0.0.0:8080

USER 10001:10001

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl --fail --silent --show-error http://127.0.0.1:8080/healthz || exit 1

CMD ["midden", "--config", "/etc/midden.toml", "serve"]
