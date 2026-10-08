# PS5 payload toolchain: Rust (build-std) + ps5-payload-dev SDK. Built by ps5/build.sh.
# Runs on arm64 or x86-64 hosts: everything here cross-compiles, nothing is emulated.
# ponytail: the Rust image is pinned by digest and the SDK by SHA-256; Debian's clang-19 packages
# float within trixie (point releases only). Pin a snapshot.debian.org date if that ever bites.
FROM rust:1.99.0-slim-trixie@sha256:24e632c09342c20abf8312cf4f61430a911c01ed3a5e4c02b87292b1c39c5273
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates clang-19 lld-19 llvm-19 make curl unzip python3 \
    && rm -rf /var/lib/apt/lists/*
ENV PS5_PAYLOAD_SDK=/opt/ps5-payload-sdk LLVM_CONFIG=/usr/bin/llvm-config-19
ENV PATH=/usr/lib/llvm-19/bin:$PATH
# Override the repo's rust-toolchain.toml (stable): build-std needs this toolchain's patched rust-src.
ENV RUSTUP_TOOLCHAIN=1.99.0 RUSTUP_AUTO_INSTALL=0
ARG SDK_URL=https://github.com/ps5-payload-dev/sdk/releases/download/v0.43/ps5-payload-sdk.zip
ARG SDK_SHA256=a9cc9929f21b2b2c5d5b309f3bab4997067c45281c0622cf4838b1aecba66fcb
RUN curl -fsSL --proto =https --retry 3 "$SDK_URL" -o /tmp/sdk.zip \
    && echo "$SDK_SHA256  /tmp/sdk.zip" | sha256sum -c - \
    && unzip -q /tmp/sdk.zip -d /opt && rm /tmp/sdk.zip \
    && test -x "$PS5_PAYLOAD_SDK/bin/prospero-clang"
RUN rustup component add rust-src
COPY prepare-rust-std.py /tmp/
RUN python3 /tmp/prepare-rust-std.py && rm /tmp/prepare-rust-std.py
WORKDIR /work
