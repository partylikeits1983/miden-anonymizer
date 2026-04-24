//! PTA single-hop demo against Miden testnet.
//!
//! This binary mirrors the style of `tutorials/rust-client/src/bin/*`: it
//! connects to the Miden testnet via `miden-client`, creates accounts for
//! Alice and Bob, deploys a pass-through account, and drives a single P2IDF
//! transfer Alice -> PTA -> Bob.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example pta_single_hop_demo
//! ```
//!
//! NOTE: This demo is a scaffolding stub. Parts of it are commented out
//! because the PTA's custom note script and auth component need to be
//! registered with the client's proving/execution surface. Use it as a
//! starting point; flesh out the account-deployment and transaction-request
//! construction to exercise the P2IDF flow on a live node.

use std::sync::Arc;

use miden_anonymizer::account::PassThroughAccount;
use miden_client::ClientError;
use miden_client::builder::ClientBuilder;
use miden_client::keystore::FilesystemKeyStore;
use miden_client::rpc::{Endpoint, GrpcClient};
use miden_client_sqlite_store::ClientBuilderSqliteExt;
use rand::RngCore;

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    // --- 1. Connect to testnet ------------------------------------------

    let endpoint = Endpoint::testnet();
    let rpc_client = Arc::new(GrpcClient::new(&endpoint, 10_000));

    let keystore = Arc::new(
        FilesystemKeyStore::new(std::path::PathBuf::from("./keystore"))
            .expect("failed to open keystore"),
    );

    let mut client = ClientBuilder::new()
        .rpc(rpc_client)
        .sqlite_store(std::path::PathBuf::from("./store.sqlite3"))
        .authenticator(keystore.clone())
        .in_debug_mode(true.into())
        .build()
        .await?;

    let sync = client.sync_state().await?;
    println!("synced to block {}", sync.block_num);

    // --- 2. Build a pass-through account --------------------------------

    let mut init_seed = [0u8; 32];
    client.rng().fill_bytes(&mut init_seed);

    let (pta_account, _pta_seed) =
        PassThroughAccount::build(init_seed).expect("failed to build PTA account");
    println!("built PTA account id = {}", pta_account.id());

    // TODO(deployment): register the PTA with the client and submit its
    // deployment transaction. The client currently does not expose a single
    // function for injecting an externally-built Account; we will need to
    // drive account creation through `client.add_account(...)` or equivalent
    // and then submit a no-op transaction so the PTA's commitment is tracked.

    // --- 3. Build Alice & Bob wallets (TODO) ----------------------------
    // See tutorials/rust-client/src/bin/create_mint_consume_send.rs for the
    // Falcon-512 wallet creation idiom. Fund Alice via a faucet and invoke
    // miden_anonymizer::note::P2idForwardNote::create to build the inbound
    // P2IDF note for forwarding through the PTA to Bob.

    println!("demo scaffolding done; flesh out the TODOs to run end-to-end against testnet.");
    Ok(())
}
