//! Testnet end-to-end scaffold.
//!
//! Every test in this file is `#[ignore]` so `cargo test` (and `make test`)
//! skip it by default. Run with `make e2e-test`.
//!
//! Endpoint: `MIDEN_TESTNET_RPC` env var (parsed as `scheme://host[:port]`)
//! if set, otherwise `Endpoint::testnet()`.

use std::path::PathBuf;
use std::sync::Arc;

use miden_client::builder::ClientBuilder;
use miden_client::keystore::FilesystemKeyStore;
use miden_client::rpc::{Endpoint, GrpcClient};
use miden_client_sqlite_store::ClientBuilderSqliteExt;

/// Scratch directory that is cleaned up on drop so the sqlite store + keystore
/// from a previous run cannot poison the next one.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "miden-anonymizer-e2e-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        std::fs::create_dir_all(&path).expect("create tempdir");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Resolve the RPC endpoint from env or fall back to testnet.
fn endpoint_from_env() -> Endpoint {
    match std::env::var("MIDEN_TESTNET_RPC") {
        Ok(raw) => parse_endpoint(&raw),
        Err(_) => Endpoint::testnet(),
    }
}

fn parse_endpoint(raw: &str) -> Endpoint {
    let (protocol, rest) = raw.split_once("://").unwrap_or(("https", raw));
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() => (h, p.parse::<u16>().ok()),
        _ => (rest, None),
    };
    Endpoint::new(protocol.to_string(), host.to_string(), port)
}

#[tokio::test]
#[ignore]
async fn connects_to_testnet_and_syncs() {
    let endpoint = endpoint_from_env();
    println!("connecting to {endpoint}");

    let rpc_client = Arc::new(GrpcClient::new(&endpoint, 10_000));

    let scratch = TempDir::new("sync");
    let keystore =
        Arc::new(FilesystemKeyStore::new(scratch.join("keystore")).expect("open keystore"));

    let mut client = ClientBuilder::new()
        .rpc(rpc_client)
        .sqlite_store(scratch.join("store.sqlite3"))
        .authenticator(keystore)
        .in_debug_mode(true.into())
        .build()
        .await
        .expect("build client");

    let summary = client.sync_state().await.expect("sync_state");
    println!("synced to block {}", summary.block_num);
    assert!(
        summary.block_num.as_u32() > 0,
        "testnet block height should advance past genesis"
    );
}

#[tokio::test]
#[ignore]
async fn pta_single_hop_on_testnet() {
    // TODO: build PTA, fund Alice via faucet, send P2IDF through the PTA to
    // Bob, and assert Bob receives the expected asset. Mirror the flow in
    // `tests/single_hop.rs` but drive it through `miden-client` against a
    // live testnet node. See `examples/pta_single_hop_demo.rs` for the
    // scaffolding of the client/keystore/account-deployment surface.
    todo!("flesh out single-hop testnet flow");
}
