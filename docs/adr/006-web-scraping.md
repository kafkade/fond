# ADR-006: Web Scraping Architecture — Isolated, Schema.org-First Import

**Status**: Proposed
**Date**: 2025-07-13
**Decision**: Put website import logic in an isolated `fond-scrape` crate, prefer schema.org/JSON-LD extraction, use site-specific fallbacks only when needed, and require the user's own locally stored credentials for authenticated sites.

## Context

Import is the product's biggest adoption lever, but not every valuable source comes as a tidy file export. Section 3 identifies schema.org recipe pages as an MVP path and authenticated sites such as NYT Cooking and Cook's Illustrated as important Beta sources.

This area is also the most legally and ethically sensitive part of the roadmap. Section 3.5 defines hard red lines: no piracy, no redistribution, only the user's own paid access, and no circumvention if a site's terms forbid automation. Section 18 records the concrete failure mode F1: a site may block scraping or its ToS may prohibit it, and fond must document that gap rather than route around it.

The architecture therefore has to protect both the codebase and the product's trust model. Scraping should be best-effort, explicitly online, isolated from the core domain, and easy to disable per site without destabilizing recipe storage, search, or cooking features.

## Decision

fond will isolate web import logic in a dedicated **`fond-scrape`** crate. That crate owns HTTP fetching, session management, schema.org/JSON-LD extraction, and site-specific fallback parsers.

The import flow is **schema.org-first**: if structured recipe data is present, use it. Only when structured data is missing or incomplete should a site-specific parser be used. Authenticated imports may use the **user's own credentials**, stored locally via the OS keychain, and any imported content remains local to the user's machine. If a site's terms or technical posture make automation unacceptable, fond will not circumvent them and will instead document the limitation.

## Rationale

- **Isolation**: brittle site-specific code is kept away from storage, domain, and CLI fundamentals.
- **Standards first**: schema.org covers a wide long tail of recipe websites with much less maintenance.
- **Local-first ethics**: credentials stay on the user's machine and imported content is not redistributed.
- **Operational simplicity**: `reqwest`, `scraper`, and keychain storage fit the single-binary architecture better than bundling a browser.
- **Legal safety**: the design leaves room to decline support for a source rather than encouraging circumvention.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Headless browser automation | Heavy, brittle, harder to distribute, and a poor fit for the single-binary local-first architecture. |
| Central hosted scraping service | Creates major redistribution and legal risk, violates local-first principles, and adds infrastructure the project does not want. |
| No scraping support at all | Drops a major user need, especially for subscription recipe services and long-tail websites. |
| Site-specific parsing only | Higher maintenance burden and unnecessary when structured schema.org data is often available. |

## Consequences

- Strong upside: website import breadth improves without contaminating the core application with brittle scraping code.
- Strong upside: legal and ethical boundaries are explicit in the architecture rather than being left to ad hoc implementation choices.
- Tradeoff: authenticated importers remain fragile and may break when sites change markup or policies.
- Tradeoff: some high-value sources may remain unsupported if their terms or technical behavior make automation unacceptable.
