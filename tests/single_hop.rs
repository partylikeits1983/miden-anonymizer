//! End-to-end test: Alice -> PTA -> Bob (single-hop via MockChain).
//!
//! Asserts:
//!   - After the PTA consumes the P2IDF note, its vault is empty.
//!   - The emitted outbound note's sender is the PTA, not Alice.
//!   - The precomputed `P2idForwardPair.outbound` matches the executor's
//!     output note bit-for-bit, and Bob can consume that note to take
//!     possession of the forwarded asset.

use anyhow::Result;
use miden_anonymizer::account::PassThroughAccount;
use miden_anonymizer::note::P2idForwardNote;
use miden_protocol::Word;
use miden_protocol::asset::{Asset, FungibleAsset};
use miden_protocol::crypto::rand::RandomCoin;
use miden_protocol::note::NoteAttachment;
use miden_protocol::transaction::RawOutputNote;
use miden_standards::note::P2idNote;
use miden_testing::{Auth, MockChain};

const ASSET_AMOUNT: u64 = 100;

#[tokio::test]
async fn alice_sends_to_bob_via_pta() -> Result<()> {
    let mut builder = MockChain::builder();

    // Build Bob wallet and a faucet.
    let bob = builder.add_existing_wallet(Auth::IncrNonce)?;

    // Build and register the PTA at genesis (stripped of seed).
    let pta_account = PassThroughAccount::build_existing([7u8; 32])?;
    builder.add_account(pta_account.clone())?;

    // Create a fungible faucet.
    let faucet = builder.create_new_faucet(Auth::IncrNonce, "FOO", 1_000_000)?;
    let asset: Asset = FungibleAsset::new(faucet.id(), ASSET_AMOUNT)?.into();

    // Pre-fund Alice with the asset at genesis.
    let alice = builder.add_existing_wallet_with_assets(Auth::IncrNonce, [asset])?;

    // Alice builds a P2IDF note forwarding the asset through the PTA to Bob.
    // Seed the RNG deterministically so test output is reproducible.
    let mut rng = RandomCoin::new(Word::default());
    let pair = P2idForwardNote::create(
        alice.id(),
        pta_account.id(),
        bob.id(),
        vec![asset],
        NoteAttachment::default(),
        &mut rng,
    )?;
    let p2idf = pair.inbound;
    let outbound_p2id = pair.outbound;

    // Add the P2IDF note as a genesis-committed note so the PTA can consume
    // it as an authenticated input.
    builder.add_output_note(RawOutputNote::Full(p2idf.clone()));

    let mut chain = builder.build()?;

    // Execute the PTA tx consuming the P2IDF note.
    let executed_forward = chain
        .build_tx_context(pta_account.id(), &[p2idf.id()], &[])?
        .build()?
        .execute()
        .await?;

    // Cycle-count baseline for later PTA / P2IDF optimization work.
    let measurements = executed_forward.measurements();
    let p2idf_cycles = measurements
        .note_execution
        .iter()
        .find(|(id, _)| *id == p2idf.id())
        .map(|(_, c)| *c)
        .expect("P2IDF note measurement missing");
    println!("=== PTA single-hop cycle counts ===");
    println!("  total:            {}", measurements.total_cycles());
    println!("  trace_length:     {}", measurements.trace_length());
    println!("  prologue:         {}", measurements.prologue);
    println!("  notes_processing: {}", measurements.notes_processing);
    println!("  tx_script:        {}", measurements.tx_script_processing);
    println!("  epilogue:         {}", measurements.epilogue);
    println!("  auth_procedure:   {}", measurements.auth_procedure);
    println!("  P2IDF note:       {}", p2idf_cycles);

    chain.add_pending_executed_transaction(&executed_forward)?;
    chain.prove_next_block()?;

    // PTA vault must be empty after forwarding (pass-through invariant).
    let pta_state = chain.committed_account(pta_account.id())?;
    assert!(
        pta_state.vault().is_empty(),
        "PTA vault must be empty after the forwarding transaction"
    );

    // Inspect the outbound note emitted by the PTA.
    let output_notes = executed_forward.output_notes();
    assert_eq!(
        output_notes.num_notes(),
        1,
        "PTA transaction must emit exactly one output note (the P2ID to Bob)"
    );
    let outbound = output_notes.iter().next().expect("one output note").clone();
    assert_eq!(
        outbound.metadata().sender(),
        pta_account.id(),
        "outbound note sender must be the PTA, not Alice"
    );

    // The outbound note returned by the executor must be bit-identical to the
    // outbound P2ID we precomputed alongside the P2IDF. That equality is what
    // lets Bob consume it directly without re-deriving anything from chain.
    assert_eq!(
        outbound.id(),
        outbound_p2id.id(),
        "executor's outbound P2ID note id must match the precomputed pair.outbound"
    );

    // Bob redeems the outbound P2ID. This is the leg that completes the hop;
    // without it the asset would remain stranded in the outbound note.
    // The outbound P2ID note is private, so the MockChain only retains its
    // header and can't synthesize an authenticated input. Pass the full
    // precomputed note via the unauthenticated_notes slot.
    let bob_redeem = chain
        .build_tx_context(bob.id(), &[], &[outbound_p2id.clone()])?
        .build()?
        .execute()
        .await?;
    chain.add_pending_executed_transaction(&bob_redeem)?;
    chain.prove_next_block()?;

    let bob_state = chain.committed_account(bob.id())?;
    assert_eq!(
        bob_state
            .vault()
            .get_balance(faucet.id())
            .expect("bob should hold the forwarded asset"),
        ASSET_AMOUNT,
        "bob's vault must hold the forwarded amount after redeeming"
    );

    Ok(())
}

#[tokio::test]
async fn alice_sends_multiple_assets_to_bob_via_pta() -> Result<()> {
    let mut builder = MockChain::builder();

    let bob = builder.add_existing_wallet(Auth::IncrNonce)?;

    let pta_account = PassThroughAccount::build_existing([7u8; 32])?;
    builder.add_account(pta_account.clone())?;

    // Two distinct faucets so we can carry two assets in the same note
    // (NoteAssets rejects duplicate fungible assets from the same faucet).
    let faucet_a = builder.create_new_faucet(Auth::IncrNonce, "FOO", 1_000_000)?;
    let faucet_b = builder.create_new_faucet(Auth::IncrNonce, "BAR", 1_000_000)?;
    let asset_a: Asset = FungibleAsset::new(faucet_a.id(), ASSET_AMOUNT)?.into();
    let asset_b: Asset = FungibleAsset::new(faucet_b.id(), ASSET_AMOUNT * 2)?.into();

    let alice = builder.add_existing_wallet_with_assets(Auth::IncrNonce, [asset_a, asset_b])?;

    let mut rng = RandomCoin::new(Word::default());
    let pair = P2idForwardNote::create(
        alice.id(),
        pta_account.id(),
        bob.id(),
        vec![asset_a, asset_b],
        NoteAttachment::default(),
        &mut rng,
    )?;
    let p2idf = pair.inbound;
    let outbound_p2id = pair.outbound;

    builder.add_output_note(RawOutputNote::Full(p2idf.clone()));

    let mut chain = builder.build()?;

    let executed_forward = chain
        .build_tx_context(pta_account.id(), &[p2idf.id()], &[])?
        .build()?
        .execute()
        .await?;

    chain.add_pending_executed_transaction(&executed_forward)?;
    chain.prove_next_block()?;

    let pta_state = chain.committed_account(pta_account.id())?;
    assert!(
        pta_state.vault().is_empty(),
        "PTA vault must be empty after forwarding multiple assets"
    );

    let output_notes = executed_forward.output_notes();
    assert_eq!(
        output_notes.num_notes(),
        1,
        "PTA must emit exactly one outbound note even when forwarding multiple assets"
    );
    let outbound = output_notes.iter().next().expect("one output note").clone();
    assert_eq!(
        outbound.metadata().sender(),
        pta_account.id(),
        "outbound note sender must be the PTA"
    );
    assert_eq!(
        outbound.assets().num_assets(),
        2,
        "outbound note must carry both forwarded assets"
    );
    assert_eq!(
        outbound.id(),
        outbound_p2id.id(),
        "executor's outbound P2ID note id must match the precomputed pair.outbound \
         even with multiple assets"
    );

    // Bob redeems the multi-asset outbound P2ID and ends up with both assets
    // in his vault.
    // The outbound P2ID note is private, so the MockChain only retains its
    // header and can't synthesize an authenticated input. Pass the full
    // precomputed note via the unauthenticated_notes slot.
    let bob_redeem = chain
        .build_tx_context(bob.id(), &[], &[outbound_p2id.clone()])?
        .build()?
        .execute()
        .await?;
    chain.add_pending_executed_transaction(&bob_redeem)?;
    chain.prove_next_block()?;

    let bob_state = chain.committed_account(bob.id())?;
    assert_eq!(
        bob_state
            .vault()
            .get_balance(faucet_a.id())
            .expect("bob should hold faucet_a's asset"),
        ASSET_AMOUNT,
        "bob's vault must hold faucet_a's amount after redeeming"
    );
    assert_eq!(
        bob_state
            .vault()
            .get_balance(faucet_b.id())
            .expect("bob should hold faucet_b's asset"),
        ASSET_AMOUNT * 2,
        "bob's vault must hold faucet_b's amount after redeeming"
    );

    Ok(())
}

/// The P2IDF and P2ID scripts are loadable without panicking.
#[test]
fn p2idf_script_is_loadable() {
    let _ = P2idForwardNote::script();
    let _ = P2idForwardNote::script_root();
    let _ = P2idNote::script_root();
}
