FROM rust:1.95 AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
COPY templates ./templates
RUN cargo build --release

FROM debian:trixie-slim
RUN useradd --system --create-home --home-dir /var/lib/midden midden
WORKDIR /var/lib/midden
COPY --from=build /app/target/release/midden /usr/local/bin/midden
COPY midden.example.toml /etc/midden.toml
USER midden
EXPOSE 8080
CMD ["midden", "--config", "/etc/midden.toml", "serve"]
