//! Embedded compiled PTA MASM libraries. The `.masl` files are produced by
//! `build.rs` and placed under `$OUT_DIR/assets`.

use miden_protocol::assembly::Library;
use miden_protocol::utils::serde::Deserializable;
use miden_protocol::utils::sync::LazyLock;

// PTA AUTH COMPONENT LIBRARY
// ================================================================================================

static PTA_AUTH_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/account_components/auth/vault_empty.masl"
    ));
    Library::read_from_bytes(bytes).expect("compiled PTA auth library should be well-formed")
});

// PTA STANDARDS LIBRARY (P2IDF, etc.)
// ================================================================================================

static PTA_STANDARDS_LIB: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/standards.masl"));
    Library::read_from_bytes(bytes).expect("compiled PTA standards library should be well-formed")
});

/// Returns the compiled `VaultEmptyAuth` component library.
pub fn pta_auth_library() -> Library {
    PTA_AUTH_LIBRARY.clone()
}

/// Returns the compiled PTA standards library (contains the P2IDF note script).
pub fn pta_standards_lib() -> Library {
    PTA_STANDARDS_LIB.clone()
}
