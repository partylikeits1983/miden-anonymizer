//! Shared CLI helpers for the PTA binaries (`deploy_pta`, `use_pta`).
//!
//! Gated behind the `cli` feature because it pulls in `miden-client` and the
//! sqlite store. Library consumers that only want the PTA primitives don't
//! pay the cost.

use std::format;
use std::path::{Path, PathBuf};
use std::println;
use std::string::{String, ToString};
use std::sync::Arc;
use std::time::Duration;
use std::vec;
use std::vec::Vec;

use anyhow::{Context, Result, anyhow};
use rand::RngCore;
use tokio::time::sleep;

use miden_client::Client;
use miden_client::Felt;
use miden_client::account::component::AuthScheme;
use miden_client::account::{Account, AccountId, AccountStorageMode, AccountType, NetworkId};
use miden_client::asset::{Asset, FungibleAsset, TokenSymbol};
use miden_client::auth::AuthSecretKey;
use miden_client::builder::ClientBuilder;
use miden_client::keystore::{FilesystemKeyStore, Keystore};
use miden_client::note::{NoteAttachment, NoteType};
use miden_client::rpc::{Endpoint, GrpcClient};
use miden_client::store::TransactionFilter;
use miden_client::transaction::{TransactionId, TransactionRequestBuilder, TransactionStatus};
use miden_client_sqlite_store::ClientBuilderSqliteExt;
use miden_standards::AuthMethod;
use miden_standards::account::faucets::create_basic_fungible_faucet;
use miden_standards::account::wallets::create_basic_wallet;

use crate::account::PassThroughAccount;
use crate::note::P2idForwardNote;

/// The concrete `Client<AUTH>` flavour we use throughout the CLI.
pub type PtaClient = Client<FilesystemKeyStore>;

/// Resolve the testnet RPC endpoint, honoring `MIDEN_TESTNET_RPC` if set
/// (`scheme://host[:port]`).
pub fn testnet_endpoint() -> Endpoint {
    if let Ok(raw) = std::env::var("MIDEN_TESTNET_RPC") {
        let (protocol, rest) = raw.split_once("://").unwrap_or(("https", raw.as_str()));
        let (host, port) = match rest.rsplit_once(':') {
            Some((h, p)) if !h.is_empty() => (h.to_string(), p.parse::<u16>().ok()),
            _ => (rest.to_string(), None),
        };
        Endpoint::new(protocol.to_string(), host, port)
    } else {
        Endpoint::testnet()
    }
}

/// Build a fresh testnet `Client` rooted at `data_dir`.
///
/// The sqlite store and keystore directory live under `data_dir`; passing a
/// fresh tempdir per run guarantees a "blank slate" client (which is exactly
/// what `tests/testnet.rs` and the "any client" demo want).
pub async fn build_testnet_client(data_dir: &Path) -> Result<(PtaClient, Arc<FilesystemKeyStore>)> {
    if !data_dir.exists() {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("creating client data dir at {}", data_dir.display()))?;
    }

    let endpoint = testnet_endpoint();
    let rpc_client = Arc::new(GrpcClient::new(&endpoint, 10_000));

    let keystore =
        Arc::new(FilesystemKeyStore::new(data_dir.join("keystore")).context("opening keystore")?);

    let store_path: PathBuf = data_dir.join("store.sqlite3");
    let client = ClientBuilder::new()
        .rpc(rpc_client)
        .sqlite_store(store_path)
        .authenticator(keystore.clone())
        .in_debug_mode(true.into())
        .build()
        .await
        .context("building miden-client")?;

    Ok((client, keystore))
}

/// Polls until `tx_id` is committed (or the deadline elapses).
pub async fn wait_for_tx(client: &mut PtaClient, tx_id: TransactionId) -> Result<()> {
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    loop {
        client.sync_state().await.context("sync_state")?;
        let txs = client
            .get_transactions(TransactionFilter::Ids(vec![tx_id]))
            .await
            .context("get_transactions")?;
        let committed = txs
            .first()
            .is_some_and(|t| matches!(t.status, TransactionStatus::Committed { .. }));
        if committed {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(anyhow!("timed out waiting for tx {tx_id} to commit"));
        }
        sleep(Duration::from_secs(2)).await;
    }
}

/// Creates a new Falcon-512 wallet, registers it with the client + keystore,
/// and returns the `Account`. The returned account is "new" (carries its
/// creation seed) so the next transaction signed by it deploys it.
pub async fn create_wallet(
    client: &mut PtaClient,
    keystore: &FilesystemKeyStore,
) -> Result<Account> {
    let mut init_seed = [0u8; 32];
    client.rng().fill_bytes(&mut init_seed);
    let key_pair = AuthSecretKey::new_falcon512_poseidon2();
    let pub_key_commitment = key_pair.public_key().to_commitment();

    let account = create_basic_wallet(
        init_seed,
        AuthMethod::SingleSig {
            approver: (pub_key_commitment, AuthScheme::Falcon512Poseidon2),
        },
        AccountType::RegularAccountUpdatableCode,
        AccountStorageMode::Public,
    )
    .context("building wallet account")?;

    client
        .add_account(&account, false)
        .await
        .context("registering wallet with client")?;
    keystore
        .add_key(&key_pair, account.id())
        .await
        .context("storing wallet key")?;
    Ok(account)
}

/// Creates a new fungible faucet, registers it with the client + keystore,
/// and returns the `Account`. The first mint transaction deploys it.
pub async fn create_faucet(
    client: &mut PtaClient,
    keystore: &FilesystemKeyStore,
    symbol: &str,
    decimals: u8,
    max_supply: u64,
) -> Result<Account> {
    let mut init_seed = [0u8; 32];
    client.rng().fill_bytes(&mut init_seed);
    let key_pair = AuthSecretKey::new_falcon512_poseidon2();
    let pub_key_commitment = key_pair.public_key().to_commitment();

    let token_symbol = TokenSymbol::new(symbol).context("token symbol")?;
    let faucet = create_basic_fungible_faucet(
        init_seed,
        token_symbol,
        decimals,
        Felt::new(max_supply),
        AccountStorageMode::Public,
        AuthMethod::SingleSig {
            approver: (pub_key_commitment, AuthScheme::Falcon512Poseidon2),
        },
    )
    .context("building faucet account")?;

    client
        .add_account(&faucet, false)
        .await
        .context("registering faucet with client")?;
    keystore
        .add_key(&key_pair, faucet.id())
        .await
        .context("storing faucet key")?;
    Ok(faucet)
}

/// Mints `amount` of `faucet`'s asset to `target`, waits for commit, then has
/// `target` consume the resulting note (also waiting for commit). After this
/// returns, `target`'s vault holds the asset.
pub async fn mint_and_consume(
    client: &mut PtaClient,
    faucet: &Account,
    target: &Account,
    amount: u64,
) -> Result<()> {
    let asset = FungibleAsset::new(faucet.id(), amount).context("FungibleAsset::new")?;

    println!(
        "  minting {amount} of {} to {} ...",
        faucet.id(),
        target.id()
    );
    let mint_request = TransactionRequestBuilder::new()
        .build_mint_fungible_asset(asset, target.id(), NoteType::Public, client.rng())
        .context("building mint request")?;
    let mint_tx = client
        .submit_new_transaction(faucet.id(), mint_request)
        .await
        .context("submitting mint tx")?;
    println!("  mint tx: {}", midenscan_tx_url(mint_tx));
    wait_for_tx(client, mint_tx).await?;

    // Find the minted note destined for `target` and consume it.
    let consumable = client
        .get_consumable_notes(Some(target.id()))
        .await
        .context("get_consumable_notes")?;
    let mut notes: Vec<miden_client::note::Note> = Vec::new();
    for (record, _) in consumable {
        let note: miden_client::note::Note = record
            .try_into()
            .map_err(|_| anyhow!("consumable note has no full data yet"))?;
        notes.push(note);
    }
    if notes.is_empty() {
        return Err(anyhow!(
            "no consumable notes for {} after mint",
            target.id()
        ));
    }

    let consume_request = TransactionRequestBuilder::new()
        .build_consume_notes(notes)
        .context("building consume request")?;
    let consume_tx = client
        .submit_new_transaction(target.id(), consume_request)
        .await
        .context("submitting consume tx")?;
    println!("  consume tx: {}", midenscan_tx_url(consume_tx));
    wait_for_tx(client, consume_tx).await?;
    Ok(())
}

/// Drives the full P2IDF hop end-to-end:
///
/// 1. Alice's tx emits the P2IDF note (and waits for it to commit).
/// 2. The PTA's tx consumes the P2IDF and emits the outbound P2ID to Bob.
/// 3. Bob's tx redeems the outbound P2ID into his vault.
///
/// Returns the three transaction IDs in flow order. All three accounts must
/// already be tracked by the client; Alice must hold the asset(s) being
/// forwarded. The outbound P2ID note is private, so it's passed to Bob's
/// consume request explicitly via the precomputed
/// [`P2idForwardPair::outbound`] - no chain round-trip needed.
pub async fn forward_through_pta(
    client: &mut PtaClient,
    alice: &Account,
    pta: &Account,
    bob: &Account,
    assets: Vec<Asset>,
) -> Result<(TransactionId, TransactionId, TransactionId)> {
    let pair = P2idForwardNote::create(
        alice.id(),
        pta.id(),
        bob.id(),
        assets,
        NoteAttachment::default(),
        client.rng(),
    )
    .context("building P2IDF note")?;

    // Alice's tx: emit the P2IDF note as an own_output_note. The basic-wallet
    // template script will pull the asset out of Alice's vault into the note.
    let alice_request = TransactionRequestBuilder::new()
        .own_output_notes(vec![pair.inbound.clone()])
        .build()
        .context("building Alice's P2IDF send request")?;
    let alice_tx = client
        .submit_new_transaction(alice.id(), alice_request)
        .await
        .context("submitting Alice's P2IDF send tx")?;
    println!("  alice -> P2IDF tx: {}", midenscan_tx_url(alice_tx));
    wait_for_tx(client, alice_tx).await?;

    // PTA's tx: consume the P2IDF note. We pass the full Note so the executor
    // can unauthenticated-consume it even when the chain only has the
    // nullifier (the note is private).
    let pta_request = TransactionRequestBuilder::new()
        .input_notes([(pair.inbound, None)])
        .build()
        .context("building PTA consume request")?;
    let pta_tx = client
        .submit_new_transaction(pta.id(), pta_request)
        .await
        .context("submitting PTA consume tx")?;
    println!("  PTA forward tx: {}", midenscan_tx_url(pta_tx));
    wait_for_tx(client, pta_tx).await?;

    // Bob's tx: redeem the outbound P2ID. The note is private, so we hand
    // Bob the precomputed `pair.outbound` (bit-identical to what the PTA's
    // note script just emitted on-chain) and let him consume it.
    let bob_request = TransactionRequestBuilder::new()
        .input_notes([(pair.outbound, None)])
        .build()
        .context("building Bob's P2ID redeem request")?;
    let bob_tx = client
        .submit_new_transaction(bob.id(), bob_request)
        .await
        .context("submitting Bob's P2ID redeem tx")?;
    println!("  bob redeem tx:  {}", midenscan_tx_url(bob_tx));
    wait_for_tx(client, bob_tx).await?;

    Ok((alice_tx, pta_tx, bob_tx))
}

/// Builds (but does not submit) a fresh PTA. The returned account carries its
/// creation seed; calling `client.add_account(&pta, false)` then submitting
/// any tx against it deploys it.
pub fn build_new_pta(rng: &mut impl RngCore) -> Result<Account> {
    let mut init_seed = [0u8; 32];
    rng.fill_bytes(&mut init_seed);
    let (pta, _) = PassThroughAccount::build(init_seed).context("building PTA")?;
    Ok(pta)
}

/// Returns the testnet midenscan URL for the given transaction ID.
pub fn midenscan_tx_url(tx_id: TransactionId) -> String {
    format!("https://testnet.midenscan.com/tx/{}", tx_id)
}

/// Returns the testnet midenscan URL for the given account.
pub fn midenscan_account_url(id: AccountId) -> String {
    format!(
        "https://testnet.midenscan.com/account/{}",
        id.to_bech32(NetworkId::Testnet)
    )
}

/// Returns the bech32-encoded testnet address for the given account ID.
pub fn bech32_testnet(id: AccountId) -> String {
    id.to_bech32(NetworkId::Testnet)
}
