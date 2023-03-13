FROM rust:latest as build

RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk wasm-bindgen-cli

WORKDIR /usr/src/own_your_saves
COPY . .
RUN cd frontend && trunk build --release
RUN cargo build --release

FROM gcr.io/distroless/cc-debian10 as runner

COPY --from=build /usr/src/own_your_saves/target/release/own_your_saves /usr/local/bin/own_your_saves
COPY --from=build /usr/src/own_your_saves/frontend/dist /usr/local/bin/dist

WORKDIR /usr/local/bin
CMD ["own_your_saves"]