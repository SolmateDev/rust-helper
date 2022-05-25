# Rust Helper

A developer wants to use a language like Python, Golang, or maybe C++.  However, your favorite project like [Serum (onchain limit order book)](https://github.com/project-serum/serum-dex) and [Metaplex (of NFT fame)](https://github.com/metaplex-foundation/metaplex-program-library) only have bindings for Rust and Typescript.

This rust server daemon bridges the gap by providing a gRPC endpoint that corresponds to protobuf definitions in the `./proto` directory.  So a developer can call Rust functions over localhost, thereby granting a non-binded language like Python access to Solana program constructs.

# Installation

## Download Binary

This has not been implemented yet.

## Compile from Source

```bash
git clone --branch=main https://github.com/SolmateDev/rust-helper
cd rust-helper
cargo build --bin=server --release
```

## Compiling Client Libraries

Solmate has made several libraries available on Github.  However, one may use [the Protoc](https://grpc.io/docs/protoc-installation/) CLI program to compile bindings using the protobuf files located in the `./proto` directory.



# Running

Create an environmental variable file at `.env` that looks like:

```
GRPC_LISTEN_URL="0.0.0.0:50051"
VALIDATOR_HTTP_URL="http://127.0.0.1:8899"
VALIDATOR_WS_URL="ws://127.0.0.1:8900"
```

Make sure to have JSON RPC access to a validator.


