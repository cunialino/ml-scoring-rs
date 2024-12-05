ARG RUST_VERSION=1.80.1
ARG APP_NAME

FROM rust:${RUST_VERSION}-slim-bullseye AS builder

RUN [ $APP_NAME="scoring_server" ] && apt update && apt upgrade -y && apt install cmake gcc g++ libclang-dev libssl-dev pkg-config -y

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
cp ./target/release/$APP_NAME /bin/$APP_NAME
EOF

FROM debian:bullseye-slim AS final


ARG APP_NAME

COPY --from=builder /bin/$APP_NAME /bin/$APP_NAME

WORKDIR /usr/src/app

RUN [ $APP_NAME="scoring_server" ] && apt update && apt install libgomp1 libssl-dev -y

ENV LD_PATH=/usr/lib/x86_64-linux-gnu

ENV APP_NAME=$APP_NAME

CMD ["/bin/sh", "-c", "/bin/$APP_NAME"]
