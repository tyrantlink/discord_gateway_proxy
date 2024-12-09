FROM rust:1.82

COPY . .

RUN cargo build --release

CMD './target/release/discord_gateway_proxy'