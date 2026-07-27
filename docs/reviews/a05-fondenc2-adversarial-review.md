# A0.5 FONDENC2 adversarial cryptographic review

**Review status:** Model-based advisory review  
**Reviewer:** GPT-5.6-Sol  
**Date:** 2026-07-27  
**Issue:** [#120](https://github.com/kafkade/fond/issues/120)  
**Implementation gate verdict:** **NO-GO**

> **Honesty caveat:** This is a model-based adversarial review, not an
> independent human cryptographic review. It is advisory. It does **not**
> satisfy issue #120's "external reviewer engaged" requirement, does **not**
> constitute cryptographic proof, does **not** record human sign-off, and does
> **not** clear the gate for Epic A, C, or D implementation. Every
> cryptographic-strength judgment below is an assessment. The useful output is
> a decided baseline, concrete attack analysis, initial illustrative vectors,
> and a narrowed checklist for a human cryptographer.

## Overall gate verdict

**NO-GO for implementation.** The four appendices have a sound high-level
direction: separate password stretching from random-key derivation, bind object
headers as AEAD associated data, use per-member wraps, keep merge client-side,
and separate service authentication from vault authorization. The selected
primitive families are conventional.

They do not yet compose into one complete, versioned protocol.

### Prioritized blockers before A1 code

1. **Repair epoch-key distribution and recovery** (`K.12`, `K.16`, `I.2`,
   N-01, N-02, N-10). Unblock by specifying a non-circular wrap directory,
   recoverable old-epoch key archive, new-member history policy, identity-key
   recovery, and one signed roster/manifest transition.
2. **Replace the linear manifest with an honest-concurrency protocol** (`I.3`,
   `I.6`, N-03, N-12). Unblock by pinning either authenticated CAS plus signed
   merge records or a multi-parent authenticated DAG, including immutable
   ciphertext addressing and an authenticated head pointer.
3. **Bind history to the authorization state that created it** (`K.7`, `K.8`,
   `I.4`, `I.14`, N-04, N-05). Unblock by certifying per-device keys, binding
   each record to a historical roster hash, and replacing scalar `own_counter`
   with a per-object commitment or authenticated device operation log.
4. **Make bootstrap, invitation, and account reset client-verifiable** (`K.3`,
   `K.10`, `I.12`, `I.13`, N-07, N-08, N-09). Unblock by authenticating invite
   keys and initial head state out of band, then chaining every identity change
   from client-held roster/trust state rather than a server-only invariant.
5. **Close pre-auth and downgrade paths** (`K.13`, `K.14`, `I.10`, `I.11`,
   `I.15`, `VR-020-K13.1` through `VR-020-K13.5`, N-06, N-11). Unblock by
   capping legacy FONDENC1 parameters before Argon2, pinning measured local
   profile acceptance, selecting one reviewed OPAQUE configuration, and
   removing SRP fallback.

### Non-blocking improvements

- Keep K.9 lazy old-epoch re-encryption optional and accurately best-effort.
- Do not add a per-DEK nonce counter; K.5's random XChaCha nonce is sufficient.
- Treat SSD/CoW legacy deletion as best-effort while retaining K.15's safe
  default.
- Defer traffic padding and finer-grained subchains until the core protocol is
  correct; they harden metadata/performance but do not repair the blockers.

The validation backlog resolves to **15 ACCEPT-as-specified, 18
REJECT-must-change, 18 NEEDS-DECISION, and 4 DEFER-TO-HUMAN**. Wrap/unwrap and
full manifest-verification vectors cannot be canonical until the rejected and
undecided constructions are repaired. Consequently issue #120 acceptance
criteria 1 through 4 all remain open.

## Scope, method, and reviewed material

The review covered:

- the FONDENC2 key hierarchy, object envelope, membership, invitation,
  revocation, and FONDENC1 migration;
- authenticated causal history, object identifiers, version vectors,
  manifests, checkpoints, rollback/fork detection, and tombstones;
- KDF profiles, profile migration, epoch rotation, and cross-object
  atomicity;
- OPAQUE selection, ciphersuite, KSF binding, account reset, identity binding,
  vault authorization, and the SRP fallback;
- the current FONDENC1 implementation, especially `open_bundle`,
  `seal_blob`, and `open_blob`.

**Provenance note: A0.4 read from uncommitted worktree.** The requested,
transient `kafkade-fuzzy-waddle` worktree carried the not-yet-merged A0.4
protocol text; the review worktree and `origin/main` were still at
`50365a248b7c05d0cacf82c98fc87c08b16c1b6d`. The reviewed sources were:

| Source | Revision / SHA-256 |
|---|---|
| `kafkade-fuzzy-waddle/docs/adr/020-zero-knowledge-identity.md` | worktree `HEAD` `98bde72f612e9b51ea762571a6919155a1cc0928`; `bd6e5f416efc07103afff388ca78865bb99d8d1b7780a7760477e82f8b202210` |
| `kafkade-fuzzy-waddle/docs/adr/021-optional-sync-server.md` | worktree `HEAD` `98bde72f612e9b51ea762571a6919155a1cc0928`; `8506690433c3b3e0774f8d035f89d794c480c19d16f55542206b95982ac144c2` |
| `origin/main:crates/fond-store/src/crypto.rs` | `origin/main` `50365a248b7c05d0cacf82c98fc87c08b16c1b6d` |

Method:

1. Inventory every literal `[Validation Required]` occurrence, including
   wrapped and repeated tags.
2. Trace key ownership, persistence, recovery, and rotation across both ADRs.
3. Treat the sync server as malicious, not merely buggy, and attempt replay,
   rollback, fork, equivocation, key substitution, downgrade, and metadata
   attacks.
4. Walk crash boundaries for migration, profile upgrade, and epoch rotation.
5. Check whether every signed or hashed value has an unambiguous transcript,
   trust anchor, and historical authorization state.
6. Check PAKE claims against RFC 9807, current crate evidence, and public audit
   provenance.
7. Generate deterministic outputs only where the current text fixes all bytes
   needed by the primitive invocation.

ADR-022, ADR-023, and A4 implementation were not reviewed as protocols. Where
the reviewed text depends on them, that dependency is recorded rather than
assumed away.

## Per-scope-area adversarial findings

### Key hierarchy

**What holds.** Separating one Argon2id unlock from fast random-key derivation
is the right construction class. The 54-byte object header includes magic,
version, class, epoch, object ID, and nonce as XChaCha20-Poly1305 AAD. With a
unique DEK and sound CSPRNG, random 192-bit nonces have ample collision margin.
The server cannot decrypt an object from ciphertext, a wrap, and an OPAQUE
record without member-held secret material.

**What breaks.** `VK_e` is random and old objects remain encrypted under it, but
the protocol defines no epoch-key archive. Keeping only `VK_{e+1}` loses old
data; keeping historical roster wraps means a passphrase or Secret Key change
must re-wrap every historical Vault Key, not one key. A new member's historical
access policy is also undefined.

The roster creates a second blocker. It is a FONDENC2 object at the new epoch,
yet its plaintext contains each member's wrap of the key needed to decrypt that
object. The wrap directory must be outside the `VK_{e+1}` encryption boundary,
or be encrypted under an already-held transition key.

The spec also calls `seal_blob` / `open_blob` "already-audited" and reusable,
but the current blob sub-header contains key mode and free-form Argon2
parameters. FONDENC2 objects expressly must not. Only the underlying AEAD call
is reusable; the existing envelope parser is not.

### Household membership

**What holds.** Per-member wrapping and cryptographic roles are appropriate for
a server that cannot enforce plaintext authorization. Forward-only revocation
is described honestly: a removed member cannot be made to forget old keys or
plaintext.

**What breaks.** Invitation public keys may be published "via the server".
A malicious server can replace the invitee key and receive the Vault Key unless
the invitation is bound to an authenticated fingerprint, QR code, or existing
member-authorized transcript. Selecting HPKE versus a libsodium sealed box does
not solve recipient authentication.

The current same-member device enrollment transports only the Secret Key, but
the random Ed25519/X25519 private identity keys have no derivation, wrapped
backup, or recovery path. A restored device can derive the MUK yet cannot prove
the existing member identity. This also makes "Emergency Kit contains only the
Secret Key" insufficient as written.

Per-device signing keys are required, not optional, if `device_id` is a causal
actor and device revocation is expected. Each device key needs a certificate or
roster authorization rooted in the member/admin identity, and every historical
record must identify the roster state that authorized that device.

### Causal history and anti-rollback

**What holds.** Version vectors are preferable to server-controlled clocks.
Binding vectors and blob hashes into signed records can make rewriting
detectable after a client has a trusted head. The text correctly admits that a
local watermark cannot prevent withholding or detect forever-partitioned forks.

**What breaks.** A linear `prev_manifest_hash` chain assumes one writer.
Devices A and B can both read head 10, independently create valid record 11,
and sign different hashes. The verifier calls this equivocation even when both
writes are honest. Per-object vectors inside the records do not merge the two
manifest heads. The protocol needs a defined append/CAS and signed merge
operation, a multi-parent authenticated DAG, or an explicitly trusted
sequencer.

Manifest records are encrypted, so the server cannot see `manifest_seq`,
`prev_manifest_hash`, author, or signature to provide even untrusted CAS or
immutable append semantics. The external storage key, versioned record
addressing, and authenticated head-pointer protocol are missing.

Checking every historical signature against a "current-roster member" makes a
valid old chain unverifiable after revocation. Checking only the current roster
also fails to prove that the signer was authorized when the record was made.
Each record must bind a roster hash/sequence, and verification must replay the
historical roster authorization.

`trusted_state.own_counter` is insufficient. If counters are global per device,
an object last written at counter 3 remains valid after another object advances
the device to 10, but the proposed `VV[own] >= own_counter` check rejects it. If
counters are per object, one scalar cannot detect omissions. Durable state must
commit to the device's own per-object writes or to an authenticated operation
log.

A new device has no trusted manifest head. A server can provide any old,
self-consistent signed checkpoint. Device bootstrap therefore needs a head hash
or checkpoint commitment carried by an authenticated invitation, another
device, or a roster transition. This first-sync rollback window is not stated.

### KDF profiles and rotation

**What holds.** Replacing free-form costs with an append-only profile registry
bounds attacker-selected work. Sixteen random salt bytes are sufficient.
Keeping deprecated profiles readable while refusing them for new writes is the
right compatibility direction.

**What breaks.** A profile ID cannot be authenticated until the client derives
the candidate key and checks the wrap tag. An attacker can substitute any
valid, expensive profile ID and force its bounded cost before failure. A
registry-wide 1 GiB ceiling is therefore not a safe pre-auth ceiling for every
client. The expected member profile must be locally pinned where possible, and
platform-specific accepted-cost bounds and attempt throttling are still
required.

Withdrawing a profile outright destroys availability for the only remaining
copy of a wrap. Profile upgrade must be explicit or clearly announced,
transactional, crash-safe, and preserve a recovery path. It must not silently
mutate vault state during unlock.

The epoch-invariant object-ID namespace cannot derive from the random
per-epoch Vault Key. Use a separately generated vault-lifetime namespace key,
distributed under membership controls and explicitly accepted as retained
metadata capability after revocation.

Roster and manifest transitions need one authenticated state-machine commit.
"Publish the roster, then write a manifest" exposes crash states and does not
define which epoch can authorize writes during the transition.

### PAKE and vault authorization

**What holds.** OPAQUE is the appropriate primary PAKE family. A registration
record without the OPRF seed is not a conventional offline verifier. Even if a
full server compromise recovers a passphrase, the never-uploaded Secret Key can
still prevent MUK derivation, assuming its lifecycle is fixed. OPAQUE-3DH with
the RFC 9807 ristretto255/SHA-512 profile is a defensible baseline.

**What breaks or needs qualification.**
[opaque-ke 4.0.1](https://github.com/facebook/opaque-ke/releases/tag/v4.0.1)
is current stable and tests RFC 9807 vectors, but its public
[NCC Group review](https://web.archive.org/web/20211213145520id_/https://research.nccgroup.com/wp-content/uploads/2021/12/NCC_Group_WhatsAppLLC_OPAQUE_Report_2021-12-10_v1.3.pdf)
covered v0.5.0 and an earlier draft in 2021, not v4.0.1. The standalone
`voprf 0.5.0` code did not exist in that form during the review. RFC-vector
tests are valuable conformance evidence, not an audit of the current release.

[RFC 9807 section 10.11](https://www.rfc-editor.org/rfc/rfc9807.html#section-10.11)
states that a corrupted single server with its OPRF secret can run an exhaustive
offline dictionary attack; the configured KSF prices each guess. The ADR's
"record dump" claim is acceptable only when it explicitly excludes the OPRF
seed and an OPRF oracle. Rate limits do not constrain a malicious operator.

OPAQUE `client_identity` authenticates an identity for one registration record;
`export_key` is likewise tied to that record. Fresh registration/password reset
normally creates a fresh record and export key. Neither is a reset-independent
immutability mechanism. A sidecar key commitment must be chained from the old
vault key or roster, and clients must verify that chain. A server-side
"non-resettable" column alone is not a security invariant against the server in
this threat model.

SRP must not be a negotiated fallback. RustCrypto's current
[SRP README](https://github.com/RustCrypto/PAKEs/blob/master/srp/README.md)
states that it has never received an independent third-party audit, and the
verifier permits offline guessing after theft. "OPAQUE unavailable" should make
sync unavailable, not silently downgrade authentication.

### FONDENC1 migration

**What holds.** Staging, `fsync`, atomic rename, retaining the legacy blob by
default, and leaving `.cook` files untouched are appropriate goals. Secure
deletion is correctly suspect on SSD and copy-on-write storage.

**What breaks.** FONDENC1 was designed to travel over untrusted file sync. A
blob in a local directory is not necessarily trusted. Migration calls
`open_bundle`, which parses attacker-supplied `m_cost`, `t_cost`, and `p_cost`
and constructs Argon2 parameters before authentication. The claim that
migration sees only the user's own local blob does not close the attack.
Migration needs an explicit safe legacy-parameter allowlist/cap before calling
the old opener.

The transaction also lacks a durable inventory binding source blob hash,
expected output IDs/hashes, roster hash, and migration version. "Both present,
verify/resume" is not executable without that record. Detection must not infer
completion merely from any FONDENC2 object or marker, and the parent directory
must be durably synchronized after rename and marker updates.

## New findings not present in the validation backlog

| ID | Severity | Spec clause attacked and finding | Required disposition |
|---|---|---|---|
| N-01 | **Critical** | ADR-020 FONDENC2 §H says rotation uses a fresh random `VK_{e+1}`, leaves existing objects under `VK_e`, and passphrase change re-wraps a member's "single" Vault Key. No recoverable epoch-key archive is defined, so a new device loses old data and a changed KEK cannot open historical wraps. | Specify epoch-key retention, new-member history policy, key-archive wrapping, and recovery before implementation. |
| N-02 | **Critical** | ADR-020 FONDENC2 §G says the roster is itself a FONDENC2 object containing `wrapped_vault_key[epoch]`; §H says the new roster is published at `e+1`. Decrypting that roster requires the same `VK_{e+1}` distributed inside it. | Put an authenticated wrap directory outside the new-key encryption boundary or define an old-key-authenticated transition envelope. |
| N-03 | **Critical** | ADR-021.1 §D defines one record with one `manifest_seq` and one `prev_manifest_hash`; §E calls same-sequence, different-hash records equivocation. Two honest writers extending one head create exactly that condition. | Define CAS plus signed merge, a multi-parent authenticated DAG, or a trusted single-writer design. |
| N-04 | **High** | ADR-021.1 §E check 1 requires every record signer to be a "current-roster member." That rejects valid history signed by later-revoked members and does not prove authorization at signing time. | Bind every manifest record to a roster hash/sequence and verify against that historical roster state. |
| N-05 | **High** | ADR-021.1 §E stores one `own_counter` but check 4 applies it to entries for objects the device last wrote. Global counters reject valid older objects; per-object counters require a map. | Persist a commitment to own per-object writes or use an authenticated per-device operation log. |
| N-06 | **High** | ADR-020 A0.3 "Open legacy" says FONDENC1 parameters are safe because the blob is the user's "own local" file, while ADR-019 designed FONDENC1 for untrusted file sync. Migration can therefore retain the pre-auth Argon2 exhaustion path. | Reject legacy parameters outside a small allowlist before invoking the old KDF. |
| N-07 | **High** | ADR-021.1 §D says a checkpoint lets a new device bootstrap without full replay, while §E's trusted watermark exists only after acceptance. A new device can accept an old signed checkpoint as genesis. | Carry an authenticated head/checkpoint commitment in enrollment or require peer corroboration before accepting bootstrap state. |
| N-08 | **High** | ADR-020 FONDENC2 §H allows the invitee's X25519 key to be published "out-of-band or via the server" before sealing the Vault Key. A malicious server can substitute its key. | Authenticate the invitee key fingerprint and sign the complete invitation transcript. |
| N-09 | **High** | ADR-021.2 §E says the server "MUST enforce" that password reset never rewrites the non-resettable identity attribute, but ADR-021's threat model includes a malicious/breached server. | Anchor identity continuity in client-held trusted state and the signed roster chain; server immutability may only be defense in depth. |
| N-10 | **High** | ADR-020 FONDENC2 §G generates random Ed25519/X25519 member identity keys, while §H's same-member device enrollment transports only the Secret Key and creates no roster entry. The private identity keys have no backup, derivation, or recovery path. | Define encrypted private-key backup or deterministic derivation and reconcile it with per-device keys and Emergency Kit claims. |
| N-11 | **Medium** | ADR-020 A0.3 says profile ID and raw parameters are AAD so tampering fails "before Argon2 runs." The client must first derive the candidate KEK to verify that tag, so any valid expensive ID remains attacker-selectable pre-auth work. | Add platform-specific accepted profiles, local expected-profile pinning, and bounded retry behavior. |
| N-12 | **High** | ADR-021.1 §D puts sequence, predecessor, author, and signature inside a manifest that it also says is sealed as a FONDENC2 object. No external immutable record ID/head protocol lets the server provide CAS or clients name the latest record. | Specify versioned ciphertext addressing and an authenticated head/append protocol together with N-03. |
| N-13 | **Medium** | ADR-021.1 §B says a 16-byte truncated HMAC retains "at least 128-bit collision resistance." It has 128-bit preimage/forgery strength but only 64-bit generic collision strength. | Correct the claim and either choose 32-byte IDs or specify collision detection/recovery for 16-byte IDs. |
| N-14 | **Medium** | ADR-020 FONDENC2 §H describes libsodium `crypto_box_seal` as "X25519 + XChaCha20-Poly1305." `crypto_box_seal` is based on `crypto_box`, not that stated AEAD construction. | Pin HPKE or accurately pin the exact libsodium suite and transcript; do not describe primitives by analogy. |

## Complete validation-required adjudication

Verdict meanings:

- **ACCEPT-as-specified:** the current direction is suitable for the reviewed
  threat model, subject to ordinary byte-level specification.
- **REJECT-must-change:** implementing the current text would create an
  incorrect, unsafe, or non-interoperable protocol.
- **NEEDS-DECISION:** the spec openly presents alternatives; the recommended
  choice below must be pinned before implementation.
- **DEFER-TO-HUMAN:** this model cannot responsibly close the item; a human
  cryptographer and/or measured device evidence is required.

| Verdict | Count |
|---|---:|
| ACCEPT-as-specified | 15 |
| REJECT-must-change | 18 |
| NEEDS-DECISION | 18 |
| DEFER-TO-HUMAN | 4 |
| **Total** | **55** |

### Literal-tag coverage

Line numbers below refer to the SHA-256-pinned sources listed above. The four
gate-reminder tags at ADR-020 lines 164 and 451 and ADR-021 lines 187 and 532
state process requirements rather than decision items; this review accepts
their gate and does not count them among the 55 adjudications.

- ADR-020 tag locations: 87, 164, 206, 220, 276, 301-302, 313, 321, 329,
  340-341, 355, 383, 385, 386, 389, 451, 494, 495, 586, and 649.
- ADR-021 tag locations: 187, 246, 250, 286, 288, 337, 338, 358, 387, 408,
  445, 487-488, 491, 495, 498, 501, 504-505, 510, 532, 612, 618, 623
  (twice), 624 (twice), 625, 631, 711, 727, 731, 738, 739, 740, 741, 742,
  743, 750, 752, 764, and 767.

Repeated tags map to their K/I row. Blanket tags for K.13 and I.11 are also
decomposed into `VR-*` rows so each placeholder figure or claim receives an
explicit verdict. The inline ADR-020 PAKE fallback receives `VR-020-D.1`.

### FONDENC2 and A0.3 items

| ID | Spec clause (tightly paraphrased) | Verdict | Reasoning and recommendation |
|---|---|---|---|
| K.1 | ADR-020 FONDENC2 §C: put the Secret Key in Argon2's `secret` slot or use an HKDF pre-mix. | **NEEDS-DECISION** | Prefer Argon2id's `secret` input for the fixed 32-byte Secret Key rather than an ad hoc HKDF pre-mix. Pin Argon2 version 0x13, the exact low-level API semantics, and a cross-implementation KAT; apply an explicit FONDENC2 MUK label before or within the secret input if the same Secret Key may serve another protocol. |
| K.2 | ADR-020 FONDENC2 §G: wrap each member's Vault Key with XChaCha20-Poly1305, AES-256-KW, or AES-SIV. | **NEEDS-DECISION** | Choose XChaCha20-Poly1305 with a random 24-byte nonce and exact wrap AAD. AES-KW has no AAD and deterministic equality leakage; AES-SIV changes key-size and library assumptions. The existing XChaCha dependency and tiny wraps make nonce misuse risk manageable. |
| K.3 | ADR-020 FONDENC2 §H: use libsodium `crypto_box_seal` or HPKE for the new-member invitation. | **NEEDS-DECISION** | Choose a fully pinned HPKE Base-mode suite, then add an owner/admin signature over vault ID, invite ID, recipient fingerprint, HPKE encapsulation, role, and expiry. HPKE alone does not authenticate the recipient key source. |
| K.4 | ADR-020 FONDENC2 §B/§F: derive KEK/subkeys/DEKs with HKDF-SHA-256 or keyed BLAKE3. | **NEEDS-DECISION** | Choose HKDF-SHA-256 for standards interoperability and available KATs. Explicitly define every Extract salt, including KEK derivation, and use fixed-width or length-prefixed transcript fields. |
| K.5 | ADR-020 FONDENC2 §E: use random 192-bit XChaCha nonces or add a per-DEK counter. | **ACCEPT-as-specified** | Keep random 192-bit nonces. A durable counter adds rollback/crash state and can make reuse more likely after state loss; XChaCha's random-nonce bound is already ample per DEK. Treat RNG failure as a platform-fatal error. |
| K.6 | ADR-020 FONDENC2 §F: choose per-recipe, per-overlay-row, or per-user-bucket sync objects. | **NEEDS-DECISION** | Use one object per independently mergeable logical record: recipe body, overlay row, user-scoped record, or photo. Avoid broad per-user buckets that amplify conflicts and rewriting, and document metadata/blob-count tradeoffs. |
| K.7 | ADR-020 FONDENC2 §H: decide whether devices get distinct signing/subkeys for per-device revocation. | **REJECT-must-change** | Per-member-only signing cannot support the stated `device_id`, version-vector ownership, or device revocation. Require a distinct device signing key certified by a member/admin identity and represented in historical roster state. |
| K.8 | ADR-020 FONDENC2 §G: choose a dedicated vault signer or per-admin/threshold keys and define owner transfer. | **NEEDS-DECISION** | Prefer per-admin keys authorized by the historical roster over one shared vault signing key. Specify owner transfer as a chained old-owner authorization plus new-owner acceptance; defer threshold signatures unless a concrete recovery policy requires them. |
| K.9 | ADR-020 FONDENC2 §H: optionally re-seal old-epoch objects lazily or in background after revocation. | **ACCEPT-as-specified** | Keep it optional and describe it as best-effort forward hardening. It cannot revoke plaintext already seen or force a malicious server to delete old ciphertext, but resealing touched objects helps against later access through an honest store. |
| K.10 | ADR-020 §K / ADR-021.2 §E: bind vault identity keys to OPAQUE so login reset cannot forge vault authority. | **NEEDS-DECISION** | The separation principle is correct, but ADR-021.2 does not finish it. Resolve through I.12-I.14 with client-verifiable key continuity rather than a server-only attribute. |
| K.11 | ADR-020 FONDENC2 §E/§F: choose keyed-HMAC or random object IDs and 16- or 32-byte width. | **NEEDS-DECISION** | Use keyed IDs over a canonical typed identity. Prefer 32 bytes; if 16 bytes is retained, correct the collision-strength claim and define collision recovery. This item cannot close before I.1/I.2 and A4. |
| K.12 | ADR-020 "Emergency Kit & recovery": carry Secret Key only and no MUK/Vault Key material. | **REJECT-must-change** | Secret Key only is insufficient while random member signing/transport private keys have no recovery path. It becomes acceptable only after those keys are deterministically recoverable or encrypted under material recoverable from passphrase plus Secret Key. Never print the Vault Key. |
| K.13 | ADR-020 A0.3 "Concrete starting profiles": validate Argon2 triples, salt width, budgets, and ceiling. | **DEFER-TO-HUMAN** | A human cryptographer must approve the security floor, and real target devices must supply p50/p95 time, peak memory, thermal, and concurrency measurements. Model judgment cannot validate the placeholder triples. |
| K.14 | ADR-020 A0.3 "Deprecation lifecycle": decide whether deprecated-profile unlock silently forces a re-wrap. | **REJECT-must-change** | Do not silently rewrite on unlock and do not make a sole old wrap unreadable merely by marking a profile withdrawn. Use an explicit or prominently announced transactional re-wrap, preserve rollback-safe recovery, and retain a read-only recovery path. |
| K.15 | ADR-020 A0.3 migration step 7: retain FONDENC1 by default or offer secure deletion on SSD/CoW storage. | **ACCEPT-as-specified** | Retaining FONDENC1 by default is safest until the user verifies the new vault and backup. "Delete" must be described as best effort on SSD/CoW storage, not guaranteed secure erase; cryptographic erasure is unavailable while the old passphrase/key may still exist. |
| K.16 | ADR-020 A0.3 rotation: keep the roster epoch and ADR-021.1 manifest `vault_epoch` atomic across crashes. | **REJECT-must-change** | Separate roster and manifest publication is not atomic and the new roster is currently key-circular. Define one signed transition object/state-machine commit that binds old roster hash, new roster hash, old/new epochs, manifest predecessor/head, and completion state. |
| VR-020-D.1 | ADR-020 "Binding to a server": prefer OPAQUE and retain SRP-6a only as a reviewed fallback. | **REJECT-must-change** | Keep OPAQUE primary but remove SRP fallback. A negotiated or availability-triggered fallback is a downgrade path, and the proposed SRP crate is explicitly unaudited. |
| VR-020-K13.1 | ADR-020 A0.3 profile table: desktop `PROFILE[1] = 262144/3/1`, output 32 bytes. | **DEFER-TO-HUMAN** | `262144/3/1` is a plausible benchmark anchor, not a validated profile. Approve only after human review and measurements on minimum supported desktop hardware. |
| VR-020-K13.2 | ADR-020 A0.3 profile table: mobile/watch `PROFILE[2] = 65536/3/1`, output 32 bytes. | **DEFER-TO-HUMAN** | `65536/3/1` may still be unsuitable for a watch or concurrent mobile workload. Separate phone and watch capabilities if measurements diverge; do not label one profile safe for both by assumption. |
| VR-020-K13.3 | ADR-020 A0.3 profile text: target roughly 1 s desktop, 1.5 s mobile/watch, with acceptable RAM. | **NEEDS-DECISION** | Adopt measured acceptance gates, not those nominal values: record p50/p95 time, peak RSS, thermal/battery behavior, and concurrent-unlock limits on each minimum device class, then require human approval of the resulting floor. |
| VR-020-K13.4 | ADR-020 A0.3 MUK parameters: choose a 16- or 32-byte random per-member Argon2 salt. | **ACCEPT-as-specified** | A uniformly random 16-byte Argon2 salt is sufficient. Widening to 32 bytes does not materially improve this household-scale construction. |
| VR-020-K13.5 | ADR-020 A0.3 profile text: pin a registry-wide memory ceiling, illustrated as about 1 GiB. | **REJECT-must-change** | A single approximately 1 GiB ceiling is too high as pre-auth work on constrained clients. Use per-platform accepted-profile sets and much lower local pre-auth ceilings; authenticated tags do not prevent a valid-ID substitution from consuming work first. |

### Authenticated causal history and PAKE items

| ID | Spec clause (tightly paraphrased) | Verdict | Reasoning and recommendation |
|---|---|---|---|
| I.1 | ADR-021.1 §B: choose exact `object_class`/UUID/sub-object input fields and 16- or 32-byte HMAC truncation. | **NEEDS-DECISION** | Define a fixed typed transcript including format label, object class, durable UUID, and a fixed-width subtype/part identifier where needed. Recommend a 32-byte HMAC output; no delimiter-free variable fields. |
| I.2 | ADR-021.1 §B/§G: root object IDs in an epoch-invariant secret, optionally rotating it via coordinated re-ID. | **REJECT-must-change** | The current derivation contradicts rotation. Generate a random vault-lifetime namespace key distinct from every `VK_e`, distribute it through authenticated membership state, and explicitly accept its post-revocation metadata capability. |
| I.3 | ADR-021.1 §C: keep one library-wide chain with per-object vectors or split into per-object subchains. | **REJECT-must-change** | The library-wide linear chain cannot accept honest concurrent heads. Redesign history topology first; per-object subchains alone do not solve authenticated head convergence. |
| I.4 | ADR-021.1 §C: derive `device_id` and decide whether each device has its own signing/revocation key. | **REJECT-must-change** | Define random per-device IDs and per-device signing keys certified by a member/admin key. Bind counters, signatures, revocation, and historical authorization to that certificate. |
| I.5 | ADR-021.1 §D: hash manifest records and blobs with SHA-256 or BLAKE3. | **NEEDS-DECISION** | Choose SHA-256 for the current HKDF/HMAC family and broad interoperability. Hash a domain-separated, canonical full record and define whether `blob_hash` covers the entire FONDENC2 envelope; recommend that it does. |
| I.6 | ADR-021.1 §D: choose checkpoint cadence, author authority, and concurrent-checkpoint reconciliation. | **REJECT-must-change** | Cadence is secondary to the missing concurrency and bootstrap trust model. A checkpoint must identify all parent heads, bind historical roster state, and be accepted only under defined authority and corroboration rules. |
| I.7 | ADR-021.1 §E: keep trusted rollback state outside `fond.db`, in a file or OS keychain. | **NEEDS-DECISION** | A durable file outside `fond.db`, atomically updated and MACed with a device-local key, is a portable baseline. Keychains do not generally provide monotonic anti-rollback storage either. Document backup/restore reset behavior and require an explicit re-trust flow. |
| I.8 | ADR-021.1 §E: hard-fail or warn/quarantine after detecting fork or equivocation. | **NEEDS-DECISION** | Hard-stop automatic sync, preserve both branches and evidence, and present a quarantine/recovery workflow. "Warn and continue" would normalize a detected integrity failure. |
| I.9 | ADR-021.1 §F: reap tombstones after dominance by all roster devices or only recently active devices. | **NEEDS-DECISION** | Require signed acknowledgement/dominance from every device in the current roster plus a checkpoint that commits the deletion. Offline devices pin tombstones until explicitly removed by a signed roster transition; "recently active" alone is unsafe. |
| I.10 | ADR-021.2 §C: pin an OPAQUE crate/version and confirm the cited audit covers that exact release. | **DEFER-TO-HUMAN** | Pin stable `opaque-ke 4.0.1` only after a human reviews changes since the 2021 v0.5.0 audit and the exact dependency tree. RFC-vector tests do not close the audit gap. |
| I.11 | ADR-021.2 §F: pin OPAQUE group/hash/KDF/MAC/AKE and bind its KSF to the A0.3 registry. | **NEEDS-DECISION** | Pin OPAQUE-3DH with ristretto255/SHA-512, HKDF-SHA-512, HMAC-SHA-512, and a separately domain-identified Argon2id registry profile. Human review still must approve exact `opaque-ke` types/features, the fixed-salt KSF adapter, and dependency versions. |
| I.12 | ADR-021.2 §E: choose OPAQUE-envelope/export-key binding or a sidecar Ed25519 self-signed identity attribute. | **REJECT-must-change** | Reject envelope-only/export-key binding as the reset-independent anchor. Use a canonical self-signed sidecar chained into the roster/account history, with old-key authorization for rotation and a first-registration trust rule. |
| I.13 | ADR-021.2 §E: require the server to keep the vault-identity attribute immutable across OPAQUE reset. | **REJECT-must-change** | A malicious server cannot be trusted to preserve an immutable column. Clients must reject identity changes that do not chain from their trusted roster/key commitment; server enforcement is defense in depth only. |
| I.14 | ADR-021.2 §D/§E: define canonical, domain-separated Ed25519 authorization for every destructive/key operation. | **REJECT-must-change** | No implementable transcript exists. Pin a domain label, protocol version, vault/account/member IDs, operation type, roster hash/epoch, request nonce or counter, expiry where relevant, payload digest, and canonical encoding; define replay storage and historical signer authorization. |
| I.15 | ADR-021.2 §B/§C: specify when the non-default, non-hand-rolled SRP-6a fallback may engage. | **REJECT-must-change** | Specify no fallback. If the pinned OPAQUE implementation is unavailable or fails review, the feature remains disabled. This avoids negotiation downgrade and an unaudited verifier-based path. |
| I.16 | ADR-021.2 §F: domain-separate one passphrase's OPAQUE KSF use from its MUK Argon2 use. | **NEEDS-DECISION** | Add explicit length-prefixed application labels to the bytes fed to OPAQUE and the MUK path, retain independent salts/secret inputs, and never reuse derived values. Structural separation is strong but should not be the only written domain boundary. |
| VR-0212-B.1 | ADR-021.2 §B: downgrade every "audited/validated for fond" PAKE claim to validation-required evidence. | **ACCEPT-as-specified** | Correct: dependency provenance is evidence, not validation of fond's composition. Keep this honesty rule. |
| VR-0212-C.0 | ADR-021.2 §C: select `opaque-ke` as primary with RustCrypto `srp` as fallback. | **REJECT-must-change** | Select a human-approved, pinned `opaque-ke` configuration without SRP. A second PAKE multiplies downgrade and audit surface without preserving the required security property. |
| VR-0212-C.1 | ADR-021.2 §C: locate the `opaque-ke` third-party review and prove it covers the pinned release. | **REJECT-must-change** | The public NCC review covered v0.5.0 in 2021, not v4.0.1. Cite it as ancestor-lineage evidence and require review of the intervening changes; do not state that the pinned release is audited. |
| VR-0212-C.2 | ADR-021.2 §C: `opaque-ke` implements OPAQUE-3DH/RFC 9807 and publishes upstream vectors. | **ACCEPT-as-specified** | v4.0.1 states RFC 9807 alignment and exercises published vectors for recommended profiles. Treat this as conformance evidence, not independent validation. |
| VR-0212-C.3 | ADR-021.2 §C: current `voprf` was reviewed alongside the OPAQUE work. | **REJECT-must-change** | The current standalone `voprf 0.5.0` postdates the audit. "Reviewed alongside" overstates exact-code coverage. |
| VR-0212-C.4 | ADR-021.2 §C: `voprf` conforms to RFC 9497. | **ACCEPT-as-specified** | The crate states RFC 9497 alignment and runs RFC vector suites. Again, this is conformance evidence rather than a current cryptographic audit. |
| VR-0212-C.5 | ADR-021.2 §C: treat RustCrypto `srp` as unaudited for fond. | **ACCEPT-as-specified** | Treating it as unaudited is accurate. That evidence supports removing it, not retaining it as a "reviewed" fallback. |
| VR-0212-F.1 | ADR-021.2 §F ciphersuite table: use OPAQUE-3DH as the AKE. | **ACCEPT-as-specified** | Pin OPAQUE-3DH, the primary RFC 9807 construction. Do not select informative alternative AKEs exposed by a generic crate API. |
| VR-0212-F.2 | ADR-021.2 §F ciphersuite table: use ristretto255, with P-256 noted as an alternative. | **ACCEPT-as-specified** | Pin ristretto255 with SHA-512 as the chosen RFC profile. Do not leave P-256 as runtime negotiation; a separate build profile would require separate vectors and review. |
| VR-0212-F.3 | ADR-021.2 §F ciphersuite table: pair the selected OPRF group with SHA-512. | **ACCEPT-as-specified** | SHA-512 is the defined partner for the ristretto255 profile and has broad implementation support. |
| VR-0212-F.4 | ADR-021.2 §F ciphersuite table: use HKDF-SHA-512 and HMAC-SHA-512. | **ACCEPT-as-specified** | HKDF-SHA-512 and HMAC-SHA-512 align with the selected profile. Pin exact crate types and output lengths. |
| VR-0212-F.5 | ADR-021.2 §F ciphersuite table: run the OPAQUE KSF through Argon2id profiles from A0.3. | **NEEDS-DECISION** | Reuse a common audited parameter registry implementation, but give OPAQUE KSF entries an explicit protocol domain and pin the crate adapter's fixed-salt semantics. RFC conformance vectors use the identity KSF and do not test this integration. |
| VR-0212-F.6 | ADR-021.2 §F ciphersuite table: use RFC 9807 nonce and seed lengths. | **ACCEPT-as-specified** | Use the exact RFC 9807 suite lengths with no application override. |
| VR-0212-F.7 | ADR-021.2 §F KSF text: use one profile ID for account/MUK or two registry entries. | **NEEDS-DECISION** | Use distinct domain-specific profile IDs even if their initial triples match. This permits independent retuning and prevents a login-latency change from silently changing vault-unlock policy. |
| VR-0212-F.8 | ADR-021.2 §F threat claim (a): OPAQUE registration records yield no offline password guessing. | **REJECT-must-change** | Qualify it: records alone, excluding `oprf_seed` and an oracle, do not enable offline verification. A corrupted single server with the seed can attack offline at the KSF cost, as RFC 9807 states. |
| VR-0212-F.9 | ADR-021.2 §F threat claim (b): records plus wrapped Vault Keys still cannot decrypt the vault. | **ACCEPT-as-specified** | Assuming the Secret Key and identity private keys truly never reach the server, records and wraps alone do not derive the MUK or decrypt the Vault Key. Fixing private-key lifecycle is a prerequisite. |
| VR-0212-F.10 | ADR-021.2 §F threat claim (c): counts, sizes, timing, device IDs, and post-revocation ID correlation leak. | **ACCEPT-as-specified** | The stated leakage of counts, sizes, timing, device IDs, and retained object-ID correlation is accurate. Padding and traffic-shaping remain optional hardening, not zero-knowledge content guarantees. |

## Test vectors

The machine-readable vectors are in:

`test-vectors/fondenc2/illustrative-vectors.json`

Every vector carries the label:

`ILLUSTRATIVE-FOR-THE-PLACEHOLDER-CIPHERSUITE`

The throwaway generator was kept outside the repository in the session artifact
directory. It used Python 3.13.7, `cryptography 49.0.0`, `argon2-cffi 25.1.0`,
and `PyNaCl 1.6.2`. Its HKDF output was independently computed with Python
`hmac`/`hashlib` and `cryptography`.

Generation command:

```sh
/Users/javier/.copilot/session-state/2f59d033-ba35-47e8-9f48-b3e3e0e88ff7/files/a05-vectors/.venv/bin/python \
  /Users/javier/.copilot/session-state/2f59d033-ba35-47e8-9f48-b3e3e0e88ff7/files/a05-vectors/generate_vectors.py \
  --output /Users/javier/.copilot/session-state/2f59d033-ba35-47e8-9f48-b3e3e0e88ff7/files/a05-vectors/illustrative-vectors.json
```

SHA-256:

```text
8feb77477e51dba3f035f36a43b664363d8d4cb9e0fc36a5a07f697818ea1d75  illustrative-vectors.json
```

Generated primitive-level cases:

| Vector | Decidable output |
|---|---|
| FONDENC2 object envelope | HKDF-derived fixed 32-byte DEK, 24-byte nonce, 54-byte complete header/AAD, plaintext, ciphertext, tag, and successful decrypt |
| HKDF-SHA-256 | Fixed Vault Key, Extract salt, purpose/epoch info, PRK, purpose subkey, DEK info, and the object DEK consumed by the envelope vector |
| Argon2id profile | Fixed `PROFILE[2]` placeholder parameters, salt, password bytes, version 0x13, and 32-byte output; deliberately does not choose K.1 |
| Ed25519 manifest entry | Fixed seed, public key, explicitly illustrative payload, signature, and successful verify |

Key output anchors:

```text
FONDENC2 header length: 54
FONDENC2 tag: dec2d0296209889c5b849117d25a61b0
HKDF content subkey: 0c1d1b639f8631c1e1fcc11b246c02c66a40da1469a6e65b808cc98eba902ca4
Argon2id output: 5338a5a03624b9a80196c89057536a68070bf99cba84d6f1460d26a61ebe386e
Ed25519 public key: 2543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d
```

### Blocked canonical vectors

| Required vector | Blocking decisions |
|---|---|
| Vault-Key wrap/unwrap | K.1, K.2, K.13, and undefined roster-binding AAD |
| Full manifest verification | I.3-I.6, canonical record/map serialization, historical roster binding, external record/head addressing, and N-03/N-04/N-12 |
| Object ID | K.11/I.1, I.2, and A4 durable UUID |
| Roster chain/rotation | K.8, K.16, canonical roster hash/signature encoding, N-01, and N-02 |
| Invitation | K.3 and authenticated recipient-key binding from N-08 |
| OPAQUE login | I.10, I.11, I.16, exact crate features, and final KSF profile |

The Ed25519 case is a primitive sign/verify vector, **not** full manifest
verification. Publishing a made-up canonical manifest or wrap output would hide
the very ambiguities this gate is intended to catch. Issue #120's wrap/unwrap
and manifest-verification vector criterion therefore remains unmet until the
blocking decisions are resolved and final cross-implementation vectors replace
or supplement these illustrative cases.

## Residual human-review checklist

This is the ordered handoff to the external reviewer. No Epic A, C, or D code
may merge until every row is signed off against a revised, hash-pinned spec.

| Order | IDs closed | Human cryptographer must sign off that |
|---:|---|---|
| 1 | `K.7`, `K.8`, `K.12`, `K.16`, `I.2`, `I.4`, N-01, N-02, N-04, N-10 | Epoch-key distribution is non-circular; old-epoch keys, member/device private keys, owner transfer, new-device restore, new-member history access, revocation, and roster/manifest transition are recoverable and historically authorized. |
| 2 | `I.3`, `I.6`-`I.9`, N-03, N-05, N-07, N-12 | The authenticated history accepts honest concurrent writers, names immutable encrypted records and heads, merges forks without data loss, gives new devices a freshness anchor, persists sufficient own-write state, and reaps tombstones safely. |
| 3 | `K.1`-`K.5`, `K.10`, `K.11`, `I.1`, `I.5`, `I.12`-`I.16`, `VR-020-D.1`, `VR-0212-F.1`-`VR-0212-F.7`, N-08, N-09, N-13, N-14 | One byte-exact suite is pinned: Argon2 0x13 Secret-Key input, HKDF salts/info, XChaCha wrap AAD, HPKE invitation and recipient authentication, 32-byte typed object IDs, canonical roster/manifest/authorization transcripts, client-anchored identity reset continuity, no SRP fallback, and explicit two-use passphrase separation. |
| 4 | `I.10`, `I.11`, `VR-0212-B.1`, `VR-0212-C.0`-`VR-0212-C.5`, `VR-0212-F.8`-`VR-0212-F.10` | The exact `opaque-ke 4.0.1`/`voprf 0.5.0` features and dependency graph are acceptable despite the 2021 audit gap; RFC-vector evidence is not mislabeled as audit; full-server-compromise/offline-guessing and metadata claims are precise. |
| 5 | `K.13`-`K.15`, `VR-020-K13.1`-`VR-020-K13.5`, N-06, N-11 | Measured desktop, phone, and watch Argon2 floors are safe; pre-auth accepted-profile limits are platform-bounded; profile upgrade is explicit and recoverable; FONDENC1 parameters are allowlisted before KDF; legacy deletion claims are honest. |
| 6 | `K.6`, `K.9`, every remaining ACCEPT/NEEDS row, and issue #120 AC 2-4 | Maintainer-selected granularity and optional re-encryption do not invalidate the proof obligations; final independent vectors cover MUK/KEK, wrap/unwrap, invitation, IDs, roster transition, concurrent merge, checkpoint bootstrap, full manifest verification, authorization, OPAQUE, and migration rejection; accepted residual risks, final spec/vector hashes, and explicit sign-off are recorded on #120. |

Until those items are complete, this review should be used as a human-review
briefing package, not as permission to implement the protocol.
