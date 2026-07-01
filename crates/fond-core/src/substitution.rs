//! Curated, advisory ingredient substitution engine.
//!
//! Looks up context-aware substitutions ("out of buttermilk? use milk +
//! lemon juice") from a bundled, curated, sourced reference dataset — this
//! is **not** a generative model. Results are ranked and advisory only:
//! nothing here mutates a recipe or a `.cook` file (ROADMAP §6.2, §3A.1).
//!
//! The dataset is embedded at compile time and parsed once on first use.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Cooking context a substitution is appropriate for.
///
/// A wrong swap in baking can ruin a dish (leavening, structure), so
/// substitutions are tagged and ranked by context rather than applied blindly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CookingContext {
    /// Applicable broadly, no context-specific concern.
    General,
    /// Baking — where acidity, leavening, and structure matter most.
    Baking,
    /// Sauteing / stovetop cooking.
    Sauteing,
}

impl CookingContext {
    /// Lower-case label for display and matching.
    pub fn label(&self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Baking => "baking",
            Self::Sauteing => "sauteing",
        }
    }
}

impl std::fmt::Display for CookingContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A single ranked, sourced substitution option for an ingredient.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Substitution {
    /// The replacement (e.g., "milk + lemon juice").
    pub substitute: String,
    /// Conversion ratio / preparation (e.g., "1 cup buttermilk = 1 cup milk + 1 tbsp lemon juice").
    pub ratio: String,
    /// Cooking contexts this substitution suits.
    pub contexts: Vec<CookingContext>,
    /// Advisory caveat (e.g., a baking-structure warning), if any.
    #[serde(default)]
    pub caveat: Option<String>,
    /// Rank within the ingredient's options (1 = best general recommendation).
    pub rank: u8,
    /// Source citation for the ratio/advice.
    pub source: String,
}

impl Substitution {
    /// Whether this substitution is tagged for the given context.
    pub fn applies_to(&self, context: CookingContext) -> bool {
        self.contexts.contains(&context)
    }
}

/// A canonical ingredient and its ranked substitution options.
#[derive(Debug, Clone, Deserialize)]
struct SubstitutionSet {
    canonical: String,
    #[serde(default)]
    aliases: Vec<String>,
    substitutions: Vec<Substitution>,
}

/// The bundled dataset, as deserialized from JSON.
#[derive(Debug, Deserialize)]
struct SubstitutionDataset {
    #[allow(dead_code)]
    schema_version: u32,
    #[allow(dead_code)]
    #[serde(default)]
    description: String,
    entries: Vec<SubstitutionSet>,
}

/// The result of a substitution lookup — advisory, ranked, and sourced.
#[derive(Debug, Clone, Serialize)]
pub struct SubstitutionResult {
    /// The ingredient the user asked about (as typed, normalized for display).
    pub ingredient: String,
    /// The canonical ingredient name that matched.
    pub canonical: String,
    /// The context that was requested/inferred, if any.
    pub context: Option<CookingContext>,
    /// Ranked substitution options (context-relevant first when a context is set).
    pub substitutions: Vec<Substitution>,
}

/// Advisory disclaimer shown alongside substitution results.
pub const SUBSTITUTION_DISCLAIMER: &str = "Advisory only — substitutions are suggestions, never auto-applied. \
     Baking swaps can change structure; verify before you commit.";

const SUBSTITUTIONS_JSON: &str = include_str!("../../../data/substitutions/substitutions.json");

fn dataset() -> &'static [SubstitutionSet] {
    static DATA: OnceLock<Vec<SubstitutionSet>> = OnceLock::new();
    DATA.get_or_init(|| {
        let parsed: SubstitutionDataset = serde_json::from_str(SUBSTITUTIONS_JSON)
            .expect("bundled substitutions.json must be valid");
        parsed.entries
    })
}

/// Normalize an ingredient name for matching: lowercase, trimmed, and with
/// runs of whitespace collapsed to single spaces.
fn normalize(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Find substitutions for an ingredient, optionally prioritized by context.
///
/// Matching is case-insensitive and resolves aliases to a canonical name.
/// When a `context` is supplied, substitutions tagged for that context are
/// returned first (each group kept in rank order); otherwise all options are
/// returned in rank order. Returns `None` if the ingredient isn't in the
/// curated dataset.
pub fn find_substitutions(
    ingredient: &str,
    context: Option<CookingContext>,
) -> Option<SubstitutionResult> {
    let query = normalize(ingredient);
    if query.is_empty() {
        return None;
    }

    let set = match_set(&query)?;

    let mut subs = set.substitutions.clone();
    // Stable sort by rank first so ties keep dataset order.
    subs.sort_by_key(|s| s.rank);

    if let Some(ctx) = context {
        // Context-relevant options first, each group still rank-ordered.
        subs.sort_by_key(|s| if s.applies_to(ctx) { 0 } else { 1 });
    }

    Some(SubstitutionResult {
        ingredient: query,
        canonical: set.canonical.clone(),
        context,
        substitutions: subs,
    })
}

/// Resolve a normalized query to a dataset entry via canonical name or alias.
fn match_set(query: &str) -> Option<&'static SubstitutionSet> {
    let data = dataset();

    // Exact canonical or alias match.
    for set in data {
        if normalize(&set.canonical) == query {
            return Some(set);
        }
        if set.aliases.iter().any(|a| normalize(a) == query) {
            return Some(set);
        }
    }

    // Fall back to a simple singular/plural tolerant match on the canonical
    // name (e.g., "eggs" ↔ "egg") without pulling in false positives.
    for set in data {
        let canon = normalize(&set.canonical);
        if singular_plural_match(query, &canon) {
            return Some(set);
        }
        if set
            .aliases
            .iter()
            .any(|a| singular_plural_match(query, &normalize(a)))
        {
            return Some(set);
        }
    }

    None
}

/// True if two names match once a trailing "s" is disregarded on either side.
fn singular_plural_match(a: &str, b: &str) -> bool {
    let strip = |s: &str| s.strip_suffix('s').unwrap_or(s).to_string();
    !a.is_empty() && !b.is_empty() && strip(a) == strip(b)
}

/// List the canonical ingredient names available in the dataset (sorted).
pub fn available_ingredients() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = dataset().iter().map(|s| s.canonical.as_str()).collect();
    names.sort_unstable();
    names
}

/// Baking-related signal words found in tags or a recipe title.
const BAKING_TAG_SIGNALS: &[&str] = &[
    "bake", "baking", "baked", "dessert", "cake", "cookie", "bread", "pastry", "muffin", "pie",
    "brownie", "biscuit", "scone", "tart", "dough", "loaf", "cupcake", "frosting",
];

/// Ingredients that, together with flour, strongly imply baking.
const BAKING_INGREDIENT_SIGNALS: &[&str] = &[
    "baking powder",
    "baking soda",
    "yeast",
    "sugar",
    "brown sugar",
    "powdered sugar",
    "cocoa powder",
];

/// Infer a cooking context from recipe signals (tags, title, ingredients).
///
/// Deterministic and conservative: returns [`CookingContext::Baking`] only
/// when there is a clear baking signal, otherwise `None` (treat as general).
/// This never fabricates a sauteing context — the point is to surface baking
/// caveats, which is where a wrong substitution is most costly.
pub fn infer_context(
    tags: &[String],
    title: &str,
    ingredient_names: &[String],
) -> Option<CookingContext> {
    let contains_signal = |haystack: &str| {
        let h = haystack.to_lowercase();
        BAKING_TAG_SIGNALS.iter().any(|sig| word_present(&h, sig))
    };

    if tags.iter().any(|t| contains_signal(t)) || contains_signal(title) {
        return Some(CookingContext::Baking);
    }

    // Flour + (leavening or sugar) is a reliable baking fingerprint.
    let names: Vec<String> = ingredient_names.iter().map(|n| n.to_lowercase()).collect();
    let has_flour = names.iter().any(|n| word_present(n, "flour"));
    let has_baking_partner = names.iter().any(|n| {
        BAKING_INGREDIENT_SIGNALS
            .iter()
            .any(|sig| word_present(n, sig))
    });
    if has_flour && has_baking_partner {
        return Some(CookingContext::Baking);
    }

    None
}

/// Whether `needle` appears in `haystack` on word boundaries (both lowercase).
fn word_present(haystack: &str, needle: &str) -> bool {
    if let Some(pos) = haystack.find(needle) {
        let before_ok = pos == 0 || !haystack.as_bytes()[pos - 1].is_ascii_alphanumeric();
        let end = pos + needle.len();
        let after_ok = end >= haystack.len() || !haystack.as_bytes()[end].is_ascii_alphanumeric();
        before_ok && after_ok
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataset_loads() {
        assert!(!dataset().is_empty());
    }

    #[test]
    fn buttermilk_has_ranked_sourced_options() {
        let result = find_substitutions("buttermilk", None).expect("buttermilk present");
        assert_eq!(result.canonical, "buttermilk");
        assert!(!result.substitutions.is_empty());

        // Top option is the classic milk + acid swap.
        let top = &result.substitutions[0];
        assert!(top.substitute.to_lowercase().contains("lemon"));
        assert!(top.ratio.to_lowercase().contains("buttermilk"));
        assert!(!top.source.is_empty());
        // Baking caveat is surfaced.
        assert!(
            result
                .substitutions
                .iter()
                .any(|s| s.caveat.is_some() && s.applies_to(CookingContext::Baking))
        );
    }

    #[test]
    fn rank_order_preserved_without_context() {
        let result = find_substitutions("honey", None).unwrap();
        let ranks: Vec<u8> = result.substitutions.iter().map(|s| s.rank).collect();
        let mut sorted = ranks.clone();
        sorted.sort_unstable();
        assert_eq!(ranks, sorted);
    }

    #[test]
    fn context_prioritizes_matching_substitutions() {
        // Butter's sauteing option (oil) should lead when sauteing.
        let result = find_substitutions("butter", Some(CookingContext::Sauteing)).unwrap();
        assert!(result.substitutions[0].applies_to(CookingContext::Sauteing));

        // Every sauteing-tagged option precedes every non-sauteing one.
        let first_non_match = result
            .substitutions
            .iter()
            .position(|s| !s.applies_to(CookingContext::Sauteing));
        if let Some(idx) = first_non_match {
            assert!(
                result.substitutions[idx..]
                    .iter()
                    .all(|s| !s.applies_to(CookingContext::Sauteing))
            );
        }
    }

    #[test]
    fn case_insensitive_lookup() {
        assert!(find_substitutions("BUTTERMILK", None).is_some());
        assert!(find_substitutions("  Butter  ", None).is_some());
    }

    #[test]
    fn alias_resolution() {
        let by_alias = find_substitutions("bicarbonate of soda", None).unwrap();
        assert_eq!(by_alias.canonical, "baking soda");
    }

    #[test]
    fn singular_plural_tolerance() {
        let plural = find_substitutions("eggs", None).unwrap();
        assert_eq!(plural.canonical, "egg");
    }

    #[test]
    fn unknown_ingredient_returns_none() {
        assert!(find_substitutions("unobtanium", None).is_none());
        assert!(find_substitutions("", None).is_none());
    }

    #[test]
    fn all_entries_wellformed() {
        for set in dataset() {
            assert!(!set.canonical.is_empty());
            assert!(
                !set.substitutions.is_empty(),
                "{} has no subs",
                set.canonical
            );
            for sub in &set.substitutions {
                assert!(!sub.substitute.is_empty());
                assert!(!sub.ratio.is_empty());
                assert!(!sub.source.is_empty());
                assert!(
                    !sub.contexts.is_empty(),
                    "{} sub missing context",
                    set.canonical
                );
            }
        }
    }

    #[test]
    fn infer_context_from_tags() {
        let ctx = infer_context(
            &["dessert".to_string(), "quick".to_string()],
            "Chocolate Chip Cookies",
            &[],
        );
        assert_eq!(ctx, Some(CookingContext::Baking));
    }

    #[test]
    fn infer_context_from_title() {
        let ctx = infer_context(&[], "Banana Bread", &[]);
        assert_eq!(ctx, Some(CookingContext::Baking));
    }

    #[test]
    fn infer_context_from_flour_plus_leavening() {
        let ctx = infer_context(
            &[],
            "Weeknight Something",
            &[
                "all-purpose flour".to_string(),
                "baking powder".to_string(),
                "milk".to_string(),
            ],
        );
        assert_eq!(ctx, Some(CookingContext::Baking));
    }

    #[test]
    fn infer_context_none_for_savory() {
        let ctx = infer_context(
            &["dinner".to_string()],
            "Chicken Stir-Fry",
            &[
                "chicken".to_string(),
                "soy sauce".to_string(),
                "garlic".to_string(),
            ],
        );
        assert_eq!(ctx, None);
    }

    #[test]
    fn infer_context_flour_alone_is_not_baking() {
        // Flour as a thickener (roux) without sugar/leavening isn't "baking".
        let ctx = infer_context(
            &[],
            "Gravy",
            &[
                "flour".to_string(),
                "butter".to_string(),
                "broth".to_string(),
            ],
        );
        assert_eq!(ctx, None);
    }
}
