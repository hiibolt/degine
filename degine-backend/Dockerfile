FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM leanprover/lean4:v4.34.1
WORKDIR /app
COPY --from=build /src/target/release/degine-backend /app/degine-backend
COPY lean-project /app/lean-project
WORKDIR /app/lean-project
RUN lake build
WORKDIR /app
COPY entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh
EXPOSE 3000
ENTRYPOINT ["/app/entrypoint.sh"]
