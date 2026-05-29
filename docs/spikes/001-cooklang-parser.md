# Spike 001: cooklang-rs Parser Evaluation

| Field          | Value                                |
| -------------- | ------------------------------------ |
| **Issue**      | [#1](https://github.com/kafkade/fond/issues/1) |
| **Status**     | ✅ Complete                          |
| **Verdict**    | **GO** — adopt cooklang-rs 0.18      |
| **Date**       | 2025-07-15                           |
| **Time-box**   | 1 day                                |

## Objective

Evaluate whether the [cooklang-rs](https://github.com/Zheoni/cooklang-rs)
crate (v0.18) is suitable as fond's recipe parser, per the go/no-go criteria
defined in [ADR-003](../adr/003-cooklang-integration.md).

## Go / No-Go Criteria (from ADR-003)

| Criterion                          | Target    | Result    | Status |
| ---------------------------------- | --------- | --------- | ------ |
| Parse fidelity across test corpus  | ≥ 95%     | **100%**  | ✅     |
| Metadata extensible (custom keys)  | Yes       | Yes       | ✅     |
| Ingredients / cookware / timers    | Preserved | Preserved | ✅     |
| Unicode support                    | Required  | Full      | ✅     |
| Sections / multi-step              | Required  | Works     | ✅     |
| Serializable model (serde)         | Required  | Yes       | ✅     |

## Test Corpus

11 fixture recipes covering diverse cuisines, edge cases, and Cooklang
features:

| Recipe                   | Ingredients | Cookware | Timers | Sections | Metadata |
| ------------------------ | ----------: | -------: | -----: | -------: | :------: |
| Chicken Adobo            |           7 |        2 |      3 |        1 | ✓        |
| Sourdough Bread          |           8 |        6 |      9 |        5 | ✓        |
| Mapo Tofu (麻婆豆腐)     |          15 |        2 |      6 |        1 | ✓        |
| Simple Eggs              |           4 |        1 |      1 |        1 | ✗        |
| Pasta alla Norma         |          11 |        4 |      4 |        1 | ✓        |
| Thai Green Curry         |          23 |        3 |      4 |        2 | ✓        |
| Crème Brûlée             |           5 |        9 |      5 |        1 | ✓        |
| Birria Tacos             |          21 |        7 |      5 |        3 | ✓        |
| Miso Ramen               |          19 |        2 |      5 |        4 | ✓        |
| Çilbir                   |          11 |        3 |      2 |        1 | ✓        |
| Chocolate Chip Cookies   |          12 |        8 |      7 |        3 | ✓        |
| **Totals**               |     **136** |   **47** | **51** |   **23** | **10/11** |

## Key Findings

### 1. Parser Quality — Excellent

- **100% parse success** across all 11 fixtures (0 parse errors)
- Only 2 warnings, both for `servings` key format (`"makes 2 loaves"` instead
  of a plain number). The parser still preserves the value; it just warns that
  it can't interpret it as a number for automatic scaling.
- All Cooklang spec features work: `@ingredient{}`, `#cookware{}`,
  `~timer{}`, YAML frontmatter, `= Section` headers, `-- comments`,
  `[- block comments -]`, `> notes`

### 2. Metadata — Fully Extensible

- YAML frontmatter is parsed into a `HashMap<String, serde_yaml::Value>`
- Custom keys (title, source, tags, prep time, cook time, difficulty, cuisine)
  are all preserved verbatim
- No schema restriction — fond can define its own metadata convention on top

### 3. Unicode — Full Support

- Chinese characters (麻婆豆腐), Turkish characters (Çilbir, Ç), Spanish
  accents — all parse correctly with no issues
- Ingredient names, metadata values, and step text all support Unicode

### 4. Model Serializability — serde Support

- The `Recipe` struct derives `Serialize`/`Deserialize`
- Successfully serialized to JSON and verified round-trip through
  `serde_json::to_string_pretty`
- This enables: JSON export, database storage, API responses, test snapshots

### 5. Extensions System

- `CooklangParser::new(Extensions::all(), Converter::default())` enables all
  extensions (notes, sections, block comments, etc.)
- Extensions can be selectively enabled/disabled — fond can use `all()` and
  document which extensions it relies on

### 6. Round-Trip Emitter — Gap Identified ⚠️

- The community emitter crate (`cooklang-to-cooklang` v0.15.0) depends on
  `cooklang = "0.15"` — **incompatible** with the latest parser (v0.18)
- This is a **version mismatch in the cooklang ecosystem**, not a design flaw
- **Mitigation**: fond will need a thin custom `.cook` emitter
  (`Recipe → String`). The model is well-structured, so this is estimated at
  1–2 days of work, not a multi-week effort
- This does NOT affect the go decision — the parser (read path) is the
  critical component; the emitter (write path) is a secondary concern

## Risks and Mitigations

| Risk | Severity | Mitigation |
| ---- | -------- | ---------- |
| `servings` format warnings | Low | Use numeric format or accept warning |
| Emitter incompatibility | Medium | Write thin custom emitter (1–2 days) |
| Upstream breaking changes | Low | Pin to `0.18`, test on update |
| Cooklang spec evolution | Low | Extensions system provides forward compatibility |

## Recommendations

1. **Adopt cooklang-rs 0.18** as fond's recipe parser — it meets all criteria
2. **Define a trait boundary** (`RecipeParser`) as specified in ADR-003,
   wrapping the cooklang crate to isolate fond-domain from parser internals
3. **Plan a custom emitter** in Phase 1 (MVP) — scope as a separate issue
4. **Use `Extensions::all()`** but document which extensions fond relies on
5. **Standardize metadata conventions** — define which YAML frontmatter keys
   fond expects (title, source, servings, tags, prep\_time, cook\_time, etc.)

## Test Evidence

All 20 spike tests pass:

```text
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Test file: `crates/fond-domain/tests/cooklang_spike.rs`
Fixtures: `crates/fond-domain/tests/fixtures/*.cook` (11 files)
