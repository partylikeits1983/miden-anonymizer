//! Deploys a fresh **public** Pass-Through Account on Miden testnet.
//!
//! In Miden a regular account is "deployed" by submitting any transaction
//! whose sender is the new account (the seed embedded in the account is
//! consumed to construct its commitment in the chain). For the PTA the
//! natural first transaction is the one that consumes a P2IDF note - that's
//! the entire reason the PTA exists. So `deploy_pta` runs the full
//! Alice → PTA → Bob single-hop forward against a freshly built PTA.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --features cli --bin deploy_pta
//! ```
//!
//! The PTA's bech32 address is printed at the end - copy that into the README
//! (and pass it to `use_pta`) so subsequent runs reuse the same on-chain PTA.

use std::path::PathBuf;

use anyhow::{Context, Result};

use miden_anonymizer::cli::{
    bech32_testnet, build_new_pta, build_testnet_client, create_faucet, create_wallet,
    forward_through_pta, midenscan_account_url, midenscan_tx_url, mint_and_consume,
};
use miden_client::asset::FungibleAsset;

const ASSET_AMOUNT: u64 = 100;

#[tokio::main]
async fn main() -> Result<()> {
    let data_dir = data_dir_from_env_or_default();
    println!("client data dir: {}", data_dir.display());

    let (mut client, keystore) = build_testnet_client(&data_dir).await?;
    let sync = client.sync_state().await.context("initial sync_state")?;
    println!("synced to block {}", sync.block_num);

    // 1. Build a fresh PTA. Carries its creation seed; the first tx against
    // it deploys it.
    let pta = build_new_pta(client.rng())?;
    let pta_bech32 = bech32_testnet(pta.id());
    println!("\nbuilt new PTA");
    println!("  id (hex):    {}", pta.id());
    println!("  id (bech32): {}", pta_bech32);
    println!("  midenscan:   {}", midenscan_account_url(pta.id()));

    client
        .add_account(&pta, false)
        .await
        .context("registering PTA with client")?;

    // 2. Create Alice (sender) and Bob (final recipient) wallets.
    println!("\ncreating Alice and Bob wallets");
    let alice = create_wallet(&mut client, &keystore).await?;
    let bob = create_wallet(&mut client, &keystore).await?;
    println!("  alice: {}", bech32_testnet(alice.id()));
    println!("  bob:   {}", bech32_testnet(bob.id()));

    // 3. Spin up an ephemeral faucet just to fund Alice. Using a dedicated
    // per-run faucet keeps the demo self-contained (no dependency on a
    // hosted testnet faucet).
    println!("\ndeploying ephemeral faucet");
    let faucet = create_faucet(&mut client, &keystore, "PTA", 8, 1_000_000).await?;
    println!("  faucet: {}", bech32_testnet(faucet.id()));

    // 4. Mint into Alice's vault.
    println!("\nminting {ASSET_AMOUNT} {{PTA}} to Alice");
    mint_and_consume(&mut client, &faucet, &alice, ASSET_AMOUNT).await?;

    // 5. Drive the forward: Alice → PTA → Bob. The PTA's tx is the deploy.
    println!("\nforwarding through PTA (this is the deploy tx)");
    let asset =
        FungibleAsset::new(faucet.id(), ASSET_AMOUNT).context("constructing asset for forward")?;
    let (alice_tx, pta_tx) =
        forward_through_pta(&mut client, &alice, &pta, &bob, vec![asset.into()]).await?;

    println!("\n✅ PTA deployed.");
    println!("  PTA bech32:    {}", pta_bech32);
    println!("  PTA midenscan: {}", midenscan_account_url(pta.id()));
    println!("  alice tx:      {}", midenscan_tx_url(alice_tx));
    println!("  PTA tx:        {}", midenscan_tx_url(pta_tx));
    println!();
    println!("Add this line to README's deployed PTA section:");
    println!("  {pta_bech32}");

    Ok(())
}

fn data_dir_from_env_or_default() -> PathBuf {
    if let Ok(dir) = std::env::var("PTA_DATA_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from("./pta-data/deploy")
}
