# build image
FROM rust:1.82 AS builder

WORKDIR /usr/src/aoi
COPY . .
RUN cargo install --path .

# base image
FROM debian:bookworm-slim AS app
WORKDIR /app
RUN apt update && apt install -y fonts-noto-cjk fonts-inter openssl
COPY --from=builder /usr/local/cargo/bin/aoi /usr/local/bin/aoi
COPY templates .
ENTRYPOINT ["aoi"]
