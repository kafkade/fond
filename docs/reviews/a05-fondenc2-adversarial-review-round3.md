# A0.5 FONDENC2 adversarial re-review - round 3

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

**NO-GO for implementation.** The round-2 remediation makes real progress:

- The member's symmetric package is now stable and contains no Vault Key.
- An admin needs only a current Vault Key and authenticated member public keys
  to prepare per-member HPKE grants.
- The invitation now carries `NS_objectid` and a signed-and-sealed issuance
  snapshot.
- The specifications now describe the expanded `identity_seed` compromise
  scope and dependency-audit limits more honestly.

The highest-priority fix is nevertheless not mechanically constructible.
**N-33 (new, critical)** is a fixed-point cycle: each `vk_grant` contains the
`roster_hash` of the directory that embeds that same grant. The transition text
explicitly says `new_roster_hash` covers the grants. Genesis and every rotation
therefore require solving:

```text
H = SHA-256(directory(... vk_grant(info = ... H, aad = ... H) ...))
```

No ordinary construction can choose `H` before producing the grant while also
making `H` the hash of the completed directory. This reopens **N-15**, and the
dependent **N-01**, **K.12**, and **K.16** claims remain not resolved.

The invitation fixes completeness but overclaims freshness. A server cannot
alter or transplant the signed ciphertext undetected, but it can replay the
exact same invitation to the same recipient until `not_after`; no consumed
`invite_id` state or equivalent one-time rule exists. The anchor is an
authenticated **issuance snapshot**, not proof of globally latest state.

The same 55-row set now grades **26 RESOLVED, 15 PARTIALLY-RESOLVED, and 14
NOT-RESOLVED**, a delta of **+2 / 0 / -2** from round 2. The only grade changes
are I.2 (`NOT → RESOLVED`), I.6 (`NOT → PARTIAL`), and VR-0212-B.1
(`PARTIAL → RESOLVED`).

## Primary deliverable: residual-human checklist

These are gates, not optional hardening.

| Human gate | Required evidence before GO | Findings |
|---|---|---|
| Non-circular epoch grants | Replace `vk_grant.roster_hash` with a byte-exact, non-circular authenticated context: for example, a canonical grant-free roster-core hash or predecessor-plus-transition commitment. Define exactly which fields and signatures every hash excludes. | N-15, N-33, N-01, K.2, K.12, K.16 |
| Grant composition review | Independently review HPKE Base use, admin signatures, member-key authentication, replay rules, and the expanded `identity_seed` compromise scope. | N-15, N-31, N-33 |
| Invitation freshness | Define invitation HPKE `info`/AAD, canonical field encodings, multi-head frontier commitment, clock/expiry policy, consumed-invite durability, and recovery behavior. Prove the intended replay claim or narrow it to bounded issuance freshness. | N-07, N-08, N-18, N-32, K.3, I.6 |
| One canonical codec | Define `len_prefix`, integer widths, list/map ordering, duplicate rejection, optional fields, and every signed, hashed, HKDF, HMAC, HPKE, certificate, roster, transition, manifest, authorization, archive, and identity transcript. | K.1, K.3, K.4, K.7, K.8, I.1, I.5, I.12, I.14, N-17, N-20-N-22 |
| Roster state machine | Define the unsigned roster body, genesis, join, self-wrap update, device enrollment/revocation, role changes, owner transfer, epoch changes, member self-service, and historical verification. | N-02, N-16, N-17, N-19 |
| Legacy pre-auth bounds | Patch and test both `open_bundle` and `open_blob` so non-allowlisted legacy Argon2 triples fail before `Params::new`, memory allocation, or Argon2 work. | N-06, #121 |
| K.13 measured profiles | Approve desktop, phone, and watch profiles from minimum-device p50/p95 latency, peak RSS, thermal/battery, and concurrency measurements. Pin actual platform accepted sets and ceilings. Do not promote illustrative numbers. | K.13, VR-020-K13.1-.3, VR-020-K13.5 |
| OPAQUE implementation review | Keep this **DEFER-TO-HUMAN**. Review exact `opaque-ke 4.0.1` types/features, `voprf`, dependency graph, fixed-salt Argon2 KSF adapter, label placement, encoding, and reset flow. The 2021 NCC review covered v0.5.0, not v4.0.1. | I.10, I.11, VR-0212-C.1, VR-0212-C.3, VR-0212-F.5 |
| DAG and durable state | Define crash-atomic ordering for local mutation, op-log, object write, manifest signing, counters, trusted state, and publication; resolve equal counters, checkpoint/tombstone encoding, compaction, and re-trust. | N-03-N-05, N-23, N-24, N-30 |
| Archive and transition | Define archive addressing, corruption/loss response, immutable prepared/committed records, signed household-wide barrier UX, backup prerequisites, and the current-key compromise scope. | N-01, N-21, N-22, K.16 |
| Recovery freshness | Define how a fully restored member with only passphrase, Secret Key, and server-held state obtains a non-stale roster/head when no surviving device can provide a trusted anchor. | N-07, K.12, N-32 |
| Passphrase lifecycle | Define a staged, crash-safe, rollback-safe update spanning OPAQUE re-registration and MUK/stable-package re-wrap, including recovery when only one half completes. | N-25 |
| Migration inventory | Remove the "some FONDENC2 object exists" shortcut and make the durable inventory the sole idempotency/completion authority. | N-26 |
| Server trust claims | Keep logical authorization distinct from availability: signatures can reject forged logical deletion but cannot stop physical ciphertext deletion or withholding. | N-27 |
| Secret transfer boundary | Replace "never leaves the device" with the real boundary, "never uploaded to the server," and define authenticated Emergency Kit/device transfer. | N-29 |
| Durable recipe identity | Land and review the A4 durable recipe UUID in `.cook` frontmatter; no slug fallback is safe or stable. | A4 / #124 |
| Independent vectors and sign-off | Reproduce final normative vectors with independent implementations, record dependency versions/features, review every blocked-vector decision, and record explicit human approval on #120. | All open K-/I-/VR- gates |

## Scope, provenance, and method

The review read the revised specifications, the full round-2 review, the
round-2 vectors, and both real legacy open paths before grading.

| Reviewed input | SHA-256 |
|---|---|
| Revised `docs/adr/020-zero-knowledge-identity.md` | `32631fd00bc16ef12a8e69ba41de6de71610a78eb8fceacbf399d4ace4e230f0` |
| Revised `docs/adr/021-optional-sync-server.md` | `f9d4f61660630c9f58171cb587c700c1e9f5fe623fd5de9c1f7ead2bab9f723c` |
| `docs/reviews/a05-fondenc2-adversarial-review-round2.md` | `fa533cc0972363800f5786f737135a0e652c2fb28e5f355ed4ae68e3fa69dc56` |
| `test-vectors/fondenc2/round2-vectors.json` | `b9c9a8b8e0d5262ad6611e580deaff7c27553cb7ec588b83fa6b4b31ab1c4a37` |
| `crates/fond-store/src/crypto.rs` | `7ff41b4a3f49b3af9a65de7dda3bd78e94028e8d20146caee6fee38947b56ade` |

Evidence aliases:

- **A20:** revised `docs/adr/020-zero-knowledge-identity.md`
- **A21:** revised `docs/adr/021-optional-sync-server.md`
- **R2:** round-2 review
- **C:** `crates/fond-store/src/crypto.rs`

For example, `A20:L526-L531` identifies the exact revised file pinned above.
Quotations are verbatim except for explicit ellipses.

Verdict terms:

- **RESOLVED:** the revised clause closes the issue at paper-design level
  without a new contradiction.
- **PARTIALLY-RESOLVED:** direction materially improves, but a required
  transcript, state rule, trust binding, or implementation property is absent.
- **NOT-RESOLVED:** the original issue remains, the replacement is internally
  contradictory, or required repository behavior is absent.

### Identifier reconciliation

The revised ADR swaps the two round-2 invitation identifiers:

- R2:L306 defines **N-18** as missing `NS_objectid`.
- R2:L320 defines **N-32** as missing freshness state.
- A20:L198 calls N-18 the freshness issue, while A20:L199 and A20:L626 call
  N-32 the `NS_objectid` issue.

This review preserves the round-2 identifiers for a 1:1 diff and names the
revised label when needed.

## N-33 - critical grant/directory fixed-point cycle

**Severity:** Critical  
**Confidence:** High  
**Disposition:** New finding; blocks N-15 closure

A20:L513-L523 defines the grant body:

> `roster_hash 32 bytes directory hash that authorizes this grant`

A20:L526-L529 then feeds that value into both HPKE inputs:

> `info = len_prefix("fond/fondenc2/v2/vk-grant") ‖ vault_id ‖ member_id ‖
> epoch_le ‖ roster_hash, aad = <same info bytes>`

The directory embeds the result. A20:L555-L568 says:

> `vk_grant[member] HPKE VK_{current_epoch} grant`
>
> `admin_sigs[] ≥1 owner/admin Ed25519 signatures over the whole record`

The transition text removes any possible predecessor-hash interpretation.
A20:L1090-L1094 says:

> The per-member `vk_grant[member]` records for `e+1` are **fields of the
> `e+1` roster directory**, so `new_roster_hash` already covers them.

The dependency graph is therefore:

```text
new_roster_hash
  -> each vk_grant.info/AAD
  -> each hpke_ct
  -> completed roster-directory bytes
  -> new_roster_hash
```

This is not the old "directory encrypted under its own key" cycle; the
cleartext split fixed that one. It is a new hash-transcript cycle introduced by
the grant binding. It affects:

- **Genesis:** A20:L1183-L1188 requires an epoch-0 directory with a
  self-addressed grant before its roster hash exists.
- **Rotation:** A20:L1053-L1058 requires fresh grants, then builds and hashes
  the directory that contains them.
- **Recovery:** A20:L828-L844 requires opening a grant that cannot be
  constructed under the stated transcript.
- **Atomicity:** A20:L1084-L1094 requires the unconstructible
  `new_roster_hash` before commit.

**Required correction:** hash a canonical grant-free roster core, or bind the
grant to predecessor state plus an independently defined target transition.
The specification must define exact exclusions; merely saying "roster hash"
would recreate the ambiguity.

The pre-existing N-17 signature self-reference also remains. "Signatures over
the whole record" apparently includes `admin_sigs[]`; the unsigned canonical
body is still undefined.

## N-15 - admin-driven epoch rotation

### Full unlock and bootstrap trace

| Step | Exact clause | Round-3 result |
|---|---|---|
| Passphrase + Secret Key → MUK | A20:L317-L331: "`MUK = Argon2id(password = passphrase, secret = Secret Key, salt = ... PROFILE[...])`" | Acyclic direction. K.1 remains NOT because the required two-secret KAT is absent. |
| MUK → KEK | A20:L451-L452: "`KEK = HKDF-Expand(HKDF-Extract(_, MUK), ... )`" | Acyclic but not byte-defined: the Extract salt is `_` (K.4). |
| KEK → stable package | A20:L474-L479: "`stable_member_package = NS_objectid(32) ‖ identity_seed(32)`" | Mechanically openable with passphrase + Secret Key alone, using clear directory fields for AAD. It requires no Vault Key. |
| Stable package → X25519 private | A20:L820-L827: "`X25519_from_scalar(clamp(HKDF-Expand(identity_seed, ...)))`" | Topologically acyclic. N-20 remains because Extract/PRK semantics are absent. |
| X25519 private → `VK_e` | A20:L526-L533: HPKE-open the grant whose info/AAD includes `roster_hash`. | **Impossible as specified** because N-33 prevents producing the grant. |
| Current VK → historical keys | A20:L767-L782: "`VK_cur → ... → VK_0`" via archive records rooted in the newer key. | Directionally acyclic; N-21/N-22 still leave addressing and commit representation incomplete. |

The split therefore solves the **local symmetric circularity** from round 2:
opening the stable package never depends on a Vault Key, and opening the HPKE
grant does not cryptographically require a Vault Key. It fails later at the
public transcript: the grant requires the final directory hash that depends on
the grant.

### Can an admin produce a grant?

Ignoring N-33, yes. A20:L488-L493 correctly narrows the inputs:

> Because the admin needs only the member's **public** X25519 key, an admin can
> (re-)grant `VK_{e+1}` ... without any other member's
> KEK/passphrase/Secret Key.

The admin has:

- its own current `VK_e`;
- a fresh generated `VK_{e+1}`;
- each remaining member's X25519 public key from the signed directory; and
- an authorized Ed25519 signing key.

No recipient secret is required. That is the right public-key distribution
model. The unknown self-referential `new_roster_hash` is the sole mechanical
input the admin cannot obtain.

### Unforgeability, substitution, and replay

If N-33 is replaced with a non-circular context, the intended bindings are
directionally strong:

- `vault_id`, `member_id`, `epoch`, and the state commitment appear in HPKE
  `info` and AAD (A20:L526-L529).
- `admin_sig` covers all grant-body fields, including `hpke_enc` and `hpke_ct`
  (A20:L513-L523).
- `member_x25519` comes from a signed directory, not a bare server value
  (A20:L529-L531).

Consequences under a correct codec and state commitment:

- Lifting A's ciphertext to B fails because B has a different HPKE private key.
- Relabeling the vault, member, epoch, or state makes HPKE AAD verification or
  the admin signature fail.
- A server cannot forge a new grant without the admin signing key.
- Replaying a whole old directory remains detectable only relative to a
  trusted roster/head watermark. A full-loss recovery that has no surviving
  device anchor still needs an explicit freshness rule.

Those properties remain `[Validation Required]`; no canonical grant-signature
body, roster body, or deterministic HPKE vector exists.

### Offline members, revoked members, and atomicity

The intended asynchronous behavior is correct:

- A20:L534-L535: an offline member's grant "simply waits there until they next
  sync."
- A20:L536-L537: a revoked member receives no grant for `e+1`.
- A20:L670-L679 honestly states that revocation is forward-only and does not
  erase data already readable under old keys.

K.16 is still not resolved. A20:L1074 leaves `archive_ref` undefined,
A20:L1078 uses a mutable-looking `completion = 0/1` without an immutable
prepared-to-committed representation, and N-33 prevents constructing
`new_roster_hash`. A20:L1087-L1089 therefore overclaims:

> A crash before that leaves the vault observably at `e` ... no half-rotated
> state ... (closes K.16).

### `identity_seed` blast radius

The remediation states the expansion honestly. A20:L538-L545 says:

> anyone who learns `identity_seed` ... can open every `VK_e` granted to that
> member and thus all content of those epochs.

That compromise also yields both stable member identity private keys and any
authorization power attached to the member role. Because current-key archive
access walks backward, one granted current key can expose all connected
history. This is a real widening from the symmetric-only round-1 package and
requires human approval `[Validation Required]`.

### N-15 verdict

| Item | Verdict |
|---|---|
| Stable self-wrap independent of `VK_e` | **RESOLVED as a component** |
| Admin ability using public recipient keys | **RESOLVED as a design direction** |
| Constructible per-epoch grant | **NOT-RESOLVED (N-33)** |
| N-15 overall | **NOT-RESOLVED** |
| N-01 archive distribution | **NOT-RESOLVED** |
| K.2 wrap constructions | **PARTIALLY-RESOLVED** |
| K.12 recovery | **NOT-RESOLVED** |
| K.16 rotation atomicity | **NOT-RESOLVED** |

## N-18/N-32 - invitation completeness and freshness

### Completeness: round-2 N-18, revised label N-32

A20:L626-L637 now defines:

> `invite_pt = VK_e(32) ‖ NS_objectid(32) ‖ freshness_anchor`

The invitee can keep its locally generated `identity_seed`, receive the shared
namespace key, and form `NS_objectid ‖ identity_seed`. The missing material
from round 2 is present.

**Verdict: RESOLVED at paper-field level.**

### Freshness: round-2 N-32, revised label N-18

A20:L631-L634 seals:

> `epoch(u32 LE) ‖ roster_hash(32) ‖ transition_hash(32) ‖
> frontier_or_checkpoint_id(32) ‖ head_counter(u64 LE)`

A20:L639-L648 signs the recipient and state:

> `vault_id ‖ invite_id ‖ recipient_fingerprint ‖ hpke_enc ‖
> hpke_ct_digest ‖ role ‖ not_after ‖ epoch ‖ roster_hash ‖
> transition_hash ‖ frontier_or_checkpoint_id ‖ head_counter`

and defines:

> `hpke_ct_digest = SHA-256(hpke_enc ‖ hpke_ct)`

The invitee verifies the signature, expiry, digest, and equality of sealed and
signed anchor fields. A21:L415-L425 consistently points new-member bootstrap
at this invitation rather than a server-supplied checkpoint.

This prevents:

- modifying or swapping the HPKE ciphertext without invalidating the digest
  and admin signature;
- moving the invitation to another signed `vault_id`, role, state, or
  recipient fingerprint;
- lifting a ciphertext to a different X25519 recipient, because HPKE open
  fails; and
- forging a newer or different anchor without the admin key.

It does **not** justify A20:L639-L641's absolute claim:

> a malicious server cannot replay an older self-consistent invitation

The exact signed bytes remain valid for the same recipient until `not_after`.
There is no consumed-`invite_id` store, one-time acceptance transition, or
recipient-side prior watermark. A server can replay the old invitation and
matching old checkpoint during that window. That is bounded authenticated
withholding, not signature forgery, but it is still replay.

Additional byte and semantic gaps remain:

- invitation HPKE `info` and AEAD AAD are unspecified;
- `invite_id`, `recipient_fingerprint`, `role`, and `not_after` have no
  canonical widths or encodings;
- clock source, skew, and expiry failure rules are absent;
- one 32-byte `frontier_or_checkpoint_id` cannot directly encode a multi-head
  frontier unless it is explicitly a commitment; and
- the enrollment state machine still does not say how the invitee's self-wrap
  becomes part of an admin-signed final directory (N-16).

### Invitation verdicts

| Finding | Round-3 verdict | Reason |
|---|---|---|
| N-18 (round-2 ID: `NS_objectid`) | **RESOLVED** | The sealed plaintext carries it. |
| N-32 (round-2 ID: freshness) | **PARTIALLY-RESOLVED** | Authenticated issuance anchor exists; exact same-recipient replay and canonical transcript remain. |
| N-08 recipient substitution | **RESOLVED as a design direction** | Out-of-band key fingerprint plus signed ciphertext binding; exact encoding remains K.3 work. |
| N-07 / I.6 bootstrap freshness | **PARTIALLY-RESOLVED** | New-member anchor exists, but exact replay and full-loss recovery freshness remain. |
| K.3 invitation transport | **PARTIALLY-RESOLVED** | Primitive and fields are selected; full bytes and replay state are not. |

## N-06 - real pre-auth Argon2 path

The paper rule is correct. A20:L1160-L1166 requires:

> rejects any triple outside a small compiled allowlist/cap ... before calling
> `open_bundle`

A20:L1168-L1174 also now discloses that both real openers remain vulnerable and
tracks the implementation work in #121. Deferring the code edit to A1 is
honest for a paper-spec remediation; it does not make the repository safe or
clear the implementation gate.

The defect is present in both paths:

- **`open_bundle`:** C:L487-L503 parses unauthenticated header values;
  C:L248-L260 consumes them in `Params::new` and `derive_key`. AEAD
  authentication occurs only at C:L277-L285.
- **`open_blob`:** C:L361-L390 parses the same values; C:L396-L404 consumes
  them in `Params::new` and `derive_key`. Authentication occurs only at
  C:L418-L431.

A20:L1176-L1178 is internally stale:

> `open_bundle` — the only place legacy Argon2 params are read

`open_blob` is the second place, as the preceding implementation-task note
itself says.

**Verdict:** the FONDENC2 reject-before-derive design is correct; **N-06 remains
NOT-RESOLVED for the repository and implementation gate** until both openers
enforce and test the legacy allowlist before `Params::new`. No Argon2 values
are invented here.

## Six-table honesty sweep

The full 55-row ledger in the next section is the canonical 1:1 reconciliation.
The following table lists every status-table mismatch or stale prose claim.
Rows not listed here agree with the ledger or are explicitly qualified as
direction-only rather than "resolved."

| Source | Exact current claim | Honest round-3 disposition |
|---|---|---|
| A20:L197 | N-15: "**Resolved (A0.5-r2)**" | **Overstated. NOT-RESOLVED** because N-33 makes the grant unconstructible. |
| A20:L198-L199 | N-18 is freshness; N-32 is `NS_objectid` delivery | **Identifiers inverted** from R2. Preserve R2 IDs: N-18 resolved; N-32 partial. |
| A20:L200 | N-01: "**Resolved (A0.5-r2)**" | **Overstated. NOT-RESOLVED** because current-key distribution fails. |
| A20:L216, L747 | K.12: "**Resolved (A0.5-r2)**" | **Overstated. NOT-RESOLVED** because recovery requires the unconstructible grant. |
| A20:L751 | K.16: "**Partially ... transition now constructible**" | **Overstated. NOT-RESOLVED**; N-33, N-21, and N-22 prevent construction/commit. |
| A20:L753-L759 | Keeps K.1/K.4/K.16 "at *partial*" and "closes K.2 ... K.12" | **Overstated.** K.1/K.4/K.12/K.16 are NOT; K.2 is PARTIAL. |
| A20:L879-L880 | K.16 partial and N-15 resolved | **Overstated.** Both depend on the impossible `new_roster_hash`; grades are NOT. |
| A20:L882 | VR-020-K13.5: "Partially" | **Overstated. NOT-RESOLVED** because no actual accepted sets or ceilings exist. |
| A20:L1087-L1089 | crash behavior "closes K.16" | **Overstated.** The immutable commit representation and constructible roster hash are absent. |
| A20:L1216-L1226 | K.16 and VR-020-K13.5 partial; K.2 treated as closed construction | **Overstated** for K.16 and K13.5; K.2 remains partial. |
| A21:L206 | N-07: "**Resolved (A0.5-r2)**" | **Overstated. PARTIALLY-RESOLVED** because exact invitation replay/full-loss freshness remain. |
| A21:L211, L636 | I.6: "**Resolved (A0.5-r2)**" | **Overstated. PARTIALLY-RESOLVED** for the same reason. |
| A21:L415-L425 | Invitation "closes the first-sync rollback window" | **Overstated.** It authenticates issuance state but does not consume the invite or establish global latest state. |
| A21:L686-L696 | ADR-021.2 mapping | **Aligned when read row-wise.** The combined F.1-F.6 "Decided" row is not a single resolved grade: F.4 is partial and F.5 is not resolved. |
| A21:L983-L986 | F.1-F.6/F.5 and I.16 "no longer block" | **Overstated.** F.4 remains PARTIAL, F.5 remains NOT, and I.16 remains PARTIAL. |
| A21:L990-L993 | I.12/I.13/I.14 "**Resolved**"; I.11/I.16 "Decided" | **Stale and contradictory.** The authoritative §I table correctly grades I.12/I.13 partial, I.14 not, I.11 not, and I.16 partial. |
| A21:L183-L187, L1000-L1004 | Composition is novel and `[Validation Required]` | **Understated grade elsewhere:** this genuinely closes VR-0212-B.1's honesty issue, so that row moves PARTIAL → RESOLVED. |

The six status tables audited were:

1. A20:L195-L217, FONDENC2 remediation mapping.
2. A20:L734-L751, K.1-K.16 status.
3. A20:L877-L888, A0.3 remediation mapping.
4. A21:L201-L214, ADR-021.1 remediation mapping.
5. A21:L629-L646, I.1-I.16 status.
6. A21:L686-L696, ADR-021.2 remediation mapping.

## Complete 55-row round-3 reconciliation

| # | ID | Round 2 | Round 3 | Round-3 evidence |
|---:|---|---|---|---|
| 1 | K.1 | NOT | **NOT** | A20:L323-L331 still requires an unpublished cross-implementation two-secret KAT. |
| 2 | K.2 | PARTIAL | **PARTIAL** | The split is sound directionally; N-33 and canonical AAD/codec gaps prevent a complete grant. |
| 3 | K.3 | PARTIAL | **PARTIAL** | Payload/state fields improved; replay state, HPKE `info`/AAD, and canonical bytes remain. |
| 4 | K.4 | NOT | **NOT** | A20:L451-L452 still uses `HKDF-Extract(_, MUK)` and §F concatenation rules remain inconsistent. |
| 5 | K.5 | RESOLVED | **RESOLVED** | Random 192-bit XChaCha nonce decision unchanged. |
| 6 | K.6 | RESOLVED | **RESOLVED** | One independently mergeable record per object remains decided. |
| 7 | K.7 | PARTIAL | **PARTIAL** | Device-key model is correct; certificate transcript/enrollment transition remain incomplete. |
| 8 | K.8 | PARTIAL | **PARTIAL** | Per-admin model is selected; unsigned canonical body and role transitions remain incomplete. |
| 9 | K.9 | RESOLVED | **RESOLVED** | Lazy re-encryption remains optional and honestly forward-only. |
| 10 | K.10 | PARTIAL | **PARTIAL** | Sidecar/roster link and identity lifecycle remain incomplete. |
| 11 | K.11 | PARTIAL | **PARTIAL** | Width fixed; `len_prefix`, class taxonomy, and A4 dependency remain. |
| 12 | K.12 | NOT | **NOT** | Recovery route improved conceptually, but N-33 prevents constructing the required grant. |
| 13 | K.13 | NOT | **NOT** | **DEFER-TO-HUMAN** pending measured device evidence. |
| 14 | K.14 | RESOLVED | **RESOLVED** | Explicit transactional profile lifecycle remains. |
| 15 | K.15 | RESOLVED | **RESOLVED** | Retain-by-default/best-effort-delete decision remains. |
| 16 | K.16 | NOT | **NOT** | N-33, undefined `archive_ref`, and prepared/committed representation remain. |
| 17 | VR-020-D.1 | RESOLVED | **RESOLVED** | No SRP fallback. |
| 18 | VR-020-K13.1 | NOT | **NOT** | Desktop values remain illustrative. |
| 19 | VR-020-K13.2 | NOT | **NOT** | Mobile/watch values remain illustrative. |
| 20 | VR-020-K13.3 | NOT | **NOT** | Measurement evidence remains absent. |
| 21 | VR-020-K13.4 | RESOLVED | **RESOLVED** | Random 16-byte salt remains decided. |
| 22 | VR-020-K13.5 | NOT | **NOT** | No concrete platform accepted sets or ceilings are pinned. |
| 23 | I.1 | PARTIAL | **PARTIAL** | Width fixed; prefix encoding and taxonomy remain open. |
| 24 | I.2 | NOT | **RESOLVED** | A20:L631-L637 now delivers the vault-lifetime `NS_objectid` to a new member. |
| 25 | I.3 | PARTIAL | **PARTIAL** | DAG topology is correct; codec and state machine remain open. |
| 26 | I.4 | PARTIAL | **PARTIAL** | Per-device ownership correct; certificate/enrollment transition incomplete. |
| 27 | I.5 | NOT | **NOT** | Canonical full-record serialization remains absent. |
| 28 | I.6 | NOT | **PARTIAL** | Invitation authenticates an issuance anchor, but exact replay and frontier semantics remain. |
| 29 | I.7 | RESOLVED | **RESOLVED** | MAC'd state outside `fond.db` plus explicit re-trust remains. |
| 30 | I.8 | RESOLVED | **RESOLVED** | Hard-stop and quarantine remains. |
| 31 | I.9 | PARTIAL | **PARTIAL** | Checkpoint/tombstone representation remains open (N-24). |
| 32 | I.10 | NOT | **NOT** | **DEFER-TO-HUMAN** for exact `opaque-ke`/`voprf` version and feature review. |
| 33 | I.11 | NOT | **NOT** | **DEFER-TO-HUMAN** for exact types/features and fixed-salt adapter. |
| 34 | I.12 | PARTIAL | **PARTIAL** | Sidecar transcript and actual roster reference remain incomplete. |
| 35 | I.13 | PARTIAL | **PARTIAL** | Trust principle is corrected; continuity bytes remain incomplete. |
| 36 | I.14 | NOT | **NOT** | Canonical operation payloads, role matrix, and replay durability remain unspecified. |
| 37 | I.15 | RESOLVED | **RESOLVED** | No SRP fallback. |
| 38 | I.16 | PARTIAL | **PARTIAL** | OPAQUE normalization and exact adapter-label placement remain open. |
| 39 | VR-0212-B.1 | PARTIAL | **RESOLVED** | A21:L183-L187 and L1000-L1004 now explicitly call the composition novel and `[Validation Required]`. |
| 40 | VR-0212-C.0 | RESOLVED | **RESOLVED** | `opaque-ke` remains the sole PAKE path. |
| 41 | VR-0212-C.1 | RESOLVED | **RESOLVED** | Audit provenance remains correctly limited to v0.5.0 lineage. |
| 42 | VR-0212-C.2 | RESOLVED | **RESOLVED** | RFC vectors remain described as conformance evidence, not an audit. |
| 43 | VR-0212-C.3 | RESOLVED | **RESOLVED** | Current `voprf` remains explicitly outside the 2021 audit coverage. |
| 44 | VR-0212-C.4 | RESOLVED | **RESOLVED** | VOPRF vectors remain qualified as conformance evidence. |
| 45 | VR-0212-C.5 | RESOLVED | **RESOLVED** | SRP crate audit limitation remains stated. |
| 46 | VR-0212-F.1 | RESOLVED | **RESOLVED** | OPAQUE-3DH remains pinned. |
| 47 | VR-0212-F.2 | RESOLVED | **RESOLVED** | ristretto255 remains pinned. |
| 48 | VR-0212-F.3 | RESOLVED | **RESOLVED** | SHA-512 remains pinned. |
| 49 | VR-0212-F.4 | PARTIAL | **PARTIAL** | Exact crate types and lengths remain I.10/I.11 work. |
| 50 | VR-0212-F.5 | NOT | **NOT** | Fixed-salt Argon2 KSF adapter remains outside RFC vectors and unvalidated. |
| 51 | VR-0212-F.6 | RESOLVED | **RESOLVED** | RFC nonce/seed lengths remain pinned. |
| 52 | VR-0212-F.7 | RESOLVED | **RESOLVED** | Distinct domain-specific profile IDs remain. |
| 53 | VR-0212-F.8 | RESOLVED | **RESOLVED** | Record-only versus record-plus-OPRF-seed compromise remains honestly qualified. |
| 54 | VR-0212-F.9 | RESOLVED | **RESOLVED** | Secret Key server boundary remains explicit. |
| 55 | VR-0212-F.10 | RESOLVED | **RESOLVED** | Metadata leakage remains explicit. |

### Round-3 count

| Verdict | Round 2 | Round 3 | Delta |
|---|---:|---:|---:|
| RESOLVED | 24 | **26** | **+2** |
| PARTIALLY-RESOLVED | 15 | **15** | **0** |
| NOT-RESOLVED | 16 | **14** | **-2** |
| **Total** | **55** | **55** | - |

## Deterministic vector readiness

The round-3 file is:

`test-vectors/fondenc2/round3-vectors.json`

SHA-256:

`fba615cc76eeb81c51c730ca785d7e905e57cacc0b76281e41c6d5a95dd1ffbd`

It contains only outputs justified by current bytes:

1. A **primitive-only** XChaCha20-Poly1305 vector for the revised 64-byte
   `NS_objectid ‖ identity_seed` stable package. Its AAD is deliberately opaque
   because canonical `len_prefix` remains undefined.
2. A **serialization-only** 172-byte invitation plaintext:
   `VK_e(32) ‖ NS_objectid(32) ‖ freshness_anchor(108)`.

The round-2 96-byte package vector is obsolete because it includes
`current_Vault_Key`; the stable package no longer does.

No HPKE output was invented.

| Requested vector | Round-3 readiness |
|---|---|
| Stable package plaintext/primitive wrap | **ILLUSTRATIVE ONLY.** The 64-byte shape and direct-key XChaCha primitive are computable; canonical MUK→KEK→AAD bytes are blocked. |
| Invitation plaintext | **SERIALIZATION-ONLY COMPUTABLE.** All sealed fields have fixed widths, producing 172 bytes. |
| Per-epoch `vk_grant` HPKE seal/open | **BLOCKED.** N-33 prevents choosing `roster_hash`; `len_prefix` and sender deterministic KEM input are absent; canonical signature body is undefined. |
| Complete invitation HPKE/signature | **BLOCKED.** HPKE `info`/AAD, deterministic sender input, fingerprint/invite/role/expiry encodings, frontier commitment, and consumed-invite state are absent. |
| Canonical stable wrap | **BLOCKED.** K.1 KAT, K.4 KEK Extract salt, K.13 profile, and `len_prefix` remain open. |
| Epoch archive/transition | **BLOCKED.** N-21/N-22 and N-33 prevent a complete transcript. |
| OPAQUE login | **BLOCKED / DEFER-TO-HUMAN.** K.13 and I.10/I.11 remain gates. |

The vectors are illustrative and non-normative. They are not evidence that the
protocol is secure, implementable, or interoperable.

## Claims requiring downgrade

The following claims must remain narrowed or marked `[Validation Required]`:

- A20:L188-L190 and L757-L759: N-15/K.2/K.12 are closed.
- A20:L575: the clear directory "exactly ... removes the cycle." It removes the
  confidentiality cycle but N-33 introduces a grant/hash cycle.
- A20:L639-L641: an old invitation "cannot" be replayed.
- A20:L1087-L1089: the current transition representation closes K.16.
- A21:L415-L425: the invitation closes the first-sync rollback window without
  qualification.
- A21:L985: I.16 no longer blocks.
- A21:L990-L993: I.12/I.13/I.14 are resolved.
- Any implication that `opaque-ke 4.0.1` or current `voprf` inherits the 2021
  v0.5.0 audit.
- Any claim that current K.13 figures or platform ceilings are approved.
- Any claim that standard primitives prove this novel composition secure.

## Final gate statement

**NO-GO for implementation.** The split member package is the right direction,
and `NS_objectid` delivery is fixed, but the new HPKE grant is circular and the
invitation supplies only bounded issuance freshness. N-06 also remains live in
both production openers. Canonical transcripts, roster semantics, durable DAG
state, archive commits, K.13 measurements, and OPAQUE dependency/adapter review
remain open.

This model-based advisory review does **not** close issue #120. A human
cryptographer must make and record the final decision.
