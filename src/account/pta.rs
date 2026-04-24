use miden_protocol::Word;
use miden_protocol::account::{Account, AccountBuilder, AccountStorageMode, AccountType};
use miden_standards::account::wallets::BasicWallet;

use crate::account::VaultEmptyAuth;
use crate::errors::PtaError;

/// Builder helper for the pass-through account (PTA).
///
/// The PTA is:
/// - **Regular, immutable-code** — users need assurance that the PTA's logic
///   can never be upgraded to steal in-flight assets.
/// - **Public storage** — state is on-chain so any user can construct a local
///   transaction consuming P2IDF notes addressed at the PTA.
/// - **Components**: `VaultEmptyAuth` (enforces the pass-through invariant)
///   plus `BasicWallet` (provides `receive_asset` / `move_asset_to_note`,
///   which the P2IDF note script calls).
pub struct PassThroughAccount;

impl PassThroughAccount {
    /// Builds a new PTA [`Account`], returning the account and its creation seed.
    pub fn build(init_seed: [u8; 32]) -> Result<(Account, Word), PtaError> {
        let account = AccountBuilder::new(init_seed)
            .account_type(AccountType::RegularAccountImmutableCode)
            .storage_mode(AccountStorageMode::Public)
            .with_auth_component(VaultEmptyAuth)
            .with_component(BasicWallet)
            .build()
            .map_err(PtaError::AccountBuild)?;
        let seed = account.seed().unwrap_or_default();
        Ok((account, seed))
    }

    /// Builds a PTA as an **existing** account (seed stripped), suitable for
    /// being added to a chain's genesis state via
    /// `MockChainBuilder::add_account`.
    ///
    /// Requires the `testing` feature (and is primarily intended for tests).
    #[cfg(any(feature = "testing", test))]
    pub fn build_existing(init_seed: [u8; 32]) -> Result<Account, PtaError> {
        AccountBuilder::new(init_seed)
            .account_type(AccountType::RegularAccountImmutableCode)
            .storage_mode(AccountStorageMode::Public)
            .with_auth_component(VaultEmptyAuth)
            .with_component(BasicWallet)
            .build_existing()
            .map_err(PtaError::AccountBuild)
    }
}
