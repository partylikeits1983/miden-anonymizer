use alloc::vec;

use miden_protocol::account::component::AccountComponentMetadata;
use miden_protocol::account::{AccountComponent, AccountType};

use crate::library::pta_auth_library;

/// An [`AccountComponent`] providing the pass-through invariant as authentication.
///
/// The exported auth procedure (`auth_vault_empty`) asserts:
/// - the account's vault is empty at the **start** of the transaction
///   (prevents pre-funding grief / asset siphoning), and
/// - the account's vault is empty at the **end** of the transaction
///   (the PTA is strictly pass-through, it must not retain assets).
///
/// It also increments the nonce when state changes or on new-account creation,
/// mirroring the `NoAuth` component from `miden-standards`.
///
/// Intended for `AccountType::RegularAccountImmutableCode`.
pub struct VaultEmptyAuth;

impl VaultEmptyAuth {
    /// The component name as used in metadata.
    pub const NAME: &'static str = "miden::pta::auth::vault_empty";

    pub fn new() -> Self {
        Self
    }

    /// Returns the [`AccountComponentMetadata`] for this component.
    pub fn component_metadata() -> AccountComponentMetadata {
        AccountComponentMetadata::new(Self::NAME, AccountType::all())
            .with_description("Pass-through auth: vault must be empty before and after tx")
    }
}

impl Default for VaultEmptyAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl From<VaultEmptyAuth> for AccountComponent {
    fn from(_: VaultEmptyAuth) -> Self {
        AccountComponent::new(
            pta_auth_library(),
            vec![],
            VaultEmptyAuth::component_metadata(),
        )
        .expect(
            "VaultEmptyAuth component should satisfy the requirements of a valid account \
                 component",
        )
    }
}
