test:
    cargo nextest run --workspace --all-features

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

verify: fmt-check clippy test

install:
    HELIX_DEFAULT_RUNTIME="{{ justfile_directory() }}/runtime" cargo install --path helix-term --locked
