FROM lukemathwalker/cargo-chef:latest-rust-1.95 AS chef
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
RUN useradd --system --create-home --home-dir /var/lib/midden --shell /usr/sbin/nologin midden

WORKDIR /var/lib/midden
COPY --from=build /app/target/release/midden /usr/local/bin/midden
COPY --from=build /app/midden.example.toml /etc/midden.toml

USER midden
EXPOSE 8080
CMD ["midden", "--config", "/etc/midden.toml", "serve"]