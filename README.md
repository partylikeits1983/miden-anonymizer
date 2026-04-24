# miden-anonymizer

A standalone Rust crate implementing a **Pass-Through Account (PTA)** on
Miden - a public, sign-nothing forwarding account that breaks the on-chain
linkage between the sender and receiver of a private payment.

## Why this is needed

Even on Miden - where accounts can be private and notes can be private - a
direct payment from Alice to Bob still leaves an observable trail:

- The note Alice creates names **Alice** as its sender in the on-chain header.
- The note's tag is derived from **Bob**'s account ID so Bob's client can find
  it.
- When Bob consumes the note, his transaction publishes its nullifier in the
  same time window.

Stitch those three signals together and an observer can reasonably infer
"Alice paid Bob" - even though both accounts are private and the note's
contents are hidden.

The PTA fixes this by inserting a public, immutable-code, sign-nothing
forwarding account between the two parties. Alice's transaction sends to the
PTA. The PTA's transaction sends to Bob. Each leg is its own observable flow,
and nothing on-chain links them.

## Flow

```mermaid
flowchart LR
    A([Alice]) -- "Tx1: create" --> N1[/"P2IDF note<br/>sender: Alice<br/>target: PTA<br/>hides: P2ID-to-Bob digest"/]
    N1 -- "Tx2: consume" --> P([PTA])
    P -- "Tx2: emit" --> N2[/"P2ID note<br/>sender: PTA<br/>target: Bob"/]
    N2 -- "Tx3: consume" --> B([Bob])
```

- **Tx1** is signed by Alice. It drains Alice's vault into the P2IDF
  ("P2ID-Forward") note. The note is private and stores the *precomputed P2ID
  recipient digest* for Bob - Bob's account ID is committed inside that hash,
  never in plaintext.
- **Tx2** is the PTA's. The note script drains Alice's asset into the PTA's
  vault, rebuilds the outbound P2ID note from the precomputed recipient, and
  immediately moves the asset back out into that note. The PTA's
  `VaultEmptyAuth` component asserts the vault root is identical at the start
  and end of the transaction; combined with the PTA being created with an
  empty vault, this proves the PTA never retains anything. No signature is
  required.
- **Tx3** is signed by Bob whenever he chooses to redeem. The outbound note's
  `sender` field reads `PTA`, never `Alice`.

Anyone can submit Tx2 against the PTA. Its code is immutable, its storage is
public, and its auth is just a deterministic invariant - there are no keys to
share.

## v1 known limitations

- **Same-block contention**: two users submitting against the PTA in the same
  block will race; one succeeds, the other must re-prove next block.
- **Amount correlation**: inbound amount equals outbound amount. Mitigated by
  a denomination whitelist in v2.
- **Timing correlation**: inbound and outbound happen in the same block.
  Mitigated by a delayed-outbound mechanism in v2.
- **Anonymity set** = senders routing through the PTA in the same time
  window. Small by default; grows with adoption.

## Deployed public PTA (Miden testnet)

A public, immutable-code PTA is live on Miden testnet:

- **bech32**: `mtst1azy607tkxe7fyqq604l2ysp55qqs2whr`
- **explorer**: <https://testnet.midenscan.com/account/mtst1azy607tkxe7fyqq604l2ysp55qqs2whr>
- **deploy tx** (the first P2IDF forward - that's how the PTA got committed
  to chain): <https://testnet.midenscan.com/tx/0xdead6a5083f94f63542fd3ca432579bffbb650c98afe53507747c78778de7c83>

Because the PTA's auth requires no signature, *any* fresh client can submit a
transaction against it - no shared keys, no shared store, nothing
out-of-band beyond the public bech32 above. See `src/bin/use_pta.rs` for a
working example, and `tests/testnet.rs::any_client_can_use_public_pta` for
the same flow as a `#[ignore]`d integration test.

## Build & run

```
# library + MockChain tests
cargo build
cargo test

# drive a P2IDF forward against the already-deployed public PTA above
cargo run --release --features cli --bin use_pta

# or, against a different PTA:
cargo run --release --features cli --bin use_pta -- <pta-bech32>

# deploy a fresh PTA on testnet (prints its bech32 + midenscan link)
cargo run --release --features cli --bin deploy_pta
```

The CLI binaries pull in `miden-client`; they're gated behind the `cli`
feature so library consumers that only want the PTA primitives don't pay the
cost of the full client stack.
