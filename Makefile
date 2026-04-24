.PHONY: test e2e-test format

# Local tests (MockChain only). --nocapture so P2IDF cycle counts print.
test:
	cargo test --release -- --nocapture

# Testnet tests. Gated with #[ignore]; runs only tests/testnet.rs so the
# MockChain suite is not re-run against a live node. Reads MIDEN_TESTNET_RPC
# (optional - falls back to Endpoint::testnet()).
e2e-test:
	cargo test --release --test testnet -- --ignored --nocapture

format:
	cargo fmt --all
