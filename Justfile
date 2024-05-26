build-release:
    cargo build --release

prep:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test --release
    cargo semver-checks

version EXECUTE='':
    cargo release version --exclude balter-tests --exclude mock-service --exclude examples minor {{EXECUTE}}

publish EXECUTE='':
    cargo release publish -p balter -p balter-macros -p balter-core -p balter-runtime {{EXECUTE}}

integration TEST='':
    cargo test --release --features integration {{TEST}} -- --nocapture
