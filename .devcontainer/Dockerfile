FROM ubuntu:24.04

RUN apt update && DEBIAN_FRONTEND=noninteractive apt install -y \
    curl \
    git \
    build-essential \
    clang \
    cmake \
    valgrind


ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:${PATH}

RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable 

WORKDIR /workspaces/memalloc

