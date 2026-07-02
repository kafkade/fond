# fond — Product Roadmap & Architecture Decision Records

> **Repository:** `kafkade/fond`
> **Language:** Rust (2021 edition)
> **Primary interface:** CLI-first (`fond`), multi-platform later
> **Document type:** Product roadmap + Architecture Decision Records
> **Status:** Active
> **Author:** kafkade
> **Date:** July 2025

---

## Document Conventions

**Complexity classification** (applied throughout):

| Symbol | Meaning |
|--------|---------|
| 🟢 | Straightforward — well-understood, low-risk, off-the-shelf |
| 🟡 | Moderate — some unknowns, needs design care |
| 🔴 | Hard — genuine technical or domain difficulty, prototype first |
| ⛔ | Blocked / out of scope — depends on something unavailable or legally fraught |

**Confidence tags:**

- `[Validated]` — backed by a known fact, existing tool, or established pattern.
- `[Validation Required]` — a reasonable assumption that must be confirmed with a spike before committing.

**Output philosophy:** This document *decides*. Where alternatives existed, one path is recommended and the rejected options are listed with a one-line reason. Enumeration without a recommendation is treated as an unfinished decision.

---

## Section 0: Framing, Clarifying Questions & Assumptions

### 0.1 What fond is

**fond** is a local-first, CLI-first, Cooklang-native personal cooking and recipe management application written in Rust, designed for a household (family-shared) rather than a single user. It treats your recipes as portable plain-text files you own forever, imports your existing collection from Paprika / NYT Cooking / Cook's Illustrated in minutes, and helps you actually *cook* — with realistic timelines that work backward from when you want to eat.

The name evokes a *fond* (the browned bits at the bottom of a pan — the flavor foundation of a sauce) and *fondness* (the memories and love embedded in a family's recipes). Taglines under consideration: *"The foundation of your kitchen,"* *"Build on your fond,"* *"Where cooking memories live."*

### 0.2 Clarifying questions (and the assumptions made in their absence)

A solo project cannot wait for answers, so each open question below is resolved with a default assumption tagged for later validation. These feed the Assumptions Table in §0.3.

1. **How many recipes does the primary user already have, and where?** → Assume 300–800 recipes split across Paprika and NYT Cooking. Import quality is therefore the #1 adoption lever. `[Validation Required]`
2. **Is the "family" co-located or distributed?** → Assume co-located household sharing one machine / one home server initially; remote sync is a Post-1.0 concern. `[Validation Required]`
3. **Do users want nutrition tracking?** → Assume *informational* nutrition (per-recipe estimates) is wanted, but diet/calorie *tracking* is explicitly a non-goal.
4. **iOS/Apple Watch — how important, how soon?** → Wanted, but only after the data model is stable. Native Apple work is Phase 5+.
5. **Will users tolerate a CLI as the primary interface for months?** → The primary persona (power user) will; everyone else waits for the web UI in Beta. `[Validation Required]`
6. **Cloud or no cloud?** → Local-first, no mandatory cloud. Sync, when it comes, is user-controlled (self-hosted or file-sync).

### 0.3 Assumptions Table

| # | Assumption | Basis | Confidence | Impact if wrong |
|---|-----------|-------|-----------|-----------------|
| A1 | Primary user owns 300–800 recipes in Paprika + NYT Cooking | Typical power-user collection size | `[Validation Required]` | Import effort under/over-built |
| A2 | Cooklang is an acceptable canonical recipe format | Open spec, growing ecosystem | `[Validated]` | Whole storage model shifts |
| A3 | `cooklang-rs` parser exists and is usable from Rust | Known community crate | `[Validation Required]` | Must write a parser (🔴, weeks of work) |
| A4 | Plain `.cook` files + SQLite index is the right storage | Calibre/Obsidian precedent | `[Validated]` | Re-architecture of persistence |
| A5 | Household shares one DB initially, not federated | Simplicity for solo dev | `[Validation Required]` | Multi-device sync moves earlier |
| A6 | Users will run a CLI through MVP/Beta | Persona is technical | `[Validation Required]` | Web UI must move to MVP |
| A7 | Paprika export format is reverse-engineerable | Community has done it | `[Validation Required]` | Import slips a phase |
| A8 | schema.org/Recipe covers most websites | Widely adopted standard | `[Validated]` | Per-site parsers proliferate |
| A9 | Authenticated scraping is legal for the user's own subscriptions | Personal-use, user's own creds | `[Validation Required]` | Drop NYT/ATK importers |
| A10 | USDA FoodData Central can be embedded offline | Public-domain dataset | `[Validated]` | Nutrition needs an API |
| A11 | Ingredient density tables can be assembled | Public cooking references | `[Validation Required]` | Volume↔weight conversion limited |
| A12 | SQLite + FTS5 is sufficient for search at this scale | Proven for <100k docs | `[Validated]` | Need external search engine |
| A13 | MIT license is acceptable (matches portfolio) | Consistency w/ toku | `[Validated]` | Re-license effort |
| A14 | Solo dev, ~10 hrs/week, no funding | Stated constraint | `[Validated]` | All timelines stretch |
| A15 | Photos stored on filesystem, content-addressed | Cooklang convention | `[Validated]` | Blob storage in DB |
| A16 | Timeline timing can be parsed from `~timer{}` + heuristics | Cooklang has timers | `[Validation Required]` | Manual timeline entry |
| A17 | Cross-platform Rust toolchain (Win/macOS/Linux) works | Rust is portable | `[Validated]` | Platform-specific builds |
| A18 | A single maintainer can sustain the project long-term | Open-source norms | `[Validation Required]` | Bus-factor / burnout risk |
| A19 | Pantry tracking will be *opt-in*, not mandatory | Tedium is the top failure mode | `[Validation Required]` | Rebuild grocery flow |
| A20 | "Cooking memories" (notes/ratings) matter as much as data | Persona research, sentiment | `[Validation Required]` | De-prioritize journaling |

### 0.4 The eight foundational principles

Every decision in this document cascades from these. They are repeated as justifications throughout.

1. **User Data Ownership.** Recipes live as portable open-format `.cook` text files. No lock-in. Export is a no-op (the files *are* the export).
2. **Local-First / Offline-Capable.** The app fully works with no network. The kitchen has bad Wi-Fi; the app does not care.
3. **Family-Shared by Design.** Multi-user is baked into the data layer from day one, not retrofitted.
4. **Cooklang-Native.** Lossless round-trip. We parse `@ingredient{}`, `#cookware{}`, `~timer{}`, and `---` metadata, and we re-emit them faithfully.
5. **Recipe Import as a Superpower.** Getting a user's existing collection in (Paprika, NYT, Cook's Illustrated, schema.org) in under 10 minutes is the single biggest adoption lever.
6. **Open Source & Community-Welcoming.** MIT-licensed, readable code, good docs, low contribution friction.
7. **CLI-First, Multi-Platform Later.** Nail the engine and data model in a CLI; layer web and native UIs on a proven core.
8. **Realistic Cooking Timelines.** Model the *whole* timeline — marination, resting, parallel tasks — and work backward from the target eat time.

### 0.5 Constraint guards

- **Solo developer, ~10 hrs/week.** Every phase ships 3–5 deliverables as vertical slices. Scope is cut aggressively before timelines are.
- **No funding.** Only free/open dependencies and free-tier CI.
- **Realism over ambition.** When in doubt, ship the smaller thing that a real person can use this week.

---

## Section 1: Personas

Seven personas, each mapped to the phase that first delivers them real value. The **Home Cook Power User** is the wedge — fond must win this person first or it wins no one.

### 1.1 Home Cook Power User — *primary wedge* 🟢

- **Who:** Cooks 4–6 nights a week, owns 300–800 recipes in Paprika and/or NYT Cooking, comfortable in a terminal, frustrated by subscription lock-in and clunky sync.
- **Jobs:** Get my whole collection out of Paprika *losslessly*; search it instantly; scale and cook from it; never lose it.
- **Switching trigger:** A flawless Paprika import (<10 min) **plus** a cooking timeline that actually helps at the stove.
- **Phase served:** **MVP (Phase 1)** — add/view/import/search.
- **Why first:** Highest pain, lowest UI dependency, will tolerate a CLI, and evangelizes loudly.

### 1.2 Meal Prep Planner 🟡

- **Who:** Plans a week of meals on Sunday, batch-cooks, hates mid-week "what's for dinner."
- **Jobs:** Plan a week, generate one consolidated grocery list, know what to prep ahead.
- **Switching trigger:** Meal planning + pantry-aware grocery lists.
- **Phase served:** **Phase 3 (Family Kitchen)**.

### 1.3 Recipe Collector / Archivist 🟡

- **Who:** Has recipes scattered across screenshots, bookmarks, inherited cards, and three apps.
- **Jobs:** Consolidate everything into one durable, owned, searchable archive.
- **Switching trigger:** Broad import coverage + reliable full-text search + photos.
- **Phase served:** **Beta (Phase 2–3)**.

### 1.4 Family Cook with Dietary Constraints 🔴

- **Who:** Cooks for a household with allergies / vegetarian / low-sodium needs.
- **Jobs:** Filter by dietary tags, track allergens, adapt recipes per family member.
- **Switching trigger:** Per-user dietary profiles + allergen flagging + substitution suggestions.
- **Phase served:** **Phase 3 (Family Kitchen)**. (Allergen accuracy is 🔴 — safety-critical, see §18.)

### 1.5 Baking Enthusiast 🔴

- **Who:** Bakes by weight, scales by baker's percentage, cares about pan sizes and hydration.
- **Jobs:** Scale dough non-linearly, convert volume↔weight by ingredient, respect pan geometry.
- **Switching trigger:** Baker's-percentage scaling + accurate density-based conversion.
- **Phase served:** **Phase 2 (scaling)**, deepened later. (Density data is 🔴, see §3A.)

### 1.6 Kitchen Novice 🟡

- **Who:** New to cooking, intimidated, needs structure and timing help.
- **Jobs:** Follow a guided cook with clear steps and timers; don't burn things.
- **Switching trigger:** TUI "cook mode" with step-by-step timeline.
- **Phase served:** **Phase 2 (Cook's Companion)**, fully realized in the web/native UIs.

### 1.7 Developer / Contributor 🟢

- **Who:** Open-source-minded Rustacean or Cooklang fan.
- **Jobs:** Read clean code, run tests easily, add an importer or parser, get a PR merged.
- **Switching trigger:** MIT license, good docs, clear architecture, fast test suite.
- **Phase served:** **From day one** (public repo, contributing guide).

### 1.8 Persona → phase summary

| Persona | First real value | Key dependency |
|---------|------------------|----------------|
| Power User | MVP / Phase 1 | Paprika import, search |
| Meal Prep Planner | Phase 3 | Meal planning, pantry |
| Collector/Archivist | Beta / Phase 2–3 | Import breadth, FTS, photos |
| Dietary Family Cook | Phase 3 | Profiles, allergens |
| Baking Enthusiast | Phase 2+ | Density scaling |
| Kitchen Novice | Phase 2 | TUI cook mode |
| Developer/Contributor | Day one | License, docs, tests |

---

## Section 2: Architecture

### 2.1 High-level shape

fond is a **layered local-first application** with a pure-Rust core and swappable front-ends.

```text
┌──────────────────────────────────────────────────────────┐
│  Front-ends (later phases)                                 │
│  CLI (fond)  │  TUI cook-mode  │  Web (Axum+HTMX)  │ Apple │
├──────────────────────────────────────────────────────────┤
│  Application / Service layer  (fond-core)                  │
│   recipe svc · pantry svc · planner svc · import svc ·     │
│   timeline engine · scaling engine · grocery svc           │
├──────────────────────────────────────────────────────────┤
│  Domain model (entities, value objects, Cooklang AST)      │
├──────────────────────────────────────────────────────────┤
│  Persistence:  .cook files (source of truth)               │
│                + SQLite/FTS5 (index, metadata, overlay)    │
│                + filesystem photos (content-addressed)     │
└──────────────────────────────────────────────────────────┘
```

The CLI is a thin shell over `fond-core`. Every future front-end (web, native) calls the same core, guaranteeing behavioral parity and protecting principle #7 (CLI-first, multi-platform later).

### 2.2 Crate layout

| Crate | Responsibility |
|-------|----------------|
| `fond` (bin) | CLI entry, arg parsing (clap), output rendering |
| `fond-core` (lib) | Services, orchestration, public API for all front-ends |
| `fond-domain` (lib) | Entities, value objects, units, Cooklang AST types |
| `fond-store` (lib) | `.cook` file IO + SQLite index + migrations |
| `fond-import` (lib) | Paprika, schema.org, NYT, Cook's Illustrated adapters |
| `fond-scrape` (lib) | HTTP fetch, auth sessions, schema.org extraction |
| `fond-timeline` (lib) | Dependency-graph scheduling engine |

Splitting `core`/`domain`/`store` keeps the timeline and scaling engines unit-testable without touching disk, and lets the web/native layers depend on `fond-core` only.

### 2.3 Data flow (the canonical loop)

1. **Source of truth = `.cook` files** in `~/fond/recipes/`. Editing a file by hand is a first-class workflow.
2. On any change (CLI write, import, or detected file edit), `fond-store` **parses** the `.cook` into the domain model and **upserts** a row into SQLite (metadata + FTS5 text).
3. Reads for *search/list/filter* hit SQLite (fast); reads for *the recipe itself* re-read or use the parsed cache. SQLite is a **derived index**, never the sole truth.
4. A `fond reindex` command can rebuild the entire SQLite DB from the files at any time — this is the disaster-recovery guarantee and the proof of principle #1.

This is the **Calibre / Obsidian model**: human-readable files you own, with a database for speed. `[Validated]` (precedent), storage decision detailed in ADR-002.

### 2.4 Local-first & offline

No feature in MVP–1.0 requires the network *except* import/scraping (which is inherently online and explicitly user-initiated). Search, planning, cooking, scaling, pantry, and grocery lists all run fully offline (principle #2).

### 2.5 Concurrency & the family

Because the household shares one SQLite DB (A5), writes use SQLite's WAL mode and short transactions. `.cook` file writes are atomic (write-temp-then-rename). Conflict handling for *true* multi-device concurrent edits is deferred to the sync phase (ADR — sync, Phase 7), where a CRDT-or-file-sync decision is made. For co-located single-machine use, OS-level file locking + WAL is sufficient. `[Validated]`

### 2.6 Platform strategy

Pure Rust core compiles on Windows, macOS, and Linux unchanged (A17). The CLI ships via `cargo-dist` (cross-compiled binaries + installers). The web UI (Phase 4) is served by the same binary (`fond serve`). Native Apple apps (Phase 5) wrap `fond-core` via a C ABI / UniFFI bridge — deferred precisely because that bridge is 🔴 and must not gate the core.

---

## Section 3: Data Sources, Import & Export

Import is the superpower (principle #5). The goal: a power user with 500 recipes in Paprika is fully migrated and searching in **under 10 minutes**.

### 3.1 Export — the trivial case 🟢

Because `.cook` files *are* the canonical store, **export is a no-op**: the user already has their data as portable text. fond additionally offers:

- `fond export --format json` — structured dump (domain model → JSON) for interop.
- `fond export --format paprika` — round-trip back to Paprika (best-effort) for the nervous switcher.
- Plain `cp -r ~/fond/` — the simplest, most honest backup story.

This is the strongest possible expression of principle #1 (data ownership): leaving fond costs nothing.

### 3.2 Import sources & approach

| Source | Mechanism | Complexity | Phase |
|--------|-----------|-----------|-------|
| **Paprika** (`.paprikarecipes`) | Reverse-engineered gzip+JSON archive | 🟡 | MVP |
| **schema.org/Recipe** (any site) | JSON-LD / microdata extraction | 🟢 | MVP |
| **NYT Cooking** | Authenticated scrape + schema.org | 🔴 | Beta |
| **Cook's Illustrated / ATK** | Authenticated scrape + site parser | 🔴 | Beta |
| **Plain `.cook` files** | Direct copy / `fond import file` | 🟢 | MVP |
| **Markdown / freeform text** | Heuristic parser → Cooklang draft | 🟡 | Beta |
| **Photos of recipe cards (OCR)** | OCR → text → parse | ⛔→🔴 | Research |

### 3.3 Paprika import (the MVP centerpiece) 🟡

The `.paprikarecipes` file is a ZIP of gzip-compressed JSON recipe objects (community-reverse-engineered; `[Validation Required]` via a spike on a real export). The adapter:

1. Unzips and gunzips each entry.
2. Maps Paprika fields → domain model (name, ingredients text, directions, notes, source, photo, categories, rating, times).
3. **Converts ingredient lines and directions into Cooklang**, attempting to annotate `@ingredients{}` and `~timers{}` where confidently parseable; falls back to plain step text where not (lossless: nothing is dropped).
4. Writes one `.cook` file per recipe; decodes embedded base64 photos to content-addressed files.
5. Indexes into SQLite.

A **dry-run mode** (`fond import paprika file.paprikarecipes --dry-run`) reports how many recipes parsed cleanly vs. need review, building trust before any file is written.

### 3.4 schema.org web import 🟢

`fond import url <URL>` fetches the page, extracts `application/ld+json` `Recipe` objects (falling back to microdata), and maps to the domain model. schema.org/Recipe is widely adopted (A8, `[Validated]`), so this single adapter covers a long tail of food blogs with no per-site code.

### 3.5 Authenticated scraping (NYT / Cook's Illustrated) 🔴

This is the legally and technically sensitive area. **Red lines (non-goals):**

- **No piracy.** fond only accesses content the user *already pays for*, using the **user's own credentials** entered locally.
- **No redistribution.** Scraped recipes stay in the user's local store; fond never uploads, shares, or publishes them.
- **Respect for terms.** This is `[Validation Required]` per-site; if a site's ToS forbids it, fond documents the limitation rather than circumventing it (see §18 failure mode).

Mechanism: a local authenticated session (cookies stored in the OS keychain), schema.org-first extraction, site-specific fallback parsers. Because sites change and may block automation, these importers are **best-effort** and isolated in `fond-scrape` so breakage never affects the core.

### 3.6 Import data-quality strategy

Every import produces a **review queue**: recipes that parsed perfectly are written immediately; ambiguous ones (unparseable quantities, missing yields) are flagged for `fond review`. This keeps the <10-minute promise (clean ones are instant) while preserving correctness.

### 3.7 Non-goals (red lines)

No piracy, no redistribution of others' content, no social network, no e-commerce/affiliate monetization, no diet/calorie *tracking* as a primary function. These are firm.

---

## Section 3A: Culinary Domain Complexity

Cooking software is deceptively hard. The difficulty is not the CRUD — it's the *domain*. This section maps the real complexity so the roadmap budgets for it honestly.

### 3A.1 Ingredient ontology 🔴

A recipe says "2 large eggs, beaten" and "1 cup all-purpose flour, sifted." Underneath:

- **Name normalization.** "scallion" = "green onion" = "spring onion." fond needs a canonical-name map with aliases so search, pantry matching, and grocery consolidation work. `[Validation Required]` for coverage.
- **Prep as modifier, not identity.** "beaten," "sifted," "diced" describe *state*, not a different ingredient. The ontology separates `ingredient` (eggs) from `prep` (beaten) so "eggs" matches the pantry regardless of prep.
- **Allergens.** Each canonical ingredient carries allergen flags (gluten, dairy, nuts, egg, soy, shellfish…). **Safety-critical** — see §18; fond surfaces allergen info but always disclaims it is not medical advice.
- **Substitution groups.** "butter→margarine→oil (baking-dependent)" — substitutions are context-sensitive (baking vs. sautéing) and ranked, never automatic.
- **Categories / aisles.** Each ingredient maps to a grocery aisle (produce, dairy, dry goods) for shopping-list grouping.

This is modeled as an **embedded reference dataset** (shipped with fond, user-extendable), not user-authored from scratch. Building it is incremental: MVP ships a small seed; it grows per phase.

### 3A.2 Unit systems & conversion 🔴

The hardest "simple" problem in cooking software.

- **Unit families:** volume (cup, tbsp, mL, L), weight (g, kg, oz, lb), count (each, dozen), vague ("a pinch," "to taste," "1 can"), temperature (°F/°C), and baker's percentage.
- **Volume↔weight requires ingredient-specific density.** 1 cup flour ≈ 120 g; 1 cup water = 236 g; 1 cup honey ≈ 340 g. There is **no general conversion** — it's per-ingredient (A11). fond ships a density table keyed to canonical ingredients (ADR-007).
- **Vague units don't convert.** "A pinch" stays "a pinch"; fond never fabricates a gram value. It carries them through and excludes them from scaling math where nonsensical.
- **"1 can" / "1 stick".** Package-unit aliases ("1 can tomatoes" ≈ 14 oz; "1 stick butter" = 113 g) handled via the ingredient dataset.

Recommendation: a typed `Quantity { value, unit, ingredient_ref }` with conversion routed through the density table only when both units and a density are known; otherwise conversion is refused gracefully (🟡 to get the engine right, 🔴 to get the *data* complete).

### 3A.3 Timeline modeling 🔴

Principle #8 — the differentiator. A real cooking timeline has:

- **Active vs. passive time.** 10 min chopping (active, you're busy) vs. 8 hr marinating (passive, you're free). Schedulers that sum all durations are useless; fond distinguishes them.
- **Task types:** active-prep, passive-prep (marinate/chill), active-cook, passive-cook (simmer/bake), rest (rest the steak, proof the dough).
- **Dependencies.** "Sauce must finish before plating"; "dough must rest 1 hr before rolling." Modeled as a **directed acyclic graph (DAG)** of steps with duration + type + dependencies (ADR-008).
- **Backward scheduling.** Given "dinner at 7:00 PM," compute the latest start for each task so everything finishes together — the marinade alarm fires this morning, the rice starts at 6:30.
- **Multi-recipe coordination.** Cooking three dishes for one meal means merging three DAGs and resolving oven/stove contention (🔴, later phase).

MVP ships *no* timeline; Phase 2 ships single-recipe backward scheduling from `~timer{}` annotations + heuristic parsing of step text (A16, `[Validation Required]`). Multi-recipe coordination is Phase 3+.

### 3A.4 Pantry / inventory model 🟡→🔴

- **Quantity normalization.** "I have flour" vs. "I have 2.3 kg flour" — fond supports both *presence* and *quantity* tracking, opt-in per ingredient (A19 — tedium is the #1 pantry failure mode, §18).
- **Expiration & par levels.** Optional expiry dates and "keep at least N" thresholds for staples.
- **Consumption deduction.** When you cook a recipe, fond *can* deduct used quantities — but only with **manual confirmation** (ADR-009), never silently, because automatic deduction drifts from reality fast.
- **Pantry coverage %.** "You have 9 of 12 ingredients for this recipe" — a high-value, low-tedium read that powers "what can I cook tonight?"

Design stance: **pantry is opt-in and presence-first.** A user who never enters quantities still gets coverage % from a simple have/don't-have list. This directly mitigates the tedium failure mode.

### 3A.5 Grocery list generation 🟡

- **Sources:** a single recipe, a meal plan, or ad-hoc additions.
- **Pantry-aware delta.** Subtract what's already in the pantry from what the recipes need.
- **Consolidation.** "1 onion" + "½ onion" across three recipes → "1½ onions" (requires unit normalization, §3A.2).
- **Aisle grouping.** Sorted by store section (from the ingredient ontology) so the list matches a shopping route.
- **Vague-unit handling.** Non-consolidatable items ("salt, to taste") listed once.

### 3A.6 Recipe scaling 🔴

- **Linear ingredients.** Most things scale linearly (2× recipe = 2× flour).
- **Sub-linear / invariant.** Salt, leavening, spices, and pan-coating fat often *don't* scale linearly — doubling a cake doesn't double the baking soda. fond flags these via the ingredient ontology and scales them conservatively (or warns). `[Validation Required]` for which ingredients.
- **Baker's percentage.** For doughs, scale by hydration %/flour weight, not by serving count.
- **Pan / equipment constraints.** Doubling a cake may exceed the pan; doubling a sauté may exceed the pan's surface and steam instead of sear. fond *warns* using pan-capacity heuristics (later phase).
- **Time doesn't scale linearly either.** 2× volume ≠ 2× cook time. fond does **not** auto-scale times; it flags that times may need adjustment.

MVP ships linear scaling with a warning that non-linear ingredients/times may need a human eye. Honest > wrong.

### 3A.7 Edge cases that break naive models

| Edge case | Why it's hard | fond's stance |
|-----------|---------------|---------------|
| Multi-component sub-recipes ("make the roux, then…") | Recipes contain recipes | Cooklang supports recipe refs; model as composable units |
| Technique references ("fold," "temper") | Knowledge, not data | Link to a glossary; don't model as steps |
| Equipment requirements (stand mixer, sous-vide) | Affects feasibility & timeline | `#cookware{}` tags drive an equipment list |
| Yield ambiguity ("serves 4–6") | Ranges, not numbers | Store ranges; scale from a chosen target |
| Altitude / climate adjustments | Physics-dependent | Out of scope (Research); document as known gap |
| "Cook until done" (no time) | Subjective doneness | Carry as a non-timed step; never fabricate a timer |

### 3A.8 Domain complexity summary

The CRUD is 🟢. The **ontology, density data, timeline DAG, and non-linear scaling are 🔴** and are the genuine engineering risk. The roadmap front-loads the *data model* to absorb this complexity and defers the hardest data-collection work (full density tables, multi-recipe coordination) to later phases.

---

## Section 4: Core Features

Grouped by the job they serve, tagged with release tier.

### 4.1 Recipe management (MVP)

- `fond add` / `fond edit` / `fond view` / `fond rm` — CRUD over `.cook` files. 🟢
- `fond list` / `fond search <query>` — FTS5-backed instant search. 🟢
- `fond tag` — categories, cuisines, free tags. 🟢
- Photos attached to recipes (content-addressed filesystem). 🟢
- Notes & ratings per recipe ("cooking memories," A20). 🟢

### 4.2 Import / export (MVP→Beta)

- Paprika import (MVP), schema.org URL import (MVP), NYT/Cook's Illustrated (Beta). 🟡🔴
- Export to JSON / Paprika / plain copy (MVP). 🟢

### 4.3 Cooking companion (Phase 2 / Beta)

- TUI **cook mode**: step-by-step with live timers. 🟡
- **Timeline**: backward-scheduled plan from a target eat-time. 🔴
- Recipe **scaling** (linear + warnings). 🔴

### 4.4 Planning & household (Phase 3 / Beta→1.0)

- **Meal planning** (assign recipes to days). 🟡
- **Pantry** (opt-in, presence-first). 🟡
- **Grocery lists** (pantry-aware, aisle-grouped, consolidated). 🟡
- **Family profiles** (per-user dietary prefs, allergens). 🔴
- **Scoreboard** (what you've cooked, frequency, ratings over time). 🟢

### 4.5 Nutrition (Phase 3, informational)

- Per-recipe estimates from USDA FoodData Central (offline subset, A10). 🟡
- **Non-goal:** calorie/diet *tracking*.

### 4.6 Interfaces (Phase 4+)

- Web UI (`fond serve`, Axum + HTMX). 🟡
- Native Apple apps (iOS/iPad/macOS/Watch). 🔴

### 4.7 Feature → tier matrix

| Feature | MVP | Beta | 1.0 | Post-1.0 |
|---------|:---:|:---:|:---:|:---:|
| Recipe CRUD + search | ✅ | | | |
| Cooklang round-trip | ✅ | | | |
| Paprika + schema.org import | ✅ | | | |
| Basic pantry (presence) | ✅ | | | |
| Shopping list (basic) | ✅ | | | |
| Web UI | | ✅ | | |
| Authenticated scraping | | ✅ | | |
| Meal planning | | ✅ | | |
| Scoreboard | | ✅ | | |
| Cooking timeline | | ✅ | ✅ | |
| Scaling (full) | | ✅ | ✅ | |
| Family profiles / dietary | | | ✅ | |
| Nutrition (informational) | | | ✅ | |
| Native Apple apps | | | | ✅ |
| Sync / multi-device | | | | ✅ |

---

## Section 5: Features the User May Have Missed

Capabilities not explicitly requested but that the domain and personas imply. Each is tagged with the phase it best fits so none derails the MVP.

| Feature | Why it matters | Tier |
|---------|----------------|------|
| **"What can I cook tonight?"** ranking | Pantry coverage % + time filter answers the most-asked kitchen question | Beta (built on pantry) |
| **Leftover / batch tracking** | Meal-preppers cook once, eat thrice — track cooked portions | Phase 3 |
| **Recipe provenance & attribution** | Where did this come from? (URL, person, book) — respects sources, aids trust | MVP (metadata field) |
| **"Made it" history with photos** | The "cooking memories" emotional core (A20) — photo + note per cook | Phase 2 |
| **Seasonality tags** | Surface asparagus recipes in spring — small data, big delight | Post-1.0 |
| **Equipment-aware filtering** | "No stand mixer tonight" — uses `#cookware{}` data already parsed | Phase 2 |
| **Shopping list check-off + aisle order** | Turns the list into a usable in-store tool | Beta |
| **Recipe versioning / edit history** | `.cook` files + git = free version history; surface it | Post-1.0 |
| **Print / share a single recipe (read-only)** | Hand a recipe to a friend without sharing the whole library | Beta |
| **Cook-mode "keep screen awake"** | Tiny UX detail that matters at the stove | Phase 2 (TUI/web) |
| **Conversational unit display toggle** | Show metric or US-customary per user preference | Phase 2 |
| **Duplicate detection on import** | Re-importing shouldn't create twins — idempotent import keyed by provenance | MVP (part of ADR-010) |

The unifying principle: most of these **fall out of data we already model** (pantry, cookware, provenance, photos). They are cheap *because* the data model was built deliberately — the payoff of front-loading §8.

---

## Section 6: Smart Features (AI-Assisted — Future)

All AI features are **Research-tier (Phase 6)** and explicitly gated behind a stable data model and offline-first guarantees. None is in the MVP. Each must degrade gracefully to a non-AI path.

### 6.1 Inventory-based recipe suggestions 🟡

- **Idea:** "You have chicken, lemon, and rice — here are 6 recipes you can make now."
- **Approach:** This is *not* AI to start — it's pantry-coverage ranking (§3A.4) sorted by % coverage and time. A genuine win with **zero ML**. AI only later improves ranking by taste history.
- **Stance:** Ship the deterministic version in Beta; defer any ML.

### 6.2 Ingredient substitution engine 🔴

- **Idea:** "Out of buttermilk? Use milk + lemon juice."
- **Approach:** A curated substitution dataset (context-aware: baking vs. sautéing) from the ingredient ontology (§3A.1), *not* a generative model. Rankings, never silent swaps.
- **Risk:** Wrong substitution in baking ruins a dish — must be advisory, sourced, and reversible.

### 6.3 Intelligent recipe scaling 🔴

- **Idea:** Scale non-linearly (leavening, salt, time) automatically.
- **Approach:** Rule-based using ingredient classification (§3A.6), not ML. AI could *suggest* adjustments but the engine stays deterministic and explainable.
- **Stance:** Linear + warnings in Phase 2; rule-based non-linear later; AI never owns correctness.

### 6.4 Automatic nutritional calculation 🟡

- **Idea:** Estimate per-serving nutrition.
- **Approach:** Deterministic sum over USDA FoodData Central (§9, A10) — *not* AI. Informational only; explicit non-goal to be a diet tracker.

### 6.5 OCR for handwritten / photo recipes ⛔→🔴

- **Idea:** Snap grandma's index card → editable recipe.
- **Approach:** OCR (Tesseract or a vision model) → text → Cooklang draft → review queue. Handwriting accuracy is genuinely hard (⛔ today for cursive); printed cards are tractable (🔴).
- **Stance:** Research-tier; always routes through the import review queue (ADR-010); never auto-saves.

**AI governance principle:** fond is local-first and offline-capable (#2). Any AI that requires the cloud is *optional and clearly labeled*; the core product must work fully without it. fond never sends a user's private recipes to a third party without explicit, per-action consent.

---

## Section 7: Reference App Analysis

Deep study of prior art using an **adopt / adapt / reject** framework — to learn, not to clone.

### 7.1 Paprika Recipe Manager

- **Adopt:** Excellent web clipper UX; clean recipe rendering; cross-device convenience expectation.
- **Adapt:** Its category/rating model → our tags + per-user ratings. Its sync → our (later) user-controlled sync.
- **Reject:** Closed binary format and paid proprietary sync (the lock-in fond exists to escape). Weak cooking-timeline support.
- **Takeaway:** Paprika is the incumbent to *migrate away from* — hence Paprika import is the MVP centerpiece (§3.3).

### 7.2 Cooklang ecosystem

- **Adopt:** The `.cook` format itself as our canonical store (principle #4); the CLI ethos; the open spec.
- **Adapt:** Extend Cooklang metadata for our richer model (ratings, pantry links) *without breaking round-trip*.
- **Reject:** Nothing — we are deliberately ecosystem-compatible. fond aims to be a *premier* Cooklang app, not a fork.
- **Takeaway:** Compatibility is a feature; contribute upstream where we extend (ADR-003).

### 7.3 NYT Cooking

- **Adopt:** Editorial quality bar for recipe presentation; the "cook mode" concept.
- **Adapt:** Their guided-cooking UX → our TUI/web cook mode (§6.3).
- **Reject:** Walled garden, zero data ownership, subscription lock-in.
- **Takeaway:** A key import source (Beta, authenticated, user's own subscription) — never a redistribution source (§3.5).

### 7.4 Mealie / Tandoor (self-hosted)

- **Adopt:** Self-hosting + family-sharing validation; meal-planning and shopping-list patterns; their schema.org import.
- **Adapt:** Their server model → our optional `fond serve` (Phase 4) rather than server-first.
- **Reject:** Server-first/DB-only architecture (we are files-first, local-first, CLI-first); Docker-as-prerequisite friction.
- **Takeaway:** Proof that family-shared self-hosted cooking apps have a real audience — and that a *files-first, CLI-first* alternative is an open niche.

### 7.5 Synthesis

No existing app is simultaneously files-first-owned, a genuine cooking aid, family-shared, and CLI-first/scriptable. fond's design borrows the best UX (Paprika clipper, NYT cook mode, Mealie planning) atop an ownership model (Cooklang files) none of the polished apps offer. Detailed positioning is in §10.

---

## Section 8: Data Model

### 8.1 Entities

| Entity | Key fields | Notes |
|--------|-----------|-------|
| **Recipe** | id (uuid v7), slug, title, source_url, yield_min, yield_max, prep/cook/total time, created, updated, cook_file_path | Mirrors a `.cook` file |
| **Ingredient (canonical)** | id, name, aliases[], category/aisle, allergens[], density_g_per_ml?, default_unit | Embedded reference dataset |
| **RecipeIngredient** | recipe_id, canonical_id?, raw_text, quantity, unit, prep, optional | Links recipe→ingredient; raw_text preserves Cooklang |
| **Step** | recipe_id, order, text, timers[], cookware[], duration?, task_type | task_type ∈ {active-prep, passive-prep, active-cook, passive-cook, rest} |
| **Cookware** | id, name | From `#cookware{}` |
| **Tag** | id, kind (cuisine/category/custom), name | |
| **Photo** | id, recipe_id, content_hash, path, is_primary | Content-addressed |
| **User (household member)** | id, name, dietary_prefs[], allergens[], is_active | Family-shared from day one |
| **Note** | id, recipe_id, user_id, text, created | "Cooking memories" |
| **Rating** | recipe_id, user_id, stars, created | |
| **CookLog** | id, recipe_id, user_id, cooked_at, servings | Powers the scoreboard |
| **PantryItem** | id, canonical_id, present, quantity?, unit?, expires?, par_level? | Opt-in quantity |
| **MealPlan** | id, name, owner_id | |
| **MealPlanEntry** | meal_plan_id, date, meal (b/l/d), recipe_id, target_servings | |
| **GroceryList** | id, source (recipe/plan/adhoc), created | |
| **GroceryItem** | grocery_list_id, canonical_id, quantity, unit, aisle, checked | |
| **NutritionFact** | canonical_id, per_100g {kcal, protein, fat, carb, …} | From USDA subset |
| **Migration** | version, applied_at | Schema versioning (refinery) |

### 8.2 Family-shared vs. per-user

The DB is **shared** (one household DB, A5). Data splits into:

- **Shared:** recipes, ingredients, tags, photos, meal plans, grocery lists.
- **Per-user (scoped by `user_id`):** notes, ratings, cook logs, dietary profiles, allergen sets.

This satisfies principle #3: multi-user is in the schema from day one, even though the MVP UI may default to a single active user. No retrofit later.

### 8.3 Storage layering (the hybrid)

```text
~/fond/
  recipes/            ← .cook files (SOURCE OF TRUTH)
    chicken-adobo.cook
    sourdough.cook
  photos/             ← content-addressed images
    a1/b2c3....jpg
  fond.db             ← SQLite index/overlay (DERIVED, rebuildable)
  config.toml
```

- `.cook` files own recipe content (principle #1, #4).
- SQLite owns *derived* search index (FTS5), *relational* overlays (ratings, pantry, plans, cook logs) that don't belong in a single recipe file, and the ingredient/nutrition reference data.
- `fond reindex` rebuilds `fond.db` entirely from files + bundled reference data. The DB is disposable; the files are sacred. (ADR-002.)

### 8.4 ERD (text)

```text
User 1───* Note *───1 Recipe
User 1───* Rating *──1 Recipe
User 1───* CookLog *─1 Recipe
Recipe 1──* RecipeIngredient *──? Ingredient(canonical)
Recipe 1──* Step
Recipe 1──* Photo
Recipe *──* Tag
Recipe *──* Cookware
Ingredient 1──? NutritionFact
Ingredient 1──* PantryItem
MealPlan 1─* MealPlanEntry *─1 Recipe
GroceryList 1─* GroceryItem *─? Ingredient
```

### 8.5 Search

SQLite **FTS5** over title + ingredients + steps + tags + notes (A12, `[Validated]`). At the assumed scale (<10k recipes) this is instant and needs no external engine. Filters (tag, cuisine, max-time, dietary, pantry-coverage) are SQL `WHERE` clauses joined to FTS results.

---

## Section 9: Design Brief & CLI

### 9.1 CLI principles

- **Verb-noun, discoverable, scriptable.** `fond <noun> <verb>` with sensible defaults.
- **Human output by default, `--json` for scripts.** Tables via `comfy-table`/`tabled`.
- **Fails loudly and helpfully.** Errors name the file/recipe and suggest a fix.
- **Offline-first.** Only `import url`/scrape touch the network.

### 9.2 Command surface (illustrative)

```bash
# Setup
fond init                                  # create ~/fond, db, config
fond reindex                               # rebuild fond.db from files

# Recipes
fond add                                   # interactive new recipe → .cook
fond add --file ./pie.cook                 # ingest an existing .cook
fond view chicken-adobo                    # render a recipe
fond edit chicken-adobo                    # open in $EDITOR
fond list --tag dinner --max-time 30
fond search "braised pork"
fond tag chicken-adobo --add filipino,weeknight
fond rate chicken-adobo 5
fond note chicken-adobo "Used less vinegar; perfect."

# Import / export
fond import paprika ~/Downloads/recipes.paprikarecipes --dry-run
fond import url https://cooking.nytimes.com/recipes/12345
fond export --format json > backup.json

# Cooking
fond cook chicken-adobo                     # TUI cook mode w/ timers
fond cook chicken-adobo --serve-at 19:00    # backward-scheduled timeline
fond scale sourdough --to 2x                # or --servings 8

# Planning & household
fond plan week --add monday:dinner=chicken-adobo
fond pantry add flour eggs "olive oil"
fond pantry check chicken-adobo             # coverage %
fond grocery from-plan week                 # pantry-aware, aisle-grouped
fond user add "Sam" --allergen peanut --diet vegetarian
fond scoreboard --since 2025-01-01
```text
### 9.3 TUI cook mode (Phase 2)

A full-screen terminal view: current step highlighted, upcoming steps queued, **live countdown timers** firing alerts, and the backward-scheduled timeline as a side rail ("start rice at 6:30"). Built with `ratatui`. This serves the Kitchen Novice and Power User alike.

### 9.4 Web & native (later)

Web (Phase 4): `fond serve` → Axum + HTMX server-rendered UI, same `fond-core`. Native (Phase 5): SwiftUI front-ends over a UniFFI binding. Both are *skins* on the proven core — no business logic leaks into the UI.

### 9.5 Visual identity

Warm, kitchen-evoking palette (caramel/fond-brown, cream); monospace-friendly CLI output; a simple pan-and-flame mark. Identity work is deliberately light until the web UI phase.

---

## Section 10: Competitive Analysis

### 10.1 The 2×2

Axes: **Data Ownership** (locked-in ↔ user-owned) and **Cooking Depth** (catalog-only ↔ active-cooking-aid).

```text
                 Active cooking aid
                        ▲
          Paprika ·     │        · fond (target)
        (some timers)   │      (timeline, scaling,
                        │       household, owned files)
  Locked-in ───────────┼──────────────► User-owned
                        │
        NYT Cooking ·   │     · Cooklang CLI / plain files
        AllRecipes ·    │       (owned but bare)
                        │
                 Catalog only
```

### 10.2 Competitor table

| Product | Strength | Weakness fond exploits |
|---------|----------|------------------------|
| **Paprika** | Polished, great clipper, cross-device | Closed format, paid sync, weak timelines, lock-in |
| **NYT Cooking** | Editorial quality | Walled garden, no ownership, subscription |
| **AllRecipes / Yummly** | Huge catalog | Ad-heavy, no ownership, no real cooking aid |
| **Cooklang CLI / plain files** | Open, owned, scriptable | Bare — no pantry, planning, timeline, household |
| **Notion / spreadsheets** | Flexible | No cooking domain logic at all |
| **Mela / Crouton (iOS)** | Lovely UX, some Cooklang | Apple-only, less CLI/automation, less household |

### 10.3 fond's wedge

fond is the only option that is **simultaneously** (a) user-owned open files, (b) a genuine cooking aid (timeline + scaling), and (c) family-shared and (d) CLI-first/scriptable. It out-owns Paprika and out-cooks plain Cooklang. The beachhead is the Power User escaping Paprika lock-in.

---

## Section 11: Licensing

**Recommendation: MIT license.** `[Validated]`

- **Why MIT:** Maximum adoption and contribution friction-reduction (principle #6); consistent with the rest of the portfolio (`toku`); simple and well-understood; lets the project be used and forked freely.
- **Rejected — GPL/AGPL:** Copyleft would protect against closed forks but deters casual contributors and corporate users, and adds friction for a hobby project seeking adoption. One-line reason: *adoption > protection here.*
- **Rejected — Apache-2.0:** Excellent (explicit patent grant) but heavier than needed for a recipe app; MIT's brevity wins for a small project. One-line reason: *unnecessary ceremony.*
- **Rejected — dual / source-available:** Adds legal complexity with no monetization plan. One-line reason: *no business model to protect.*

**Data & content note:** The MIT license covers fond's *code*. Users' recipes are their own. The bundled ingredient/nutrition reference data uses public-domain (USDA FoodData Central, A10) or permissively-licensed sources only; each is attributed in `THIRD_PARTY.md`. Scraped content is never redistributed (§3.5).

---

## Section 12: Technology Stack

Mirrors `toku`'s proven choices where sensible; deviations are justified.

| Concern | Choice | Rationale | Conf. |
|---------|--------|-----------|-------|
| Language | Rust 2021 | Performance, portability, safety, one binary | `[Validated]` |
| CLI parsing | `clap` v4 (derive) | De-facto standard, great help/UX | `[Validated]` |
| Cooklang parsing | `cooklang-rs` | Native spec support; avoid writing a parser | `[Validation Required]` |
| Storage (files) | `.cook` plain text | Ownership, round-trip (princ. #1, #4) | `[Validated]` |
| Storage (index) | SQLite via `rusqlite` | Embedded, zero-admin, ubiquitous | `[Validated]` |
| Search | SQLite **FTS5** | Built-in, fast at scale | `[Validated]` |
| Migrations | `refinery` | Versioned schema, matches toku | `[Validated]` |
| HTTP (scrape) | `reqwest` | Mature, async, cookie/session support | `[Validated]` |
| HTML/JSON-LD extract | `scraper` + `serde_json` | schema.org extraction | `[Validated]` |
| Serialization | `serde` | Universal | `[Validated]` |
| Table output | `comfy-table` / `tabled` | Clean CLI tables | `[Validated]` |
| TUI cook mode | `ratatui` | Mature TUI lib | `[Validated]` |
| IDs | `uuid` v7 | Time-ordered, sortable | `[Validated]` |
| Async runtime | `tokio` (minimal) | Only for scraping/serve | `[Validated]` |
| Web (Phase 4) | `axum` + HTMX | Server-rendered, light, same core | `[Validated]` |
| Keychain (auth) | `keyring` | Store scrape creds securely | `[Validation Required]` |
| Snapshot tests | `insta` | Golden tests for import/render | `[Validated]` |
| Native bridge (Ph5) | UniFFI | Rust→Swift bindings | `[Validated]` |
| Distribution | `cargo-dist` | Cross-platform binaries/installers | `[Validated]` |
| Docs | `mdBook` | Matches toku, easy hosting | `[Validated]` |
| CI | GitHub Actions | Free for OSS | `[Validated]` |

**Key deviation from toku:** addition of `ratatui` (cook mode), `scraper` (web import), `keyring` (auth), and the bundled USDA dataset — all driven by the cooking domain.

---

## Section 13: Phased Roadmap

Each phase ships 3–5 vertical-slice deliverables. Scope is cut, not timelines. Effort is in *calendar* terms at ~10 hrs/week.

### Phase 0 — First Recipe *(MVP groundwork)*

- **Theme:** Prove the spine end-to-end with one recipe.
- **Goal:** `fond init`, parse a `.cook` file, index to SQLite, `fond view` it.
- **Deliverables:** (1) crate scaffold + CI; (2) `.cook` parse via `cooklang-rs` spike; (3) SQLite schema + refinery; (4) `init`/`add --file`/`view`/`reindex`.
- **Acceptance:** Add a `.cook` file, see it rendered; `reindex` rebuilds DB from files.
- **Dependencies:** A3 (`cooklang-rs`) validated.
- **Risks:** `cooklang-rs` immature → fallback parser (🔴).
- **Cut line:** No search, no import yet.
- **Definition of done:** CI green on 3 OSes; one recipe round-trips losslessly.

### Phase 1 — Minimum Usable Kitchen *(MVP)*

- **Theme:** A power user lives in fond daily.
- **Goal:** Import the collection, search it, basic pantry + shopping list.
- **Deliverables:** (1) Paprika import (+dry-run/review); (2) schema.org URL import; (3) FTS5 search + filters + tags; (4) presence-based pantry + `pantry check` coverage; (5) basic grocery list from a recipe.
- **Acceptance:** Import 500 Paprika recipes in <10 min; search returns instantly; coverage % works.
- **Dependencies:** A7 (Paprika format), A1.
- **Risks:** Paprika format drift (§18) → isolate adapter.
- **Cut line:** No timeline, no web, no scaling.
- **Definition of done:** Power User persona fully served offline.

### Phase 2 — Cook's Companion *(Beta)*

- **Theme:** fond helps you actually cook.
- **Goal:** Timeline engine, TUI cook mode, scaling, scoreboard, notes/ratings.
- **Deliverables:** (1) timeline DAG + backward scheduling (single recipe); (2) `ratatui` cook mode w/ live timers; (3) linear scaling + non-linear warnings; (4) notes/ratings/cook-log + scoreboard.
- **Acceptance:** `fond cook X --serve-at 19:00` produces a correct backward schedule; cook mode fires timers.
- **Dependencies:** A16 (timer parsing).
- **Risks:** Prose timing unparseable (§18) → manual timeline entry fallback.
- **Cut line:** Single-recipe timelines only; no multi-recipe coordination.
- **Definition of done:** Novice + Baking + Power personas get cooking value.

### Phase 3 — Family Kitchen *(Beta→1.0)*

- **Theme:** The whole household.
- **Goal:** Family profiles, dietary/allergen filtering, meal planning, NYT/Cook's Illustrated import, informational nutrition.
- **Deliverables:** (1) `user` profiles + per-user notes/ratings; (2) dietary/allergen filters + flags; (3) meal planning + pantry-aware consolidated grocery lists; (4) authenticated NYT/Cook's Illustrated importers; (5) USDA nutrition estimates.
- **Acceptance:** Plan a week, generate one aisle-grouped pantry-aware list; filter recipes by allergen.
- **Dependencies:** A9 (legal scraping), A10/A11 (data).
- **Risks:** Scraping blocked/illegal (§18) → document limitation, keep schema.org path.
- **Cut line:** Nutrition is estimate-only; allergens disclaimed.
- **Definition of done:** Meal-Prep + Dietary-Family personas served; data model declared **stable** → 1.0.

### Phase 4 — Web Interface *(Post-1.0)*

- **Theme:** Beyond the terminal.
- **Goal:** `fond serve` web UI for non-CLI household members.
- **Deliverables:** (1) Axum+HTMX read/search UI; (2) recipe view + cook mode in browser; (3) meal plan + grocery UI.
- **Acceptance:** A non-technical family member uses fond entirely in a browser.
- **Risks:** Scope creep into a SPA → stay server-rendered.
- **Cut line:** No multi-tenant/cloud; LAN/self-host only.

### Phase 5 — Native Apple Apps *(Post-1.0)*

- **Theme:** Kitchen-native devices.
- **Goal:** iOS/iPad/macOS/Watch over UniFFI.
- **Deliverables:** UniFFI binding; SwiftUI recipe + cook-mode app; Watch timers.
- **Risks:** UniFFI bridge complexity (🔴); App Store + sync expectations.
- **Cut line:** Read/cook first; editing later.
- **Progress:** Foundation landed — `fond-ffi` UniFFI crate (read + cook mode) plus a multiplatform SwiftUI proof-of-concept (iOS + macOS) under [`apple/`](apple/); see [ADR-011](docs/adr/011-native-apple-bridge.md). A watchOS companion now surfaces active cook timers, wrist haptics/notifications when a step timer fires, a "Next up" complication/Smart Stack widget, and start/pause/advance controls — phone-relayed over WatchConnectivity, see [ADR-014](docs/adr/014-watch-companion-relay.md). iPad layouts landed. Native **editing** now landed too: create/edit/delete recipes and attach photos from the apps, writing back to the canonical `.cook` files with a lossless Cooklang round-trip (structured edit layer in `fond-domain`, single-recipe write + reindex in `fond-store`, optimistic-concurrency guard over the FFI), keeping the SQLite index in sync.

### Phase 6 — Smart Features *(Research)*

- OCR of recipe cards; AI substitution suggestions; smart "what can I cook?" ranking. All 🔴/⛔ until earlier phases are solid.

### Phase 7 — Sync & Multi-Device *(Research→Post-1.0)*

- **Goal:** Optional, user-controlled sync across devices.
- **Recommendation:** Evaluate **file-based sync** (the `.cook` files already sync via Dropbox/Syncthing/git) **before** CRDTs; if relational overlays need merging, evaluate `cr-sqlite` (as toku did). `[Validation Required]`
- **Decided:** see [ADR-012](docs/adr/012-sync-multi-device.md) and the [sync research](docs/research/sync-multi-device-strategy.md) — file-sync first for `.cook`+photos (Tier 1, **Validated**); authored-overlay sync deferred (prefer sidecar-export-over-file-sync, `cr-sqlite` only as fallback), gated on data-model stability.
- **Risks:** Conflict resolution (🔴). Deferred deliberately.

### Phase 8 — Moonshots *(Research)*

- Multi-recipe meal-time coordination (oven/stove contention solver); community recipe sharing (opt-in, ownership-preserving); voice cook mode.

### 13.1 Roadmap at a glance

| Phase | Name | Tier | Headline |
|------|------|------|----------|
| 0 | First Recipe | MVP-prep | Spine works end-to-end |
| 1 | Minimum Usable Kitchen | **MVP** | Import + search + pantry |
| 2 | Cook's Companion | Beta | Timeline + cook mode + scaling |
| 3 | Family Kitchen | **1.0 ✅** | Household + planning + scraping — data model declared stable → 1.0 |
| 4 | Web Interface | Post-1.0 | Browser UI |
| 5 | Native Apple | Post-1.0 | iOS/macOS/Watch |
| 6 | Smart Features | Research | OCR / AI |
| 7 | Sync | Research | Multi-device |
| 8 | Moonshots | Research | Coordination / community |

---

## Section 14: First 90 Days

A concrete plan to reach a usable MVP (Phases 0→1) for the solo developer.

### 14.1 De-risking spikes (do these first, in order)

1. **`cooklang-rs` spike (A3).** Parse 10 real recipes, round-trip, assess gaps. *Go/no-go on the parser.* (S)
2. **Paprika format spike (A7).** Crack a real `.paprikarecipes`, map fields. (M)
3. **schema.org extraction spike (A8).** Pull JSON-LD from 5 food blogs. (S)
4. **SQLite/FTS5 + reindex spike.** Prove derive-from-files + rebuild. (S)

### 14.2 The 10 MVP epics (with effort)

| # | Epic | Effort | Phase |
|---|------|--------|-------|
| E1 | Crate scaffold, CI, cargo-dist skeleton | M | 0 |
| E2 | Domain model + `.cook` parse/emit | L | 0 |
| E3 | SQLite schema + refinery + reindex | M | 0 |
| E4 | CLI shell (clap) + view/list/add/edit | M | 0–1 |
| E5 | FTS5 search + tags + filters | M | 1 |
| E6 | Paprika importer (+dry-run/review) | L | 1 |
| E7 | schema.org URL importer | M | 1 |
| E8 | Presence pantry + coverage % | M | 1 |
| E9 | Basic grocery list from recipe | S | 1 |
| E10 | Export (json/paprika) + docs (mdBook) | M | 1 |

*S≈1–2 wks, M≈2–4 wks, L≈4–8 wks, XL≈8+ wks at 10 hrs/week.*

### 14.3 Due-diligence backlog (parallel, low effort)

- Confirm Paprika ToS / personal-use stance for import.
- Confirm NYT/ATK scraping legality per their ToS (gates Phase 3).
- Source & license-check a starter ingredient/density/aisle dataset.
- Download & subset USDA FoodData Central; verify size for embedding.
- Stand up repo: README, CONTRIBUTING, LICENSE (MIT), issue templates.

### 14.4 Architecture Decision Records

Full ADR documents are in [`docs/adr/`](docs/adr/).

Ten ADRs covering the load-bearing technical decisions. Each: context → decision → alternatives rejected → consequences.

#### ADR-001 — Rust as the runtime/language 🟢 `[Validated]`

- **Context:** Need a portable, single-binary, fast, safe core that runs identically on Windows/macOS/Linux and can later bridge to native Apple and a web server.
- **Decision:** Build in **Rust (2021 edition)**.
- **Alternatives rejected:** *Go* — simpler but weaker domain modeling (enums/exhaustiveness) and no path to UniFFI/Swift; *TypeScript/Node* — runtime dependency, weaker for a CLI single-binary; *Swift* — Apple-centric, weak Windows/Linux CLI story; *Python* — packaging/perf pain for a distributable CLI.
- **Consequences:** Steeper dev velocity early; superb distribution (`cargo-dist`) and reuse across all front-ends. Matches portfolio (`toku`).

#### ADR-002 — Recipe storage: hybrid files + SQLite index 🟢 `[Validated]`

- **Context:** Must honor data ownership (#1) and Cooklang-native (#4) while supporting fast search and relational overlays (ratings, pantry, plans).
- **Decision:** **`.cook` plain-text files are the source of truth; SQLite is a derived, rebuildable index/overlay.** `fond reindex` reconstructs the DB from files.
- **Alternatives rejected:** *DB-only (everything in SQLite)* — violates ownership, no human-editable files, lock-in; *files-only (no DB)* — search/filter/overlays become slow and awkward at scale; *document DB (e.g., embedded Mongo-like)* — heavier, no FTS5, no precedent.
- **Consequences:** Slight write amplification (write file + index); enormous trust/ownership win; disaster recovery is trivial. Calibre/Obsidian precedent.

#### ADR-003 — Cooklang integration: use `cooklang-rs` 🟡 `[Validation Required]`

- **Context:** We must parse/emit `@ingredient{}`, `#cookware{}`, `~timer{}`, `---` metadata losslessly.
- **Decision:** Adopt the community **`cooklang-rs`** crate; gate on a Phase-0 spike against real recipes.
- **Alternatives rejected:** *Write our own parser* — multi-week 🔴 effort, reinvents a spec'd wheel, ongoing maintenance; *shell out to the reference parser* — adds a runtime dependency, breaks single-binary.
- **Consequences:** If the spike reveals gaps (e.g., missing emit/round-trip), we contribute upstream or write a thin emitter on top. Parser risk is isolated behind a `fond-domain` trait so a swap is contained.

#### ADR-004 — CLI structure & output 🟢 `[Validated]`

- **Context:** Primary interface for MVP/Beta; must be scriptable yet human-friendly.
- **Decision:** **`clap` v4 (derive)**, verb-noun commands, human tables by default (`comfy-table`), `--json` everywhere for scripting, `$EDITOR` integration for `edit`.
- **Alternatives rejected:** *Hand-rolled arg parsing* — error-prone, poor help; *`structopt`* — superseded by clap derive; *interactive-only TUI* — not scriptable, bad for power users.
- **Consequences:** Consistent UX, free help/completions; a stable `--json` contract that the web/native layers and tests can rely on.

#### ADR-005 — DB schema: family-shared with per-user scoping 🟡 `[Validation Required]`

- **Context:** Principle #3 demands multi-user from day one without over-engineering for a solo MVP.
- **Decision:** **One shared SQLite DB.** Shared entities (recipes, ingredients, plans) are global; subjective entities (notes, ratings, cook logs, dietary profiles) carry a `user_id`. MVP defaults to a single active user but the columns exist from v1.
- **Alternatives rejected:** *Per-user databases* — complicates sharing the recipe corpus, duplicates data; *no user concept until later* — guarantees a painful retrofit (violates #3); *full auth/RBAC* — overkill for a household.
- **Consequences:** Cheap now, no migration later. True multi-device identity reconciliation is deferred to the sync phase.

#### ADR-006 — Web scraping architecture 🔴 `[Validation Required]`

- **Context:** Import from websites, including authenticated subscriptions, without legal/ethical overreach.
- **Decision:** Isolated **`fond-scrape`** crate: schema.org/JSON-LD extraction first; site-specific fallback parsers; authenticated sessions using the **user's own credentials** stored in the OS keychain; strict no-redistribution. Per-site legality is `[Validation Required]`.
- **Alternatives rejected:** *Headless browser (Playwright/Chromium)* — heavy dependency, brittle, breaks single-binary; *central scraping service* — legal/redistribution nightmare, violates local-first; *no scraping at all* — abandons a key persona need.
- **Consequences:** Importers are best-effort and may break when sites change; breakage is sandboxed away from the core. If a site's ToS forbids automation, fond documents the gap rather than circumventing.

#### ADR-007 — Unit conversion engine 🔴 `[Validation Required]`

- **Context:** Volume↔weight conversion is ingredient-specific (density); vague units don't convert; baker's % exists.
- **Decision:** Typed **`Quantity { value, unit, ingredient_ref }`** with a units module that converts *within* a family freely and *across* volume↔weight **only** via a per-ingredient **density table** bundled in the reference dataset. Unknown density → conversion refused with a clear message. Vague units pass through untouched.
- **Alternatives rejected:** *Generic unit library (uom)* — handles physics units but not ingredient-specific culinary density or vague units; *assume a global density* — silently wrong; *force everything to grams* — loses the user's intent and vague units.
- **Consequences:** Correct-or-honest behavior; the engine is done early but the *density data* grows over phases (a 🔴 data, not code, problem).

#### ADR-008 — Cooking timeline engine 🔴 `[Validation Required]`

- **Context:** Principle #8 — realistic backward-scheduled timelines with active/passive time and dependencies.
- **Decision:** Model steps as a **directed acyclic graph** with `{duration, task_type, depends_on}`; compute a **backward schedule** from a target eat-time (latest-start per node via reverse topological pass). Durations come from `~timer{}` annotations plus heuristic parsing of step text; unparseable timing stays untimed (never fabricated).
- **Alternatives rejected:** *Flat sum of all durations* — ignores parallelism, wildly overestimates; *forward-only scheduling* — can't answer "when do I start?"; *ML timing prediction* — no data, overkill, Research-tier.
- **Consequences:** Single-recipe scheduling ships in Phase 2; multi-recipe resource contention (oven/stove) is a genuinely hard Phase 8 extension. Heuristic timing accuracy is `[Validation Required]`.

#### ADR-009 — Pantry & grocery model 🟡 `[Validation Required]`

- **Context:** Pantry tedium is the top product failure mode (§18); grocery lists must be pantry-aware and consolidated.
- **Decision:** **Presence-first, opt-in quantity.** A `PantryItem` records `present` (bool) by default; `quantity/unit/expiry/par_level` are optional. Coverage % works from presence alone. Consumption deduction requires **explicit user confirmation** after cooking — never silent.
- **Alternatives rejected:** *Mandatory quantity tracking* — guarantees abandonment (tedium); *fully automatic deduction* — drifts from reality, erodes trust; *no pantry* — abandons meal-prep/grocery value.
- **Consequences:** Low-friction adoption with a growth path to power-user precision. Grocery consolidation depends on the units engine (ADR-007) and ontology.

#### ADR-010 — Import architecture 🟡 `[Validation Required]`

- **Context:** Many sources (Paprika, schema.org, NYT, ATK, files), each messy, with a <10-minute promise and a no-data-loss rule.
- **Decision:** A trait-based **adapter pipeline** in `fond-import`: each source implements `Importer → Vec<RecipeDraft>`; drafts flow through a common normalize→to-Cooklang→validate→(write | review-queue) pipeline with a **dry-run** mode. Clean recipes write immediately; ambiguous ones queue for `fond review`.
- **Alternatives rejected:** *One bespoke import path per source* — duplicated normalization, inconsistent quality; *strict all-or-nothing import* — one bad recipe fails the batch; *lossy "good enough" import* — violates the no-data-loss principle.
- **Consequences:** New importers are cheap to add (one trait impl); quality is uniform; the <10-min promise holds because clean recipes never block on review.

#### 14.4.1 ADR summary

| ADR | Decision | Complexity |
|-----|----------|-----------|
| 001 | Rust runtime | 🟢 |
| 002 | Hybrid files + SQLite | 🟢 |
| 003 | `cooklang-rs` parser | 🟡 |
| 004 | clap v4 CLI + `--json` | 🟢 |
| 005 | Shared DB, per-user scoping | 🟡 |
| 006 | Isolated schema.org-first scraper | 🔴 |
| 007 | Per-ingredient density conversion | 🔴 |
| 008 | DAG backward-scheduling timeline | 🔴 |
| 009 | Presence-first opt-in pantry | 🟡 |
| 010 | Trait-based import pipeline + dry-run | 🟡 |

### 14.5 90-day definition of success

A real user imports their Paprika collection in <10 minutes, searches it instantly, checks pantry coverage, and generates a grocery list — entirely offline. That is the MVP bar.

---

## Section 15: Dependency Map

```text
cooklang-rs spike ─┬─► E2 domain/parse ─► E3 index ─► E4 CLI ─┬─► E5 search
                   │                                          ├─► E6 Paprika import
schema.org spike ──┼──────────────────────────────────────► E7 URL import
Paprika spike ─────┘                                          │
E5 search ─► filters ─► E8 pantry ─► E9 grocery               │
E2/E3 ─► (Phase 2) timeline engine ─► cook mode               │
E3 + user model ─► (Phase 3) profiles ─► dietary/meal-plan ───┘
data model STABLE (end Phase 3) ─► Phase 4 web ─► Phase 5 native
.cook files ─► Phase 7 file-sync ─►? cr-sqlite for overlays
```

**Critical path:** `cooklang-rs` viability → domain/parse → index → CLI → import. If the parser spike fails, the *entire* timeline shifts (write-our-own-parser is 🔴, multi-week). Hence it is spike #1.

---

## Section 16: Feasibility & Compromise Matrix

| Capability | Ideal | Realistic (solo) | Compromise shipped |
|------------|-------|------------------|--------------------|
| Cooklang round-trip | 100% lossless w/ annotations | High-fidelity, some prose kept as plain steps | Nothing dropped; annotate what's confident |
| Paprika import | Perfect field mapping | ~90% clean, rest in review queue | Dry-run + review flow |
| Web scraping | Every site | schema.org sites + a few authed parsers | Best-effort, isolated, documented gaps |
| Ingredient ontology | Complete normalized graph | Seed set, grows per phase | User-extendable dataset |
| Density conversion | Every ingredient | Common ingredients only | Refuse gracefully when unknown |
| Timeline | Multi-recipe coordination | Single-recipe backward schedule | Multi-recipe is Phase 8 |
| Scaling | Physically accurate (incl. time) | Linear + warnings | Never auto-scale time |
| Pantry | Full quantity tracking | Presence-first, opt-in quantity | Avoid tedium trap |
| Nutrition | Lab-accurate | USDA estimates | Informational + disclaimer |
| Sync | Seamless multi-device | File-sync first | CRDT only if needed |
| Native apps | Full parity | Read/cook first | Editing later |

The through-line: **ship the honest 80%, refuse to fake the other 20%, and grow the data over time.**

---

## Section 17: Naming *(DECIDED)*

The name is **fond** — *decided, not reopened*.

- **CLI binary / crate / repo:** `fond` · `fond` · `kafkade/fond`.
- **App name:** Fond.
- **Config dir:** `~/.config/fond/` (XDG) or `~/.fond/`; **recipe dir:** `~/fond/`.
- **Meaning:** the *fond* (browned flavor base in a pan) + *fondness* (family memories).
- **Taglines:** "The foundation of your kitchen" · "Build on your fond" · "Where cooking memories live."

No further naming exploration is needed; this section exists only to record the decision.

---

## Section 17A: Success Metrics

### 17A.1 Activation (the make-or-break)

- **Time-to-import:** P50 < 10 min for a 500-recipe Paprika collection. *(The single most important metric.)*
- **Import fidelity:** ≥90% of recipes write clean (no review needed).
- **First-search latency:** <100 ms on 5k recipes.

### 17A.2 Engagement

- **Weekly cooking sessions** (`fond cook` invocations) per active household.
- **Recipes cooked / month** (from CookLog) — proves it's a *cooking* tool, not a catalog.
- **Notes & ratings added** — the "cooking memories" signal (A20).

### 17A.3 Retention / ownership

- **Reindex success rate:** 100% (data-recovery guarantee must never fail).
- **Zero data-loss incidents** across imports/edits.
- **Export usage** treated as healthy (proves no lock-in), not churn.

### 17A.4 Community (principle #6)

- GitHub stars, external contributors, new importer PRs, Cooklang-ecosystem mentions.

### 17A.5 Anti-metrics (we deliberately do NOT optimize)

- Daily active *app-opens* (a kitchen tool shouldn't demand constant attention).
- Time-in-app (less time fighting the tool is better).
- Calorie/diet-tracking engagement (explicit non-goal).

---

## Section 18: Failure Modes & Mitigations

| # | Failure mode | Likelihood | Severity | Mitigation |
|---|--------------|-----------|----------|------------|
| F1 | **NYT/ATK block scraping** or ToS forbids it | High | Med | Isolate in `fond-scrape`; schema.org fallback; document the gap; never circumvent — see ADR-006 |
| F2 | **Paprika format undocumented / changes** | Med | High (MVP) | Spike on real export first; isolate adapter; dry-run + review; pin to observed format with tests |
| F3 | **Pantry too tedious → abandoned** | High | High | Presence-first, opt-in quantity, manual deduction (ADR-009); coverage % works with zero quantity data |
| F4 | **Feature creep** (solo dev overcommits) | High | High | 3–5 deliverables/phase; hard cut lines; vertical slices; defer all 🔴 data work |
| F5 | **Family-sharing complexity balloons** | Med | Med | Shared DB + `user_id` scoping only; no auth/RBAC; defer multi-device identity to sync phase |
| F6 | **Timeline can't parse prose timing** | Med | Med | `~timer{}` first; heuristics second; untimed steps stay untimed; manual timeline entry fallback (ADR-008) |
| F7 | **Solo-dev burnout / bus factor** | Med | High | MIT + clean architecture + docs to enable contributors; ship usable MVP fast for motivation; small phases |
| F8 | **`cooklang-rs` insufficient** | Med | High | Spike #1 (Phase 0); trait-isolated parser; contribute upstream or thin emitter — see ADR-003 |
| F9 | **Allergen data wrong → safety harm** | Low | Critical | Always disclaim "not medical advice"; surface allergens as info, never as a guarantee; user-verifiable source data |
| F10 | **Density/ontology data never complete** | High | Med | Refuse-gracefully design (ADR-007); ship seed + grow; user-extendable dataset |

---

## Section 19: Decision Log

A consolidated record of every load-bearing decision. One row, one decision, one reason.

| # | Decision | Recommendation | Rationale | Conf. |
|---|----------|----------------|-----------|-------|
| D1 | Language/runtime | Rust 2021 | Portable single-binary, safe, reusable core | `[Validated]` |
| D2 | Recipe storage | Hybrid: `.cook` files (truth) + SQLite index | Ownership + speed; Calibre model (ADR-002) | `[Validated]` |
| D3 | Database | SQLite + FTS5 via rusqlite | Embedded, zero-admin, fast search | `[Validated]` |
| D4 | Migrations | refinery | Versioned schema; matches toku | `[Validated]` |
| D5 | Cooklang parser | `cooklang-rs` (spike-gated) | Avoid reinventing the spec (ADR-003) | `[Validation Required]` |
| D6 | CLI framework | clap v4 derive + `--json` | Standard, scriptable (ADR-004) | `[Validated]` |
| D7 | Multi-user model | Shared DB + `user_id` scoping | Family from day one, no retrofit (ADR-005) | `[Validation Required]` |
| D8 | Scraping arch | schema.org-first + site parsers + user auth | Coverage w/o overreach (ADR-006) | `[Validation Required]` |
| D9 | Unit conversion | Per-ingredient density table; refuse-if-unknown | Culinary correctness (ADR-007) | `[Validation Required]` |
| D10 | Timeline | DAG backward-scheduling, active/passive | Realistic timing (ADR-008, princ. #8) | `[Validation Required]` |
| D11 | Pantry/grocery | Presence-first, opt-in qty, manual deduction | Beat the tedium failure (ADR-009) | `[Validation Required]` |
| D12 | Import arch | Trait adapters + normalize pipeline + dry-run | Quality + <10-min promise (ADR-010) | `[Validation Required]` |
| D13 | Import priority | Paprika + schema.org first; NYT/ATK in Beta | Serve the wedge persona first | `[Validated]` |
| D14 | Nutrition source | USDA FoodData Central (offline subset) | Public-domain, embeddable, informational | `[Validated]` |
| D15 | Photo storage | Content-addressed filesystem alongside files | Cooklang convention, ownership | `[Validated]` |
| D16 | License | MIT | Adoption + portfolio consistency (§11) | `[Validated]` |
| D17 | Web stack (Ph4) | Axum + HTMX over `fond-core` | Light, server-rendered, same core | `[Validated]` |
| D18 | Sync (Ph7) | File-sync first; cr-sqlite only if overlays need it ([ADR-012](docs/adr/012-sync-multi-device.md)) | Defer CRDT complexity; leverage owned files | File-sync `[Validated]`; overlay/CRDT `[Validation Required]` |
| D19 | Native bridge (Ph5) | UniFFI → SwiftUI | Reuse core on Apple platforms | `[Validated]` |
| D20 | Distribution/docs | cargo-dist + mdBook + GitHub Actions | Cross-platform binaries, free OSS CI | `[Validated]` |

---

*End of roadmap. This document decides a path, tags every assumption for validation, and front-loads the genuine domain risk (parser viability, density/timeline data) so a solo developer can ship a usable MVP within ~90 days and grow toward a family-shared, owned-forever cooking companion.*
