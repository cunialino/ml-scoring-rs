ARG RUST_VERSION=1.80.1
ARG APP_NAME

FROM rust:${RUST_VERSION}-slim-bullseye AS builder
ARG APP_NAME
WORKDIR /usr/src/app

RUN --mount=type=bind,source=services,target=services \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/usr/src/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
 <<EOF
set -e
cargo build --locked --release -p $APP_NAME
cp ./target/release/$APP_NAME /bin/server
EOF

FROM debian:bullseye-slim AS final

COPY --from=builder /bin/server /bin/

WORKDIR /usr/src/app

CMD ["/bin/server"]
