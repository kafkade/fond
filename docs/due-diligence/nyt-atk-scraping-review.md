# Due Diligence: NYT Cooking & Cook's Illustrated/ATK Scraping Legality

**Date**: 2025-05-30
**Status**: Reviewed — Automated access prohibited by both services
**Related Issue**: [#16](https://github.com/kafkade/fond/issues/16)
**Related ADR**: [ADR-006](../adr/006-web-scraping.md)
**Roadmap Reference**: Section 18, Failure Mode F1

## Summary

Both NYT Cooking and America's Test Kitchen (Cook's Illustrated, Cook's Country)
**explicitly prohibit automated access, scraping, and systematic data
collection** in their Terms of Service. This prohibition applies regardless of
whether the access is for personal, non-commercial use. **fond must not build
authenticated scrapers for these services.**

The schema.org import path (`fond import url`) remains a viable alternative for
any publicly accessible recipe pages that include structured data.

## Documents Reviewed

| Document | URL | Effective Date |
|----------|-----|----------------|
| NYT Terms of Service | https://help.nytimes.com/hc/en-us/articles/115014893428-Terms-of-Service | January 20, 2026 |
| ATK Terms of Use | https://www.americastestkitchen.com/corporate-pages/terms-of-use | Current |
| fond ADR-006 | `docs/adr/006-web-scraping.md` | July 13, 2025 |
| fond Roadmap §18 | `ROADMAP.md` (Failure Mode F1) | — |

---

## NYT Cooking

### Relevant ToS Provisions

**Section 4.1(2)** — Prohibited Use:

> Without NYT's prior written consent, you shall not: use robots, spiders,
> scripts, service, software or any manual or automatic device, tool, or process
> designed to data mine or scrape the Content, data or information from the
> Services, or otherwise use, access, or collect the Content, data or
> information from the Services using automated means

**Section 4.1(3)** — AI/ML prohibition:

> use the Content for the development of any software program, model, algorithm,
> or generative AI tool, including, but not limited to, training or using the
> Content in connection with the development or operation of a machine learning
> or artificial intelligence (AI) system

**Section 4.1(4)** — Anti-circumvention:

> use services, software or any manual or automatic device, tool, or process
> designed to circumvent any restriction, condition, or technological measure
> that controls access to the Services in any way, including overriding any
> security feature, bypassing or circumventing any access controls or use limits
> of the Services

**Section 4.1(5)** — No caching/archiving:

> cache or archive the Content (except for a public search engine's use of
> spiders for creating search indices)

**Section 2.1** — Personal use only, with explicit AI carve-out:

> The contents of the Services are intended for your personal, non-commercial
> use. [...] Non-commercial use does not include the use of Content without
> prior written consent from The New York Times Company in connection with:
> (1) the development of any software program, model, algorithm, or other
> generative AI tool

**Section 4.2** — Enforcement:

> Engaging in a prohibited use of the Services may result in civil, criminal,
> and/or administrative penalties, fines, or sanctions against the user and
> those assisting the user.

### Analysis

The NYT ToS is unambiguous: **all automated access is prohibited** without prior
written consent. There is no personal-use exception for automated tools. Even
manual archiving is restricted. The prohibition covers:

1. Any automated tool that collects content from their services
2. Any caching or archiving of content
3. Any use for software development (including fond itself)
4. Circumventing access controls (including authentication barriers)

**Verdict: fond must not implement an NYT Cooking scraper.**

---

## America's Test Kitchen / Cook's Illustrated / Cook's Country

### Relevant ToS Provisions

**Section 1** — No Unlawful Use:

> You may not use the Site or Services for any purpose that is unlawful or
> prohibited by the Terms of Use, or to solicit the performance of any illegal
> activity or other activity which infringes the rights of ATK or others.
> Without limiting the foregoing, you may not attempt to gain unauthorized
> access to any portion of the Site or any other systems or networks connected
> to the Site by hacking or any other illegitimate means.

**Section 9** — Proprietary Rights:

> You acknowledge and agree that the content and materials available on the
> Site [...] are protected by copyrights, trademarks, service marks or other
> proprietary rights and laws and are the sole and exclusive property of ATK
> and that you have no rights of ownership and may only use the data and
> information subject to these Terms of Use

> ATK grants you a non-exclusive license to use the content and materials for
> which you have subscribed and paid for solely in accordance with these Terms
> of Use and your Service Subscription. Use of any of the materials for any
> purpose not expressly permitted in these Terms of Use is prohibited.

### Analysis

ATK's ToS is less explicit than NYT's about automated tools, but the
restrictions are still clear:

1. **Access is subscription-gated** — most recipe content requires paid access
2. **License is limited** — content may only be used "solely in accordance with
   these Terms of Use"
3. **No unauthorized access** — gaining access by means other than the normal
   user interface (i.e., hacking, scraping) is prohibited
4. **Use beyond what's expressly permitted is prohibited** — automated
   collection is not an expressly permitted use

While ATK doesn't use the word "scraping" explicitly, the combination of
"no unauthorized access," "use only as expressly permitted," and the
subscription-gated nature of their content makes automated scraping a clear
ToS violation.

**Verdict: fond must not implement an ATK/Cook's Illustrated scraper.**

---

## Impact on fond

### What This Means

Per ADR-006 and Roadmap Failure Mode F1, this is the expected outcome that fond
was architecturally prepared for:

> F1: **NYT/ATK block scraping** or ToS forbids it — Likelihood: High —
> Mitigation: Isolate in `fond-scrape`; schema.org fallback; document the gap;
> never circumvent

### What fond Can Do

1. **Schema.org import (`fond import url`)** — Any publicly accessible recipe
   page (no login required) that includes JSON-LD or microdata can be imported.
   This works with the long tail of food blogs, many of which use WordPress
   recipe plugins with schema.org markup.

2. **Manual copy-paste** — Users can manually create `.cook` files from recipes
   they have legitimate access to. This is a personal, non-automated activity
   that falls within normal fair use.

3. **Paprika import** — Users who have saved NYT/ATK recipes in Paprika can
   import them via `fond import paprika`. The user already had legitimate access
   to save those recipes in Paprika using Paprika's built-in clipping features.

### What fond Must Not Do

1. ❌ Build authenticated scrapers for NYT Cooking or ATK
2. ❌ Automate login/session management for these services
3. ❌ Cache or archive content from these services
4. ❌ Circumvent paywalls or access controls
5. ❌ Provide instructions or tools for users to scrape these services

### Schema.org Fallback Nuance

Some NYT and ATK recipe pages may include schema.org/JSON-LD structured data on
publicly accessible (non-paywalled) pages. Using `fond import url` on a publicly
accessible page that includes structured data is **different** from building an
authenticated scraper:

- No login or authentication involved
- No circumvention of access controls
- Same mechanism as a search engine indexing the page
- User manually provides a single URL (not batch/automated)

However, per the NYT ToS Section 4.1(2), even automated access to public pages
is technically prohibited. fond's `import url` command is user-initiated (one URL
at a time, manually provided) and extracts only what the site itself publishes as
structured data. This falls closer to "a user visiting a page and saving a
recipe" than to "automated scraping," but users should be aware of the ToS
restrictions.

---

## Recommendations

1. **Do not implement** NYT Cooking or ATK authenticated importers in any phase
2. **Document this limitation** in user-facing docs and the mdBook
3. **Keep the schema.org path** as the general-purpose web import mechanism
4. **Update ADR-006** status to note that F1 has been validated (ToS does forbid
   automation for both services)
5. **Consider Paprika bridge** — document that users can import NYT/ATK recipes
   they've already saved in Paprika via `fond import paprika`
6. **Re-review annually** — ToS may change; both companies may offer APIs or
   relaxed terms in the future
7. **Never circumvent** — this is a hard architectural constraint per ADR-006

## Legal Context

For completeness, the legal landscape around web scraping includes:

| Precedent | Relevance |
|-----------|-----------|
| *hiQ Labs v. LinkedIn* (9th Cir. 2022) | Scraping public data may not violate CFAA, but ToS breach is a separate claim |
| *Van Buren v. United States* (SCOTUS 2021) | CFAA "exceeds authorized access" narrowed to gate-based access controls |
| *NYT v. OpenAI* (S.D.N.Y. 2023, ongoing) | NYT actively litigating against automated use of its content |

The legal question is not "can we technically scrape?" but "should we, given the
ToS?" fond's answer, per ADR-006, is clear: **no**.
