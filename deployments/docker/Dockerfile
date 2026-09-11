FROM rust:1-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p sdkwork-api-inventory-standalone-gateway

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /src/target/release/sdkwork-api-inventory-standalone-gateway /usr/local/bin/sdkwork-api-inventory-standalone-gateway
ENV INVENTORY_API_BIND=0.0.0.0:18092
EXPOSE 18092
CMD ["sdkwork-api-inventory-standalone-gateway"]
