FROM rust:1.65.0-bullseye as build-env

RUN apt update && apt install -y build-essential

WORKDIR /app
COPY . /app
RUN cargo build

FROM gcr.io/distroless/cc
COPY --from=build-env /app/target/release/umbrel-to-citadel /

CMD ["/umbrel-to-citadel"]
