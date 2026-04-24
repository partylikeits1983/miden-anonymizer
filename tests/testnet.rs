//! Testnet end-to-end scaffold.
//!
//! Every test in this file is `#[ignore]` so `cargo test` (and `make test`)
//! skip it by default. Run with `make e2e-test`.
//!
//! Endpoint: `MIDEN_TESTNET_RPC` env var (parsed as `scheme://host[:port]`)
//! if set, otherwise `Endpoint::testnet()`.

use std::path::PathBuf;
use std::sync::Arc;

use miden_anonymizer::cli::{
    bech32_testnet, build_testnet_client, create_faucet, create_wallet, forward_through_pta,
    midenscan_account_url, midenscan_tx_url, mint_and_consume,
};
use miden_client::account::AccountId;
use miden_client::asset::FungibleAsset;
use miden_client::builder::ClientBuilder;
use miden_client::keystore::FilesystemKeyStore;
use miden_client::rpc::{Endpoint, GrpcClient};
use miden_client_sqlite_store::ClientBuilderSqliteExt;

/// The public PTA on Miden testnet that this test exercises. Keep in sync
/// with the README and `src/bin/use_pta.rs`.
const PUBLIC_PTA_BECH32: &str = "mtst1azy607tkxe7fyqq604l2ysp55qqs2whr";

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

/// Demonstrates the core anonymity-set property of the public PTA: *any*
/// fresh client instance - no shared keystore, no shared sqlite store, no
/// out-of-band setup - can drive a transaction against the deployed PTA.
///
/// All this client knows is the PTA's bech32 address (a pubic constant). It
/// imports the PTA's public state via `import_account_by_id`, then runs a
/// single P2IDF forward through it. The PTA's `VaultEmptyAuth` requires no
/// signature, so no key material ever leaves the original deployer.
#[tokio::test]
#[ignore]
async fn any_client_can_use_public_pta() -> anyhow::Result<()> {
    use anyhow::Context;

    let scratch = TempDir::new("any-client");
    let (mut client, keystore) = build_testnet_client(&scratch.0).await?;
    let sync = client.sync_state().await.context("initial sync_state")?;
    println!("fresh client synced to block {}", sync.block_num);

    let (_network, pta_id) =
        AccountId::from_bech32(PUBLIC_PTA_BECH32).context("decoding PTA bech32")?;

    println!("importing public PTA {} ...", PUBLIC_PTA_BECH32);
    client
        .import_account_by_id(pta_id)
        .await
        .context("import PTA")?;
    let pta = client
        .try_get_account(pta_id)
        .await
        .context("fetching imported PTA")?;
    println!("  midenscan: {}", midenscan_account_url(pta.id()));

    let alice = create_wallet(&mut client, &keystore).await?;
    let bob = create_wallet(&mut client, &keystore).await?;
    println!("alice: {}", bech32_testnet(alice.id()));
    println!("bob:   {}", bech32_testnet(bob.id()));

    let faucet = create_faucet(&mut client, &keystore, "ANY", 8, 1_000_000).await?;
    println!("faucet: {}", bech32_testnet(faucet.id()));

    let amount: u64 = 50;
    mint_and_consume(&mut client, &faucet, &alice, amount).await?;

    let asset = FungibleAsset::new(faucet.id(), amount).context("FungibleAsset::new")?;
    let (alice_tx, pta_tx) =
        forward_through_pta(&mut client, &alice, &pta, &bob, vec![asset.into()]).await?;

    println!("alice tx: {}", midenscan_tx_url(alice_tx));
    println!("PTA tx:   {}", midenscan_tx_url(pta_tx));

    // The PTA tx committing on-chain *is* the property under test: a fresh
    // client, with no shared state, was able to drive a transaction against
    // the public PTA. `forward_through_pta` already waited for the PTA tx
    // to commit, so reaching this point with `Ok(_)` is the assertion.
    //
    // We deliberately don't probe Bob's inbox here - the outbound note is
    // private and propagating it to a separate client is out of scope for
    // this test (and unrelated to the "any client" property).
    Ok(())
}
