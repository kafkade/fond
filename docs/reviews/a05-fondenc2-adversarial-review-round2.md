# A0.5 FONDENC2 adversarial re-review - round 2

**Review status:** Model-based advisory re-review  
**Reviewer:** GPT-5.6-Sol  
**Date:** 2026-07-27  
**Issue:** [#120](https://github.com/kafkade/fond/issues/120)  
**Implementation gate verdict:** **NO-GO**

> **Human sign-off is still mandatory.** This is a model-based adversarial
> review, not an independent human cryptographic review. It does **not** close
> issue #120, does **not** constitute a proof or audit, and does **not** clear
> implementation. Only a human cryptographer's explicit sign-off can do that.
> Every unverified "audited", "proven", "secure", or composition claim is
> treated below as `[Validation Required]`.

## Advisory verdict

**NO-GO for implementation.** The revision closes several important paper
decisions: it removes SRP fallback, corrects the 16-byte object-ID collision
claim by selecting 32-byte IDs, replaces the inaccurate `crypto_box_seal`
description with a pinned HPKE suite, adopts a multi-parent history DAG, states
metadata and withholding limits more honestly, and defines explicit profile
deprecation behavior.

It does not yet compose into an implementable protocol. Three independent
failures are enough to keep the gate closed:

1. **N-15 - epoch rotation is mechanically impossible.** Each member package
   is encrypted under that member's passphrase-derived symmetric KEK, but
   rotation tells an admin to re-wrap every remaining member's package. The
   admin cannot derive those KEKs. No per-member public-key epoch grant exists.
2. **N-32/N-18 - new-member bootstrap is stale and incomplete.** The signed
   invitation binds neither current roster/epoch/frontier state nor
   `NS_objectid`. A malicious server can replay an older self-consistent state,
   and the invitee cannot construct the specified package from the delivered
   Vault Key alone.
3. **N-06 - the real pre-auth Argon2 path remains live.** The migration text
   requires a safe pre-parser, but production `open_bundle` and `open_blob`
   still derive from unauthenticated `m_cost`/`t_cost`/`p_cost` before AEAD
   verification.

The remaining roster, manifest, authorization, identity, archive, and KDF
transcripts are also not byte-exact. The final grade across the original 55
rows is **24 RESOLVED, 15 PARTIALLY-RESOLVED, and 16 NOT-RESOLVED**.

## Primary deliverable: residual-human checklist

The external reviewer must verify all of the following before a GO is possible.
These are gates, not optional hardening.

| Human gate | Required evidence before GO | Findings / clauses |
|---|---|---|
| Epoch distribution redesign | A byte-exact design that separates stable member-private material from per-epoch grants, lets an admin grant `VK_{e+1}` using public information, handles offline remaining members, and excludes revoked members. | N-01, N-15, K.2, K.12, K.16; ADR-020 lines 412-452, 891-942 |
| Invitation freshness and completeness | A signed or inseparably bound invitation carrying the exact current epoch, roster/transition hash, trusted frontier or checkpoint, `NS_objectid`, HPKE ciphertext digest, expiry, and recipient-authentication transcript. | N-07, N-08, N-18, N-32, K.3, I.2, I.6; ADR-020 lines 508-535; ADR-021 lines 406-417 |
| Legacy pre-auth resource bounds | Tests proving every passphrase-mode FONDENC1/FONDBKP1 entry point rejects non-allowlisted costs before `Params::new` or Argon2 allocation. Include both `open_bundle` and `open_blob`. | N-06; ADR-020 lines 996-1005; `crypto.rs` lines 248-285, 361-431, 487-503 |
| K.13 measured profiles | Approved desktop, phone, and watch triples based on minimum-device p50/p95 latency, peak RSS, thermal/battery behavior, and concurrent unlocks; exact local accepted sets and ceilings. No placeholder values may be promoted without evidence. | K.13, VR-020-K13.1-.3, VR-020-K13.5; ADR-020 lines 763-819 |
| OPAQUE implementation review | Exact `opaque-ke 4.0.1` types, features, dependency graph, `voprf` use, fixed-salt Argon2 KSF adapter, label placement, password encoding, and reset flow. The 2021 NCC audit covered `opaque-ke` v0.5.0, not v4.0.1; standalone `voprf 0.5.0` postdates it. | I.10, I.11, VR-0212-C.1, VR-0212-C.3, VR-0212-F.5; ADR-021 lines 770-785, 910-975 |
| One canonical codec | A versioned byte codec defining `len_prefix` width/endianness, integer encoding, list/map sorting, duplicate rejection, optional-field encoding, and every signed, hashed, HKDF, HMAC, HPKE, certificate, roster, transition, manifest, authorization, archive, and identity transcript. | K.1, K.3, K.4, K.7, K.8, I.1, I.5, I.12, I.14, N-17, N-20-.22 |
| Roster state machine | Semantic transition rules for join, self-wrap update, device enrollment/revocation, role change, owner transfer, and epoch change; an unsigned-body definition for signatures; historical verification; and member-authorized self-service. | N-02, N-16, N-17, N-19; ADR-020 lines 453-498 |
| DAG and durable client state | Crash-atomic ordering across local mutation, op-log append, object write, manifest signing, head-counter persistence, and publication; equal-counter conflict handling; checkpoint/tombstone consistency; compaction and re-trust abuse analysis. | N-03, N-04, N-05, N-23, N-24, N-30; ADR-021 lines 327-530, 547-554 |
| Archive and history-barrier policy | Confirmation that current-key compromise reveals all archived history; exact archive addressing and corruption recovery; signed and prominently confirmed household-wide barrier semantics; backup requirements; no member-selective claim without segment grants. | N-01, N-21, N-22; ADR-020 lines 633-677 |
| Identity/account lifecycle | `identity_seed` compromise analysis, exact HKDF semantics, actual sidecar references from roster state, first-registration trust, old/new-key rotation signatures, and a crash-safe passphrase change spanning OPAQUE registration and MUK re-wrap. | K.10, I.12, I.13, N-20, N-25, N-28, N-29; ADR-020 lines 679-711; ADR-021 lines 869-946 |
| Zero-knowledge claim review | Approval of the precise content-only confidentiality claim and all disclosed metadata; qualification that signatures detect accepted logical deletion but cannot prevent a malicious store from physically deleting or withholding ciphertext. | N-27, N-31; ADR-021 lines 573-602, 848-856, 948-966 |
| Independent vectors and issue sign-off | Reproduce the final normative KATs with independent implementations, record exact dependency versions/features, review every blocked-vector decision, and record explicit human approval on issue #120. | All K-/I-/VR- gates; round-2 vector section below |

## Scope, provenance, and method

The re-review compared the revised ADRs against the complete round-1 review,
its 55-row adjudication, its N-01 through N-14 findings, its vectors, and the
current FONDENC1 code.

| Reviewed input | SHA-256 |
|---|---|
| `docs/adr/020-zero-knowledge-identity.md` | `dfb424c015a3ef0d552735f13417f6c21a627f62de20481f715eacc7f92a037c` |
| `docs/adr/021-optional-sync-server.md` | `58c61e64508b8959f3734fdbbd725990f55c81fd8df12696e5fec4a7f4eb3354` |
| `docs/reviews/a05-fondenc2-adversarial-review.md` | `d0693f0d879ec4195484734c2bccf23943a8f424552dc3a495ed4fac17d2a4c1` |
| `test-vectors/fondenc2/illustrative-vectors.json` | `8feb77477e51dba3f035f36a43b664363d8d4cb9e0fc36a5a07f697818ea1d75` |
| `crates/fond-store/src/crypto.rs` | `7ff41b4a3f49b3af9a65de7dda3bd78e94028e8d20146caee6fee38947b56ade` |
| `test-vectors/fondenc2/round2-vectors.json` | `b9c9a8b8e0d5262ad6611e580deaff7c27553cb7ec588b83fa6b4b31ab1c4a37` |

Absolute evidence-path aliases used in every adjudication table:

- **A20:** `/Users/javier/dev/kafkade/copilot-worktrees/fond/kafkade-cautious-invention/docs/adr/020-zero-knowledge-identity.md`
- **A21:** `/Users/javier/dev/kafkade/copilot-worktrees/fond/kafkade-cautious-invention/docs/adr/021-optional-sync-server.md`
- **R1:** `/Users/javier/dev/kafkade/copilot-worktrees/fond/kafkade-cautious-invention/docs/reviews/a05-fondenc2-adversarial-review.md`
- **C:** `/Users/javier/dev/kafkade/copilot-worktrees/fond/kafkade-cautious-invention/crates/fond-store/src/crypto.rs`
- **B:** `/Users/javier/dev/kafkade/copilot-worktrees/fond/kafkade-cautious-invention/crates/fond-store/src/backup.rs`

The revised files were merged unchanged from the transient review worktree;
the SHA-256 values above pin that content. For example, `A20:L639-L648`
denotes the exact absolute A20 path above at lines 639 through 648. Quotations
in the tables are verbatim except for ellipses that remove text without
changing the quoted claim.

Verdict terms:

- **RESOLVED:** the revised clause closes the round-1 issue at paper-design
  level without a new contradiction.
- **PARTIALLY-RESOLVED:** the direction is materially improved, but a required
  transcript, state transition, trust binding, or implementation property is
  missing.
- **NOT-RESOLVED:** the original issue remains, the replacement is internally
  contradictory, or the required repository behavior is still absent.

The round-1 prose visibly numbered five umbrella blockers. The request for this
round names six specific structural finding groups. This review uses those six
groups and treats N-06 as the sixth item; it does not rewrite the round-1 text.

## Six structural blockers

| Structural group | Exact revised clause | Round-2 verdict |
|---|---|---|
| **N-01 - epoch archive/recovery** | A20 §L, A20:L639-L648: "`archive[e] = VK_e` sealed under a subkey of `VK_{e+1}`" and "any holder of the current Vault Key can walk the chain backward." A20:L897-L898 then require: "Re-wrap each **remaining** member's **key package** ... (each KEK)." | **NOT-RESOLVED.** The archive direction recovers old keys from a current key, but N-15 prevents distribution of that current key to other members. Archive addressing and lost-link recovery also remain undefined. |
| **N-02 - roster key cycle** | A20 §G, A20:L458-L484: "**Membership & wrap directory — authenticated, NOT confidential under any `VK`.**" and the directory "sits **outside** the new-key encryption boundary." | **PARTIALLY-RESOLVED.** The confidentiality cycle is broken. The signed body's canonical bytes, signature self-reference, semantic transition checks, member self-service, and fresh-state bootstrap are not. |
| **N-03/N-12 - concurrency and immutable addressing** | A21 ADR-021.1 §C/§D, A21:L320-L377: "an authenticated multi-parent DAG" and "`record_id` is ... SHA-256 of the sealed envelope bytes." | **PARTIALLY-RESOLVED.** The topology no longer rejects honest siblings. "Compare-and-append" checks only parent existence, canonical record bytes are absent, and signed frontier/counter durability is incomplete. |
| **N-04 - historical authorization** | A21 ADR-021.1 §D/§E, A21:L390-L397 and A21:L466-L505: each record carries the "`roster_hash` that authorized its signer at signing time" and an old-roster record "is normally valid only as an ancestor of the committed ... transition." | **PARTIALLY-RESOLVED.** Historical intent is correct, but roster hashes, certificates, transitions, and causal-cut records are not byte-defined or governed by a complete authorization state machine. |
| **N-05 - own-write anti-rollback** | A21 ADR-021.1 §E, A21:L436-L442: a "durable append-only operation log" with `entry = { object_id, own_component_after, prev_oplog_hash }`. | **PARTIALLY-RESOLVED.** The scalar defect is recognized, but entries do not bind the authored record/blob, and no crash-atomic order connects the log to mutation, sealing, signing, or publication. |
| **N-06 - pre-auth Argon2** | A20:L996-L1002 requires rejecting legacy triples "outside a small compiled allowlist/cap" before `open_bundle`. C:L248-L285 and C:L361-L431 still derive before AEAD authentication. | **NOT-RESOLVED overall.** The paper fix is correct but absent from both real openers, including the B:L475-L481 backup-reachable `open_blob` path. |

## The 18 former REJECT rows

| ID | Exact revised clause | Verdict and delta |
|---|---|---|
| **K.7** | A20:L418-L426: each device holds its "**own random Ed25519 signing key**" and the key is "**certified** by the member identity key." | **PARTIALLY-RESOLVED.** The key model is fixed; the certificate lacks a domain label, protocol version, canonical fields, and exact time encoding. |
| **K.12** | A20 §M, A20:L679-L711: identity derives from a "random `identity_seed` carried in the KEK-wrapped member key package"; the Kit is "**Secret Key only**." | **NOT-RESOLVED.** Recovery depends on the impossible N-15 cross-member rotation flow, and "Kit protects against passphrase loss" contradicts the two-secret loss statement. |
| **K.14** | A20:L793-L801: deprecated-profile unlock "**MUST NOT silently** re-wrap" and migration is "transactional, crash-safe, rollback-safe." | **RESOLVED** as a paper lifecycle decision. |
| **K.16** | A20:L904-L931: "One **signed transition object** makes roster epoch and ... manifest `vault_epoch` a single state-machine commit." | **NOT-RESOLVED.** The transition cannot be constructed while remaining-member wraps cannot be produced; `archive_ref` and prepared-to-committed representation are also undefined. |
| **VR-020-D.1** | A20:L86-L95 and A21:L758-L766: "There is **no SRP-6a fallback**." | **RESOLVED.** |
| **VR-020-K13.5** | A20:L774-L790: each platform ships a "**small accepted-profile set**" and "**much lower local pre-auth `m_cost` ceiling**." | **NOT-RESOLVED.** The policy is correct, but no actual accepted sets or ceilings are pinned; this remains part of K.13 human work. |
| **I.2** | A21:L254-L282: a "random vault-lifetime key `NS_objectid`" is "distinct from every per-epoch `VK_e`." A20:L532-L534 says the invitee HPKE-opens only the Vault Key, then assumes shared `NS_objectid`. | **NOT-RESOLVED.** Rotation invariance is selected, but new-member distribution is absent. |
| **I.3** | A21:L320-L325: "an authenticated multi-parent DAG, not a single linear chain." | **PARTIALLY-RESOLVED.** Correct topology, incomplete codec and state machine. |
| **I.4** | A21:L312-L318: "`device_id` is a **random** 16-byte id" and each device has its "**own random Ed25519 signing key**, **certified** by the member identity key." | **PARTIALLY-RESOLVED.** Correct key ownership, incomplete certificate/enrollment transition. |
| **I.6** | A21:L406-L417: checkpoint parents name "**all** frontier heads" and a new device "must not accept any checkpoint as genesis on the server's word." | **NOT-RESOLVED.** Same-member enrollment carries an anchor, but the new-member invitation does not. |
| **I.12** | A21:L869-L908: a "**client-anchored, self-signed sidecar chained into the roster/account history**." | **PARTIALLY-RESOLVED.** The transcript omits vault/protocol binding, leaves `registration_context` undefined, and no shown roster field references it. |
| **I.13** | A21:L881-L905: "Server immutability is therefore **defense-in-depth only**, never the security invariant." | **PARTIALLY-RESOLVED.** The trust principle is corrected; continuity remains unimplementable until the sidecar/roster link and first-registration rule are byte-defined. |
| **I.14** | A21:L823-L846: "`auth_sig = Sign_{ed25519}( canonical(`" and "`payload_digest = SHA-256(op payload)`." | **NOT-RESOLVED.** `canonical(...)`, widths, ordering, operation payloads, authorization matrix, and durable replay state are still unspecified. |
| **I.15** | A21:L758-L773: OPAQUE "is the sole account aPAKE; there is **no SRP-6a fallback**." | **RESOLVED.** |
| **VR-0212-C.0** | A21:L758-L773: "`opaque-ke` ... with no SRP-6a fallback." | **RESOLVED.** |
| **VR-0212-C.1** | A21:L775-L785: the audit covered "v0.5.0 and an earlier draft, **NOT the current v4.0.1**." | **RESOLVED** as an honesty correction; I.10 remains human-gated. |
| **VR-0212-C.3** | A21:L778: "The current standalone `voprf 0.5.0` **postdates** the 2021 OPAQUE audit." | **RESOLVED** as an honesty correction; I.10/I.11 remain human-gated. |
| **VR-0212-F.8** | A21:L948-L957: records "**excluding the `oprf_seed`**" do not enable offline verification, but a "**corrupted single server**" can mount an offline dictionary attack. | **RESOLVED** as a qualified claim. |

## Primitive corrections N-13 and N-14

### N-13 - object-ID width

**RESOLVED for width; not vector-complete.** ADR-020 lines 347-352 and
ADR-021 lines 268-275 now say:

> A 16-byte truncation gives 128-bit preimage/forgery strength but only 64-bit
> generic collision strength. The full 32-byte HMAC output is used.

That corrects the prior claim and the envelope is consistently widened to 70
bytes. Canonical object-ID computation still waits on the byte definition of
`len_prefix`, the complete object-class taxonomy, and A4 durable recipe UUIDs.

### N-14 - invitation primitive

**RESOLVED for primitive description; invitation remains
PARTIALLY-RESOLVED.** ADR-020 lines 517-520 pin:

> HPKE Base, DHKEM(X25519, HKDF-SHA-256), HKDF-SHA-256,
> ChaCha20-Poly1305.

The text correctly stops calling `crypto_box_seal` an XChaCha construction.
It still omits HPKE `info`, AEAD AAD, plaintext schema, ciphertext layout,
fingerprint/SAS encoding, ciphertext digest binding, `NS_objectid`, and the
current roster/epoch/frontier anchor.

## Adversarial analysis of the six new decisions

### 1. Forward-chained epoch-key archive

ADR-020 lines 639-648 state:

> `archive[e] = VK_e` sealed under a subkey of `VK_{e+1}` ... any holder of
> the current Vault Key can walk the chain backward.

The one-way direction targets the stated policy: possession of `VK_e` should
not derive `VK_{e+1}`, while possession of the current key should reveal all
linked historical keys. That property remains `[Validation Required]` because
the archive record, `archive_ref`, and signed/content-addressed encoding are
undefined. If the eventual bytes implement this direction, the compromise
scope is broad by design:
**any current-VK compromise recovers all archived history. There is no
forward secrecy for history.** The revised text says this honestly.

A server can delete or withhold a link. The design intends signatures to make a
forged or reordered logical link detectable, but that claim is
`[Validation Required]` until the signed archive/transition bytes are defined.
No signature can restore deleted bytes.
A malicious authorized admin can also publish a signed household barrier by
omitting the link. No mandatory user confirmation, backup precondition,
archive-record address, or recovery path for a lost link is specified. N-21
and N-22 remain.

Most importantly, the archive does not distribute `VK_{e+1}`. N-15 prevents
the rotation that would give remaining members the root from which to walk it.

### 2. Household-wide history barrier

ADR-020 lines 667-677 correctly retract the member-selective claim:

> History barrier is household-wide, not member-selective ... severs the chain
> for every holder of the current `VK`.

That policy is internally honest. Cached old keys remain usable, so the
barrier is logical key-recovery deletion, not guaranteed erasure. A malicious
server can simulate loss by withholding a link, and an authorized admin can
make the barrier permanent. A human must approve the UX, backup, signatures,
and confirmation requirements. Per-member history restriction remains
deferred until segment roots or equivalent per-member grants are designed.

### 3. Signed cleartext roster and wrap directory

ADR-020 lines 458-484 say the directory is:

> authenticated, NOT confidential under any `VK`

and concede server-visible member count, roles, and public keys. This genuinely
breaks N-02's confidentiality cycle, and the leak matches ADR-021 §G's honest
limits.

The remaining integrity design is incomplete. `admin_sigs[]` are described as
signatures "over the whole record", apparently including the signature list
itself. No unsigned body, canonical codec, roster-hash algorithm, list ordering,
or duplicate rule is defined. The generic "at least one predecessor admin"
acceptance rule does not itself enforce the two-signature owner-transfer rule.
An ordinary member cannot publish their own passphrase re-wrap or device
certificate without an admin signature. `prev_roster_hash` makes rollback
detectable only from already trusted state; a fresh invitee still has N-32's
stale-state gap.

### 4. Unified KEK-wrapped member package

ADR-020 lines 427-444 define:

> `current_Vault_Key(32) || NS_objectid(32) || identity_seed(32)`

under `key = KEK_member`.

The "re-wrap one key" claim works for a member changing their own passphrase:
that member can derive the old and new KEKs. It fails for admin-driven rotation.
Lines 897-898 require the admin to re-wrap every remaining member package, but
only each member knows the passphrase and Secret Key needed for their KEK.
HPKE is used only for initial invitation; no HPKE epoch grant exists. This is
N-15, a new critical blocker.

The one-wrap-per-member shape also conflicts with lines 817-819 allowing
different profiles per member/device: one wrap has one profile. The AAD binds
useful fields, but its `len_prefix` encoding and the member-key serialization
are not byte-defined.

### 5. DAG, signed merges, CAS, frontier, counters, and op-log

The server remains untrusted in the intended topology. Clients recompute
`record_id`, decrypt, compare `parents_ct[]` with signed `parents[]`, verify the
historical roster, and require trusted-frontier dominance. A server is never
supposed to bless a checkpoint on its own.

The guarantees are narrower than some wording suggests:

- "Compare-and-append" at ADR-021 lines 373-377 checks only parent existence
  and record-ID novelty; it does not compare against an expected frontier.
- A server can permanently partition honest sibling branches. The spec
  correctly admits this at lines 507-519 and 573-587.
- Per-signer signed head counters detect lower counters only after a client has
  witnessed a higher one. The signer's own durable next-counter state and
  equal-counter/different-frontier handling are absent.
- `own_oplog` commits only object ID and counter, not the authored record or
  ciphertext hash. No crash ordering links local mutation, log append, object
  durability, manifest signature, head advertisement, and publication.
- Merge records are device-signed, but canonical ordering, duplicate
  rejection, authorization semantics, and conflict payloads are not defined.
- A checkpoint is called the complete live-object set while tombstone reaping
  requires a checkpoint to commit an unreaped deletion. N-24 records that
  contradiction.
- Re-trust after state loss is an explicit security downgrade and must stay
  user-visible; it cannot be presented as equivalent to continuous trust.

### 6. Random `identity_seed` and OPAQUE binding

Moving identity derivation off the passphrase-derived MUK avoids identity
rotation on passphrase changes and makes package-based recovery possible. A
compromised `identity_seed` exposes both stable member private identity keys;
it does not alone expose `VK` or `NS_objectid`, but it permits device
certification and any member/admin authorization allowed to that identity.
That blast radius needs explicit treatment.

The formulas at ADR-020 lines 693-694 call `HKDF-Expand(identity_seed, ...)`
without saying whether `identity_seed` is a PRK or first undergoes Extract.
The account sidecar omits vault/protocol binding, leaves
`registration_context` undefined, and is not referenced by any shown roster
field. Moving identity off the passphrase does not itself create an OPAQUE key
dependency, but the same passphrase still drives two lifecycles. No atomic,
rollback-safe passphrase change coordinates OPAQUE re-registration with MUK
package re-wrap (N-25).

## New round-2 findings

| ID | Severity | Exact clause and attack | Required disposition |
|---|---|---|---|
| **N-15** | **Critical** | ADR-020 lines 412-444: `key = KEK_member`; lines 897-898: admin must "Re-wrap each remaining member's key package ... (each KEK)." The admin has no other member's passphrase, Secret Key, KEK, or private seed. | Separate stable member-private state from epoch grants; use authenticated per-member HPKE grants or equivalent public-key distribution, including offline-member handling. |
| **N-16** | **High** | Lines 440-441 say a member changes their own wrap; lines 507-508 let member identity certify a device; lines 489-492 accept a directory only with an owner/admin signature. | Define member-authorized self-wrap and device subrecords, or require and specify an explicit admin co-sign flow. |
| **N-17** | **High** | Line 474 says admin signatures cover "the whole record"; lines 489-494 accept one predecessor admin but separately require old- and new-owner signatures for transfer. | Define an unsigned canonical body and role-aware semantic transition validator. |
| **N-18** | **High** | Lines 532-534 deliver only the Vault Key through HPKE, then require the invitee to assemble a package "with the shared `NS_objectid`." | Include `NS_objectid` inside the authenticated HPKE plaintext and bind its schema. |
| **N-19** | **Medium** | Lines 427-431 define one wrap per member; lines 817-819 allow different member/device profiles. | Pin one profile per member wrap or define separately addressable per-device wraps. |
| **N-20** | **Medium** | Lines 693-694 use `HKDF-Expand(identity_seed, ...)` without Extract/PRK semantics. | Define HKDF-Extract salt and Expand info bytes, or explicitly define the seed as a PRK with justification and KATs. |
| **N-21** | **Medium** | Lines 642-649 define archive AEAD bytes; line 916 requires a 32-byte `archive_ref` record ID that has no derivation or storage format. | Define archive envelope, content address, nonce/tag layout, lookup, and corruption behavior. |
| **N-22** | **Medium** | Lines 920-931 use `completion = 0/1` but do not define how an immutable signed prepared object becomes committed. | Define separate immutable records and linkage, or one atomic local publication protocol with exact CAS semantics. |
| **N-23** | **Medium** | ADR-021 lines 378-386 require a strictly monotonic signer counter; trusted state stores only counters seen from signers. | Persist the local signer's next counter durably and reject equal-counter/different-frontier signatures. |
| **N-24** | **Medium** | ADR-021 lines 406-411 define checkpoints as a complete live set; lines 547-554 require a checkpoint to commit a not-yet-reaped tombstone. | Define whether checkpoints include pending tombstones and how deletion commitments survive reaping. |
| **N-25** | **High** | ADR-020 lines 954-958 change the MUK package; ADR-021 lines 937-946 use the same passphrase for OPAQUE and MUK. No dual update state exists. | Specify staged, crash-safe, rollback-safe OPAQUE registration plus package re-wrap and recovery from either half completing. |
| **N-26** | **High** | ADR-020 lines 579-580 no-op if "a `FONDENC2` object [is] present"; lines 990-995 say never infer completion from some FONDENC2 object. | Remove the contradictory shortcut and make the durable migration inventory authoritative. |
| **N-27** | **Medium** | ADR-021 lines 848-856 say compromise "can NEVER authorize vault destruction" and cannot "delete or rewrite vault state"; lines 573-587 admit server withholding/deletion. | Limit the claim to client acceptance of logical authorization; physical ciphertext deletion remains possible. |
| **N-28** | **Medium** | ADR-020 lines 109-112 say loss of either passphrase or Secret Key is unrecoverable; lines 708-711 say the Secret-Key-only Kit protects against passphrase loss. | Correct the recovery claim; the Kit cannot recreate a forgotten passphrase. |
| **N-29** | **Medium** | ADR-020 lines 63-65 say the Secret Key "Never leaves the device"; lines 502-503 transport it through Kit/keychain export. | State the real boundary: never uploaded to the server, with an explicit authenticated device-transfer model. |
| **N-30** | **Medium** | ADR-021 lines 373-377 call parent-existence validation "compare-and-append." | Rename it or define an expected-frontier comparison and race semantics. |
| **N-31** | **Medium** | ADR-020 lines 597-598 and ADR-021 lines 183-184, 989-991 claim "reviewed primitives" and "nothing hand-rolled" while I.10/I.11 and bespoke compositions remain open. | Replace with `[Validation Required]` and identify the novel protocol composition. |
| **N-32** | **High** | Same-member enrollment at ADR-020 lines 508-512 carries a freshness anchor. New-member signature at lines 527-530 covers only vault/invite/fingerprint/encapsulation/role/expiry, despite ADR-021 lines 412-417 forbidding server-word genesis. | Bind current epoch, roster/transition hash, frontier/checkpoint, and head counter into the signed and HPKE-authenticated invitation. |

## Zero-knowledge and server trust boundary

The intended boundary remains local-first and content-confidential: the server
does not receive `VK`, MUK, Secret Key, KEK, or plaintext; clients verify CAS
addresses, signed bodies, historical rosters, and frontier continuity. No
checkpoint should be accepted merely because the server supplied it.

The design does not make the server trustworthy and must not imply otherwise:

- The server can delete or withhold ciphertext. The intended
  `[Validation Required]` property is that canonical signatures plus complete
  authorization transitions prevent acceptance of a forged logical deletion;
  those bytes are not yet defined, and signatures would not provide
  availability.
- The server can replay an old state to a device with no prior watermark.
  Same-member enrollment addresses this, but N-32 leaves new-member bootstrap
  exposed.
- The server can maintain permanent split views of branches never compared
  out-of-band. Signed heads expose disagreement only when devices reconcile.
- The server sees account identifiers/email, member count, roles, public keys,
  device certificates/IDs, KDF profile IDs and salts, invitation metadata,
  stable opaque object IDs, blob sizes/timing, CAS parent topology, and signed
  head counters.
- OPAQUE records without `oprf_seed` do not provide offline verification. A
  fully compromised single server holding both records and `oprf_seed` can
  guess at KSF cost, as ADR-021 lines 950-957 now says.

The target claim, still `[Validation Required]`, is **server-blind content
confidentiality plus client-verifiable integrity for witnessed state**. It is
not yet established while canonical codecs, authorization transitions, N-15,
and N-32 remain open, and it would not imply traffic-analysis privacy,
availability, global freshness, or server-proof deletion.

## Domain separation

The intended passphrase uses are cryptographically distinct:

- MUK input: `len_prefix("fond/fondenc2/v2/muk") || NFC(passphrase)`, with the
  32-byte Secret Key in Argon2id's keyed `secret` slot and a per-member salt.
- OPAQUE KSF input: label `fond/fondenc2/v2/opaque-ksf`, applied to the
  OPRF-transformed password under the server's OPRF secret, with no Secret Key.
- Identity: random `identity_seed`, no longer MUK/passphrase-derived.

If implemented exactly with independent salts and outputs, learning one
derivation's output does not directly derive another. The label separation is
the right design direction.

It is not yet byte-exact:

- `len_prefix` has no width, endianness, or nesting rule.
- MUK specifies NFC; OPAQUE's corresponding password normalization/encoding is
  not pinned.
- KEK uses `HKDF-Extract(_, MUK)` with an undefined salt.
- §F formulas concatenate raw label/purpose/epoch and label/class/object ID
  while claiming every multi-field `info` is length-prefixed.
- Identity uses HKDF-Expand without Extract/PRK semantics.
- Device certificates have no protocol/domain label.
- Roster, manifest, transition, authorization, invitation, archive, and
  identity-sidecar canonical encodings remain absent.

Therefore the distinct-label decision is **PARTIALLY-RESOLVED**, while the
claim that every derivation and transcript is fully pinned is
`[Validation Required]`.

## Confirmed FONDENC1 pre-auth Argon2 defect

The attack claimed by the revised ADR is real, and it exists in two openers.

### `open_bundle`

`crypto.rs` lines 487-503 parse attacker-controlled `m_cost`, `t_cost`, and
`p_cost`. Lines 248-260 immediately construct `Params` and run `derive_key`.
AEAD authentication occurs only at lines 277-285.

### `open_blob`

`crypto.rs` lines 361-404 parse the same unauthenticated costs and derive the
key. Only lines 418-431 build AAD and authenticate. `backup.rs` lines 475-481
reach this function for encrypted archive bytes:

> `crypto::open_blob(&bytes[HEADER_LEN..], header_bytes, key)?`

FONDENC1 was explicitly designed to travel over untrusted file sync. A hostile
synced bundle or backup can therefore select extreme Argon2 work before
authentication. ADR-020 lines 996-1002 correctly require a compiled pre-KDF
allowlist, but the repository does not yet enforce it. That legacy allowlist
must come from historically emitted FONDENC1/FONDBKP1 triples plus conservative
local caps. It is separate from K.13, which governs new FONDENC2 and OPAQUE
profiles. No Argon2 numbers are invented here.

## Complete 55-row round-1 delta

Line references are to the SHA-256-pinned revised ADRs above. Short quotations
are included so this review remains useful if the source worktree disappears.

| # | ID | Round-1 disposition | Round-2 verdict | Revised evidence and auditable delta |
|---:|---|---|---|---|
| 1 | K.1 | NEEDS-DECISION | **NOT-RESOLVED** | A20 §C, A20:L286-L295; quote: "use Argon2id's keyed `secret` ... version `0x13` ... a cross-implementation **Known-Answer Test (KAT)**." No two-secret KAT is published. |
| 2 | K.2 | NEEDS-DECISION | **PARTIALLY-RESOLVED** | A20 §G, A20:L442-L452; quote: "**XChaCha20-Poly1305 keywrap** with a random 24-byte nonce" and "`key = KEK_member`." Canonical AAD bytes, KEK Extract salt, and N-15 remain open. |
| 3 | K.3 | NEEDS-DECISION | **PARTIALLY-RESOLVED** | A20 §H, A20:L517-L535; quote: "**HPKE Base-mode** ... DHKEM(X25519, HKDF-SHA-256), HKDF-SHA-256, ChaCha20-Poly1305." Missing `info`, AAD, plaintext, fingerprint encoding, `NS_objectid`, and freshness anchor. |
| 4 | K.4 | NEEDS-DECISION | **NOT-RESOLVED** | A20 §F/§G, A20:L368-L375 and A20:L410-L415; quotes: "`PRK_vault = HKDF-Extract(salt = \"fond/fondenc2/v2/vault\"`" and "`HKDF-Extract(_, MUK)`." The KEK salt is undefined and raw concatenations conflict with "length-prefixed." |
| 5 | K.5 | ACCEPT-as-specified | **RESOLVED** | A20 §E, A20:L353-L361; quote: "**keep pure-random 192-bit nonces, no per-DEK counter**" and "RNG failure is treated as a **platform-fatal error**." |
| 6 | K.6 | NEEDS-DECISION | **RESOLVED** | A20 §F, A20:L404-L408; quote: "**one object per independently-mergeable logical record**." |
| 7 | K.7 | REJECT-must-change | **PARTIALLY-RESOLVED** | A20 §G, A20:L418-L426; quote: "Each *device* additionally holds its **own random Ed25519 signing key** ... and a random `device_id`." Certificate transcript remains incomplete. |
| 8 | K.8 | NEEDS-DECISION | **PARTIALLY-RESOLVED** | A20 §G, A20:L489-L495; quotes: "**per-admin keys**" and "**Ownership transfer** is a chained authorization." Generic directory acceptance and canonical signed body remain incomplete. |
| 9 | K.9 | ACCEPT-as-specified | **RESOLVED** | A20 §H, A20:L549-L558; quote: "**optional lazy/background re-encryption** ... **best-effort forward hardening only**." |
| 10 | K.10 | NEEDS-DECISION | **PARTIALLY-RESOLVED** | A21 ADR-021.2 §E, A21:L869-L908; quote: "a **client-anchored, self-signed sidecar chained into the roster/account history**." Actual roster link, transcript fields, and first-registration trust are incomplete. |
| 11 | K.11 | NEEDS-DECISION | **PARTIALLY-RESOLVED** | A20 §E, A20:L347-L352 and A21 ADR-021.1 §B, A21:L254-L275; quote: "The full 32-byte HMAC output is used, restoring 128-bit collision strength." Canonical `len_prefix`, class taxonomy, and A4 remain. |
| 12 | K.12 | REJECT-must-change | **NOT-RESOLVED** | A20 §M, A20:L679-L711; quotes: "random `identity_seed` carried in the KEK-wrapped member key package" and "**Secret Key only**." Recovery depends on N-15, and the passphrase-loss claim contradicts the two-secret model. |
| 13 | K.13 | DEFER-TO-HUMAN | **NOT-RESOLVED** | A20 A0.3 registry, A20:L802-L819; quote: "These figures are **illustrative anchors, not decisions** and remain **deferred to a human cryptographer**." |
| 14 | K.14 | REJECT-must-change | **RESOLVED** | A20 A0.3 lifecycle, A20:L793-L801; quote: "**MUST NOT silently** re-wrap ... **transactional, crash-safe, rollback-safe**." |
| 15 | K.15 | ACCEPT-as-specified | **RESOLVED** | A20 A0.3 step 9, A20:L1023-L1026; quote: "**retain** the `FONDENC1` blob ... or **best-effort delete** it. Secure erase is **not guaranteed**." |
| 16 | K.16 | REJECT-must-change | **NOT-RESOLVED** | A20 A0.3 transition, A20:L904-L942; quote: "One **signed transition object** makes roster epoch and ... manifest `vault_epoch` a single state-machine commit." N-15 prevents member wraps; N-21/N-22 leave archive/commit representation undefined. |
| 17 | VR-020-D.1 | REJECT-must-change | **RESOLVED** | A20:L86-L95 and A21:L758-L766; quote: "There is **no SRP-6a fallback** ... sync stays disabled rather than downgrading authentication." |
| 18 | VR-020-K13.1 | DEFER-TO-HUMAN | **NOT-RESOLVED** | A20 A0.3 profile table, A20:L802-L813; quote: "`PROFILE[1]` ... `262144` KiB ... **illustrative anchors, not decisions**." |
| 19 | VR-020-K13.2 | DEFER-TO-HUMAN | **NOT-RESOLVED** | A20 A0.3 profile table, A20:L802-L816; quote: "`PROFILE[2]` ... `65536` KiB" and "Phone and watch are **separated** if their measurements diverge." |
| 20 | VR-020-K13.3 | NEEDS-DECISION | **NOT-RESOLVED** | A20 A0.3 measurements, A20:L809-L816; quote: "records p50/p95 time, peak RSS, thermal/battery behaviour" and nominal timings are "goals to measure." |
| 21 | VR-020-K13.4 | ACCEPT-as-specified | **RESOLVED** | A20 A0.3 MUK salt, A20:L853-L858; quote: "**Salt. 16 bytes (128-bit), drawn from the system CSPRNG per member**." |
| 22 | VR-020-K13.5 | REJECT-must-change | **NOT-RESOLVED** | A20 A0.3 accepted profiles, A20:L774-L790; quote: "**small accepted-profile set**" and "**much lower local pre-auth `m_cost` ceiling**." No actual sets or ceilings are pinned. |
| 23 | I.1 | NEEDS-DECISION | **PARTIALLY-RESOLVED** | A21 ADR-021.1 §B, A21:L254-L275; quote: "`‖ u8(object_class) ‖ durable_uuid(16) ‖ u16_le(sub_part)`" and "full 32-byte output." `len_prefix` and complete class semantics remain undefined. |
| 24 | I.2 | REJECT-must-change | **NOT-RESOLVED** | A21:L276-L282; quote: "a random vault-lifetime key `NS_objectid` ... distinct from every per-epoch `VK_e`." A20:L532-L534 delivers only the Vault Key, then assumes the shared namespace key. |
| 25 | I.3 | REJECT-must-change | **PARTIALLY-RESOLVED** | A21:L320-L325 and A21:L327-L372; quote: "an authenticated multi-parent DAG, not a single linear chain." Codec/state-machine work remains. |
| 26 | I.4 | REJECT-must-change | **PARTIALLY-RESOLVED** | A21:L312-L318; quote: "`device_id` is a **random** 16-byte id" and each device carries its "**own random Ed25519 signing key**, **certified** by the member identity key." |
| 27 | I.5 | NEEDS-DECISION | **NOT-RESOLVED** | A21:L398-L400; quote: "**SHA-256** over a **domain-separated canonical full-record** serialization." The canonical serialization is absent. |
| 28 | I.6 | REJECT-must-change | **NOT-RESOLVED** | A21:L406-L417; quote: "`parents[]` name **all** frontier heads" and a new device "must not accept any checkpoint as genesis on the server's word." A20's new-member invitation omits the anchor. |
| 29 | I.7 | NEEDS-DECISION | **RESOLVED** | A21:L455-L462; quote: "a dedicated local state file outside `fond.db`, atomically updated and MAC'd with a device-local key" and an "explicit **re-trust flow**." |
| 30 | I.8 | NEEDS-DECISION | **RESOLVED** | A21:L526-L530; quote: "**hard-stop + quarantine** ... preserves **both** branches and the signed evidence." |
| 31 | I.9 | NEEDS-DECISION | **PARTIALLY-RESOLVED** | A21:L547-L554; quote: "**every device in the current roster**" plus "a **checkpoint** commits the deletion." N-24 leaves checkpoint/tombstone representation inconsistent. |
| 32 | I.10 | DEFER-TO-HUMAN | **NOT-RESOLVED** | A21:L770-L778 and A21:L968-L975; quote: "covered **v0.5.0 ... NOT the current v4.0.1** ... a human MUST review the intervening changes." |
| 33 | I.11 | NEEDS-DECISION | **NOT-RESOLVED** | A21:L910-L946 and A21:L968-L975; quote: "The **suite is pinned**" but "the exact `opaque-ke` types/features/dependency graph remain a human-audit item." |
| 34 | I.12 | REJECT-must-change | **PARTIALLY-RESOLVED** | A21:L869-L908; quote: "a **client-anchored, self-signed sidecar chained into the roster/account history**." Exact transcript and roster reference are missing. |
| 35 | I.13 | REJECT-must-change | **PARTIALLY-RESOLVED** | A21:L881-L905; quote: "Server immutability is therefore **defense-in-depth only**, never the security invariant." Implementable client continuity is incomplete. |
| 36 | I.14 | REJECT-must-change | **NOT-RESOLVED** | A21:L823-L846; quotes: "`auth_sig = Sign_{ed25519}( canonical(`" and "`payload_digest = SHA-256(op payload)`." Codec, payload encoding, role matrix, and replay durability remain undefined. |
| 37 | I.15 | REJECT-must-change | **RESOLVED** | A21:L758-L773; quote: OPAQUE "is the sole account aPAKE; there is **no SRP-6a fallback**." |
| 38 | I.16 | NEEDS-DECISION | **PARTIALLY-RESOLVED** | A21:L937-L946; quote: "`fond/fondenc2/v2/opaque-ksf` for the OPAQUE KSF input and `fond/fondenc2/v2/muk` for the MUK." OPAQUE normalization and exact adapter label placement remain. |
| 39 | VR-0212-B.1 | ACCEPT-as-specified | **PARTIALLY-RESOLVED** | A21:L665-L671 and A21:L989-L991; quotes: "no crate is 'audited/validated' *for fond*" but "a **composition of reviewed primitives** ... with **nothing hand-rolled**." |
| 40 | VR-0212-C.0 | REJECT-must-change | **RESOLVED** | A21:L758-L773; quote: "`opaque-ke` ... with no SRP-6a fallback." |
| 41 | VR-0212-C.1 | REJECT-must-change | **RESOLVED** | A21:L775-L785; quote: "covered **v0.5.0 and an earlier draft, NOT the current v4.0.1** ... ancestor-lineage evidence only." I.10 remains open. |
| 42 | VR-0212-C.2 | ACCEPT-as-specified | **RESOLVED** | A21:L775-L778; quote: "exercises upstream RFC vectors — **conformance evidence, not an audit**." |
| 43 | VR-0212-C.3 | REJECT-must-change | **RESOLVED** | A21:L778; quote: "The current standalone `voprf 0.5.0` **postdates** the 2021 OPAQUE audit." |
| 44 | VR-0212-C.4 | ACCEPT-as-specified | **RESOLVED** | A21:L778; quote: "Runs RFC 9497 VOPRF vectors — conformance evidence `[Validation Required]`." |
| 45 | VR-0212-C.5 | ACCEPT-as-specified | **RESOLVED** | A21:L758-L766; quote: the candidate `srp` crate has "**never received an independent third-party aPAKE audit**." |
| 46 | VR-0212-F.1 | ACCEPT-as-specified | **RESOLVED** | A21:L917-L926; quotes: "AKE", "**OPAQUE-3DH**", and "no alternative AKE." |
| 47 | VR-0212-F.2 | ACCEPT-as-specified | **RESOLVED** | A21:L917-L926; quotes: "OPRF group", "**ristretto255**", and "P-256 is **not** a runtime alternative." |
| 48 | VR-0212-F.3 | ACCEPT-as-specified | **RESOLVED** | A21:L917-L926; quotes: "Hash" and "**SHA-512**." |
| 49 | VR-0212-F.4 | ACCEPT-as-specified | **PARTIALLY-RESOLVED** | A21:L917-L926; quote: "**HKDF-SHA-512 / HMAC-SHA-512** ... pin exact crate types/lengths." Exact types and lengths remain I.10/I.11 work. |
| 50 | VR-0212-F.5 | NEEDS-DECISION | **NOT-RESOLVED** | A21:L925 and A21:L928-L935; quote: "**Argon2id via the A0.3 profile registry, under a distinct OPAQUE domain** ... fixed-salt adapter." The adapter is unvalidated and outside RFC vectors. |
| 51 | VR-0212-F.6 | ACCEPT-as-specified | **RESOLVED** | A21:L917-L926; quotes: "Nonce / seed lengths", "**per RFC 9807**", and "exact suite lengths, no application override." |
| 52 | VR-0212-F.7 | NEEDS-DECISION | **RESOLVED** | A21:L928-L935; quote: "account KSF and the member MUK use **distinct domain-specific profile ids**." |
| 53 | VR-0212-F.8 | REJECT-must-change | **RESOLVED** | A21:L948-L957; quote: records "**excluding the `oprf_seed`**" do not enable offline verification, but a "**corrupted single server**" can mount an offline dictionary attack. |
| 54 | VR-0212-F.9 | ACCEPT-as-specified | **RESOLVED** | A21:L958-L962; quote: "The **Secret Key is never on the server**" and the attacker "cannot derive the **MUK** or unwrap the **Vault Key**." |
| 55 | VR-0212-F.10 | ACCEPT-as-specified | **RESOLVED** | A21:L963-L966 and A21:L588-L602; quote: "Blob counts, sizes, change timestamps, and device ids remain visible" and "Zero-knowledge covers **content**, not traffic analysis." |

### Round-2 count

| Verdict | Count |
|---|---:|
| RESOLVED | 24 |
| PARTIALLY-RESOLVED | 15 |
| NOT-RESOLVED | 16 |
| **Total** | **55** |

## Deterministic vector readiness

The round-1 vector file is stale for this revision: it hashes the prior ADRs,
uses 16-byte IDs and a 54-byte envelope, and its primitive manifest payload is
not the still-missing canonical manifest body.

The round-2 file is:

`test-vectors/fondenc2/round2-vectors.json`  
SHA-256:
`b9c9a8b8e0d5262ad6611e580deaff7c27553cb7ec588b83fa6b4b31ab1c4a37`

Rust `chacha20poly1305 0.11.0` under rustc/cargo `1.96.0` and Python `3.13.7`
with PyNaCl `1.6.2` produced identical sealed bytes. The JSON records the exact
primitive calls, generator-source SHA-256, and recomputation recipe. The
underlying libsodium version is not exposed by this PyNaCl binding and remains
`[Validation Required]`.

| Requested vector | Round-2 readiness |
|---|---|
| 32-byte-ID FONDENC2 envelope | **COMPUTABLE.** The file includes the exact 70-byte header, fixed direct DEK, AAD, ciphertext, tag, and envelope. It does not pretend to validate blocked HKDF bytes. |
| K.2 XChaCha keywrap | **PRIMITIVE-ONLY COMPUTABLE.** The file includes a direct KEK, 96-byte package, nonce, opaque fixed test AAD, ciphertext, and tag. It explicitly does not invent canonical `len_prefix` bytes or validate N-15 rotation. |
| Canonical MUK -> KEK -> keywrap | **BLOCKED.** K.1 KAT absent, K.4 Extract salt absent, K.13 profiles deferred, `len_prefix` undefined, and N-15 breaks cross-member use. |
| Epoch archive chain | **BLOCKED.** Raw-vs-length-prefixed HKDF/AAD contradiction, undefined archive record ID, and undefined transition commit representation. |
| HKDF transcripts | **BLOCKED.** KEK Extract salt is `_`; subkey/DEK formulas conflict with the length-prefix rule; identity Extract/PRK semantics are absent. |
| Object-ID HMAC | **BLOCKED.** Width is fixed, but `len_prefix` is not and A4 durable recipe UUID remains a dependency. |
| HPKE Base invitation | **BLOCKED.** Missing `info`, AAD, plaintext schema, deterministic KAT randomness/ephemeral-key control, fingerprint encoding, signed ciphertext-digest binding, `NS_objectid`, and state-freshness anchor. |
| Ed25519 manifest-record sign/verify | **BLOCKED.** A primitive arbitrary-byte signature would not be a manifest vector; canonical body, ordering, certificate, roster, envelope, and record-ID bytes remain undefined. |
| Roster/transition/identity/authz | **BLOCKED.** Canonical bodies and state transitions are incomplete, and N-15 prevents the rotation transcript. |
| OPAQUE login | **BLOCKED.** K.13, I.10, and I.11 remain human gates; I.16 password normalization and exact adapter-label placement remain incomplete. |

The emitted KATs are illustrative and non-normative. They are not evidence that
the complete protocol is secure or interoperable.

## Claims requiring downgrade

The following statements must be removed or marked `[Validation Required]`
until the human review and normative vectors are complete:

- ADR-020 lines 597-598: "None of these are hand-rolled or novel" and
  "composition of reviewed primitives."
- ADR-021 lines 183-184 and 989-991: "composition of reviewed primitives" and
  "nothing hand-rolled."
- Every remediation-table status saying "Resolved" where this review reports
  partial or not resolved.
- Any claim that a malicious server cannot delete vault state; only accepted
  logical authorization is protected.
- Any claim that all KDF/signature transcripts are fully pinned.
- Any implication that the `opaque-ke 4.0.1`/`voprf` dependency set inherits
  the 2021 v0.5.0 audit.

## Final gate statement

**NO-GO for implementation.** Proceed only with specification and vector
remediation. N-15, N-32/N-18, and N-06 are structural blockers; the remaining
canonical-transcript, roster, DAG durability, identity, K.13, and OPAQUE gates
must also clear. This model-based advisory review narrows the work but does
**not** close issue #120. A human cryptographer must make and record the final
decision.
