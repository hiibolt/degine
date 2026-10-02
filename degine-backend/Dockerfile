FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm AS chef
WORKDIR /src

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl git \
    && rm -rf /var/lib/apt/lists/* \
    && curl -sSf https://elan.lean-lang.org/elan-init.sh | sh -s -- -y --default-toolchain leanprover/lean4:v4.34.1
ENV PATH="/root/.elan/bin:${PATH}"
WORKDIR /app
COPY lean-project /app/lean-project
WORKDIR /app/lean-project
RUN lake build
WORKDIR /app
COPY --from=build /src/target/release/degine-backend /app/degine-backend
COPY entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh
EXPOSE 3000
ENTRYPOINT ["/app/entrypoint.sh"]
