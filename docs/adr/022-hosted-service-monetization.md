# ADR-022: kafkade-hosted service (architectural posture)

**Status**: Proposed
**Date**: 2026-07-24
**Decision**: If kafkade offers a hosted deployment, it runs the **same OSS `fond-server`**
(ADR-021) with **zero-knowledge preserved** (kafkade can neither read nor recover user data). Hosting
is **optional**, **never gates core features** (`CONTRIBUTING`), and coexists with free file-sync
(ADR-012) and self-hosting. Whether to build hosting at all is **gated on a demand / unit-economics
decision**; *not building it* is an explicitly valid outcome.

> **Commercial detail is intentionally kept out of this public repository.** Pricing, unit-economics,
> conversion assumptions, competitive positioning, and the go/no-go analysis are maintained
> **privately** by the maintainer. This ADR records only the architectural posture.

## Context

ADR-021 makes a hosted offering technically possible without compromising zero-knowledge. This ADR
captures only the *architectural* stance of a potential kafkade-hosted tier. The full monetization
analysis (strategy, pricing, critique, alternatives, adversarial review) lives in the maintainer's
private notes to avoid publishing commercial strategy in a public repo.

## Decision

- kafkade-hosted, **if** pursued, is a deployment of the unmodified OSS `fond-server` — no
  forked/closed server (avoids ecosystem fragmentation).
- It is **optional convenience** only; core features never require it (`CONTRIBUTING`, Principle #1).
- **Zero-knowledge is preserved**: the only plaintext kafkade would hold is billing/operational
  metadata, disclosed to users. kafkade cannot recover a locked-out vault.
- The build is **gated**: a demand + unit-economics validation precedes any hosting work, and "do not
  build hosting" is a legitimate result. Tracked as the ZK-Sync "paid pilot" work item.

## Alternatives Considered

| Alternative | Rejected because |
|---|---|
| Server-gated core features to force upgrades | Forbidden by `CONTRIBUTING`; breaks local-first. |
| kafkade-hosted on a forked/closed server | Fragments the ecosystem; must run the same OSS `fond-server`. |
| Selling user data / analytics | Impossible under zero-knowledge and against Principle #2. |
| Publishing the full pricing/monetization strategy here | Commercial strategy does not belong in a public repo; kept in private notes. |

## Consequences

- Preserves the ADR chain and the ADR-021/023 cross-references without exposing commercial strategy.
- Any hosting work remains blocked on the private demand/economics decision.
- Zero-knowledge guarantees (ADR-019/020/021) are unchanged by the existence of a hosted tier.
- A clear, upfront "we cannot recover your vault" UX is required if hosting ever launches.
