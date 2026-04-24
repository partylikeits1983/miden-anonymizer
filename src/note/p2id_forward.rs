use alloc::vec;
use alloc::vec::Vec;

use miden_protocol::account::AccountId;
use miden_protocol::asset::Asset;
use miden_protocol::crypto::rand::FeltRng;
use miden_protocol::errors::NoteError;
use miden_protocol::note::{
    Note,
    NoteAssets,
    NoteAttachment,
    NoteMetadata,
    NoteRecipient,
    NoteScript,
    NoteStorage,
    NoteTag,
    NoteType,
};
use miden_protocol::utils::sync::LazyLock;
use miden_protocol::{Felt, Word};
use miden_standards::note::P2idNoteStorage;

use crate::library::pta_standards_lib;

// NOTE SCRIPT
// ================================================================================================

static P2IDF_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    // Use `from_library` (not `from_library_reference`) so the note script's
    // MAST forest contains the full library. `from_library_reference`
    // produces a minimal external-node stub, relying on the runtime to
    // resolve the procedure via the `MastForestStore` — but the PTA's
    // library is not among the forests the transaction executor pre-loads,
    // so resolution would fail at execution time.
    let lib = pta_standards_lib();
    NoteScript::from_library(&lib)
        .expect("PTA standards library has exactly one @note_script procedure")
});

// P2IDF NOTE
// ================================================================================================

/// Pay-to-ID-Forward note: addressed to a PTA, carries a precomputed P2ID
/// recipient digest for the final recipient (Bob) in its storage. When
/// consumed by the PTA, the note script:
/// - drains the asset into the PTA's vault,
/// - creates a new output note using the precomputed recipient (a standard
///   P2ID note to Bob), and
/// - moves the asset from the PTA vault into that outbound note.
///
/// Because the recipient digest is the hash of `(serial_num, script, storage)`,
/// storing only the digest means Bob's account id does not appear in plaintext
/// anywhere in the P2IDF note's storage.
pub struct P2idForwardNote;

impl P2idForwardNote {
    /// Number of felts in P2IDF note storage.
    pub const NUM_STORAGE_ITEMS: usize = P2idForwardNoteStorage::NUM_ITEMS;

    /// Returns the compiled [`NoteScript`] for the P2IDF note.
    pub fn script() -> NoteScript {
        P2IDF_SCRIPT.clone()
    }

    /// Returns the root of the P2IDF note script.
    pub fn script_root() -> Word {
        P2IDF_SCRIPT.root()
    }

    /// Builds a P2IDF note.
    ///
    /// - `alice` is the sender of the P2IDF note (recorded in its metadata).
    /// - `pta` is the pass-through account that will consume this note.
    /// - `bob` is the ultimate recipient of the forwarded payment.
    /// - `asset` is the single asset being forwarded.
    /// - `rng` draws two independent serial numbers — one for the inbound
    ///   P2IDF note and one for the outbound P2ID-to-Bob note. Both are
    ///   committed to by hashes, so an observer cannot link Alice to Bob via
    ///   the serials alone.
    pub fn create<R: FeltRng>(
        alice: AccountId,
        pta: AccountId,
        bob: AccountId,
        asset: Asset,
        attachment: NoteAttachment,
        rng: &mut R,
    ) -> Result<Note, NoteError> {
        let inbound_serial = rng.draw_word();
        let outbound_serial = rng.draw_word();
        let outbound_note_type = NoteType::Private;
        let outbound_tag = NoteTag::with_account_target(bob);

        // Precompute the outbound P2ID recipient so the PTA can emit the note
        // without executing any `miden-standards` procedure itself.
        let outbound_recipient = P2idNoteStorage::new(bob).into_recipient(outbound_serial);
        let outbound_recipient_digest = outbound_recipient.digest();

        let storage = P2idForwardNoteStorage {
            outbound_recipient_digest,
            outbound_note_type,
            outbound_tag,
        };

        let recipient = NoteRecipient::new(inbound_serial, Self::script(), storage.into());

        // The inbound note is always private in v1; tag it at the PTA's
        // account bucket so a PTA operator can discover it.
        let metadata = NoteMetadata::new(alice, NoteType::Private)
            .with_tag(NoteTag::with_account_target(pta))
            .with_attachment(attachment);
        let vault = NoteAssets::new(vec![asset])?;

        Ok(Note::new(vault, metadata, recipient))
    }
}

// P2IDF NOTE STORAGE
// ================================================================================================

/// Canonical storage layout for a [`P2idForwardNote`].
///
/// The MASM script expects the following 6-felt layout (must match
/// `masm/standards/notes/p2id_forward.masm`):
///
/// ```text
///   [0..3] outbound_p2id_recipient_digest (4 felts)
///   [4] outbound_note_type  (1 = public, 2 = private; v1 uses 2)
///   [5] outbound_note_tag
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2idForwardNoteStorage {
    pub outbound_recipient_digest: Word,
    pub outbound_note_type: NoteType,
    pub outbound_tag: NoteTag,
}

impl P2idForwardNoteStorage {
    pub const NUM_ITEMS: usize = 6;

    /// Consumes the storage and returns the corresponding [`NoteRecipient`].
    pub fn into_recipient(self, inbound_serial: Word) -> NoteRecipient {
        NoteRecipient::new(inbound_serial, P2idForwardNote::script(), NoteStorage::from(self))
    }
}

impl From<P2idForwardNoteStorage> for NoteStorage {
    fn from(s: P2idForwardNoteStorage) -> Self {
        let mut items: Vec<Felt> = Vec::with_capacity(P2idForwardNoteStorage::NUM_ITEMS);
        items.extend_from_slice(s.outbound_recipient_digest.as_elements());
        items.push(Felt::new(s.outbound_note_type as u64));
        items.push(Felt::new(u32::from(s.outbound_tag) as u64));
        NoteStorage::new(items).expect("P2IDF storage fits within NoteStorage limits")
    }
}

impl TryFrom<&[Felt]> for P2idForwardNoteStorage {
    type Error = NoteError;

    fn try_from(items: &[Felt]) -> Result<Self, Self::Error> {
        if items.len() != Self::NUM_ITEMS {
            return Err(NoteError::InvalidNoteStorageLength {
                expected: Self::NUM_ITEMS,
                actual: items.len(),
            });
        }
        let outbound_recipient_digest = Word::new([items[0], items[1], items[2], items[3]]);
        let outbound_note_type = match items[4].as_canonical_u64() {
            1 => NoteType::Public,
            2 => NoteType::Private,
            _ => {
                return Err(NoteError::other("invalid outbound note type in P2IDF storage"));
            },
        };
        let outbound_tag = NoteTag::from(items[5].as_canonical_u64() as u32);
        Ok(Self { outbound_recipient_digest, outbound_note_type, outbound_tag })
    }
}
