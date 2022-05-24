# Rust Helper

This server daemon is meant to be locally run and accessed via localhost.  The purpose is to allow access to Solana Rust constructs available natively in only Rust and Typescript.  The mechanism used to provide access is gRPC + Protobufs.

# Installation

## Download Binary

This has not been implemented yet.

## Compile from Source

```bash
git clone --branch=main https://github.com/SolmateDev/rust-helper
cd rust-helper
cargo build --bin=server --release
```
