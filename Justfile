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

basic-tps: mock-service
    cargo build --release --example basic-tps
    bash test-scripts/basic-test-runner.sh basic-tps

basic-saturate: mock-service
    cd examples/basic-examples && cargo build --release --example basic-saturate
    bash test-scripts/basic-test-runner.sh basic-saturate

gossip-test:
    cargo build --release --example distr-tps
    bash test-scripts/gossip-test-runner.sh

distr-tps: mock-service
    cargo build --release --example distr-tps
    bash test-scripts/distr-tps-test-runner.sh

distr-saturate: mock-service
    cargo build --release --example distr-saturate
    bash test-scripts/distr-saturate-test-runner.sh

mock-service:
    cargo build --release --bin mock-service
