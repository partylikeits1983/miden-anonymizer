//! Drive a single P2IDF forward through an **already-deployed** public PTA on
//! Miden testnet.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --features cli --bin use_pta -- <PTA_BECH32>
//! ```
//!
//! If the bech32 argument is omitted, falls back to the address baked into
//! `DEFAULT_PTA_BECH32` below — keep that in sync with the README.
//!
//! The point of this binary (and the matching test in `tests/testnet.rs`) is
//! to demonstrate that *any* fresh client instance — no shared keystore, no
//! shared sqlite store, no out-of-band setup — can submit a transaction
//! against the public PTA. The PTA's `VaultEmptyAuth` component requires no
//! signature, so all the new client needs is the PTA's public state (fetched
//! via `import_account_by_id`).

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};

use miden_anonymizer::cli::{
    bech32_testnet, build_testnet_client, create_faucet, create_wallet, forward_through_pta,
    midenscan_account_url, midenscan_tx_url, mint_and_consume,
};
use miden_client::account::AccountId;
use miden_client::asset::FungibleAsset;

/// Filled in by `deploy_pta` and committed to the README. Override on the
/// command line if you need to point at a different deployment.
const DEFAULT_PTA_BECH32: &str = "mtst1azy607tkxe7fyqq604l2ysp55qqs2whr";

const ASSET_AMOUNT: u64 = 100;

#[tokio::main]
async fn main() -> Result<()> {
    let pta_bech32 = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_PTA_BECH32.to_string());
    if pta_bech32.starts_with('<') {
        return Err(anyhow!(
            "no PTA address provided. Pass one as the first CLI arg, or set DEFAULT_PTA_BECH32 \
             in src/bin/use_pta.rs after running deploy_pta."
        ));
    }

    let (_network, pta_id) =
        AccountId::from_bech32(&pta_bech32).context("decoding PTA bech32 address")?;

    let data_dir = data_dir_from_env_or_default();
    println!("client data dir: {}", data_dir.display());
    println!("target PTA:      {}", pta_bech32);

    let (mut client, keystore) = build_testnet_client(&data_dir).await?;
    let sync = client.sync_state().await.context("initial sync_state")?;
    println!("synced to block {}", sync.block_num);

    // 1. Pull the PTA's public state from the chain into this fresh client.
    println!("\nimporting PTA from chain ...");
    client
        .import_account_by_id(pta_id)
        .await
        .context("import_account_by_id for PTA")?;
    let pta = client
        .try_get_account(pta_id)
        .await
        .context("fetching imported PTA from local store")?;
    println!("  imported (commitment {})", pta.to_commitment());

    // 2. Fresh Alice and Bob wallets — no shared state with the PTA's
    // original deployer.
    println!("\ncreating fresh Alice and Bob wallets");
    let alice = create_wallet(&mut client, &keystore).await?;
    let bob = create_wallet(&mut client, &keystore).await?;
    println!("  alice: {}", bech32_testnet(alice.id()));
    println!("  bob:   {}", bech32_testnet(bob.id()));

    // 3. Spin up our own faucet and fund Alice.
    println!("\ndeploying ephemeral faucet");
    let faucet = create_faucet(&mut client, &keystore, "USE", 8, 1_000_000).await?;
    println!("  faucet: {}", bech32_testnet(faucet.id()));

    println!("\nminting {ASSET_AMOUNT} {{USE}} to Alice");
    mint_and_consume(&mut client, &faucet, &alice, ASSET_AMOUNT).await?;

    // 4. Forward through the existing PTA.
    println!("\nforwarding through PTA");
    let asset =
        FungibleAsset::new(faucet.id(), ASSET_AMOUNT).context("constructing asset for forward")?;
    let (alice_tx, pta_tx) =
        forward_through_pta(&mut client, &alice, &pta, &bob, vec![asset.into()]).await?;

    println!("\n✅ forwarded through public PTA.");
    println!("  PTA bech32:    {}", pta_bech32);
    println!("  PTA midenscan: {}", midenscan_account_url(pta.id()));
    println!("  alice tx:      {}", midenscan_tx_url(alice_tx));
    println!("  PTA tx:        {}", midenscan_tx_url(pta_tx));

    Ok(())
}

fn data_dir_from_env_or_default() -> PathBuf {
    if let Ok(dir) = std::env::var("PTA_DATA_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from("./pta-data/use")
}
