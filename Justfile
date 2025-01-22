default:
    just --list

build-release:
    cargo build --release

prep:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test --release

version EXECUTE='' VERSION='minor':
    cargo release version -p balter -p balter-macros -p balter-core -p balter-runtime {{VERSION}} {{EXECUTE}}

publish EXECUTE='':
    cargo release publish -p balter -p balter-macros -p balter-core -p balter-runtime {{EXECUTE}}

integration TEST='':
    cargo test --release --features integration {{TEST}} -- --nocapture
