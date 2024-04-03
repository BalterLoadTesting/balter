build-release:
    cargo build --release

prep:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test --release
    cargo semver-checks

version EXECUTE:
    cargo release version --exclude balter-tests --exclude mock-service --exclude examples minor {{EXECUTE}}

publish:
    cd balter-macros && cargo publish
    cd balter-core && cargo publish
    cd balter-runtime && cargo publish
    cd balter && cargo publish
