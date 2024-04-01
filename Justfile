build-release:
    cargo build --release

prep:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test --release
    cargo semver-checks

publish:
    cd balter-macros && cargo publish
    cd balter-core && cargo publish
    cd balter-runtime && cargo publish
    cd balter && cargo publish
