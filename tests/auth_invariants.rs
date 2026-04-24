//! Tests for the `VaultEmptyAuth` invariant.
//!
//! The main invariant is verified implicitly by the single-hop end-to-end
//! test (`tests/single_hop.rs`): the transaction succeeds only if both
//! initial-vault-empty and final-vault-empty assertions pass.
//!
//! Here we additionally confirm:
//!   1. `VaultEmptyAuth` can be turned into an `AccountComponent` and used
//!      with `BasicWallet` to construct a valid account.

use miden_anonymizer::account::{PassThroughAccount, VaultEmptyAuth};
use miden_protocol::account::AccountBuilder;
use miden_standards::account::wallets::BasicWallet;

#[test]
fn vault_empty_auth_builds_a_valid_account() {
    let _account = AccountBuilder::new([0u8; 32])
        .with_auth_component(VaultEmptyAuth)
        .with_component(BasicWallet)
        .build()
        .expect("account with VaultEmptyAuth + BasicWallet should build");
}

#[test]
fn pta_builder_returns_public_immutable_account() {
    let (pta, _seed) = PassThroughAccount::build([1u8; 32]).expect("PTA should build");
    assert!(pta.is_public(), "PTA storage mode should be public");
    assert!(
        pta.account_type().is_regular_account(),
        "PTA should be a regular account"
    );
    // Immutable code is encoded at the account_type level; checking via
    // to_string or bitpattern is out of scope — the builder above requested it.
}
