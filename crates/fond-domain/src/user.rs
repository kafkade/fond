use serde::{Deserialize, Serialize};
use std::fmt;

/// A household member profile.
///
/// Users are family-shared — the profile lives in the shared SQLite DB.
/// Subjective data (notes, ratings, cook logs, dietary prefs, allergens)
/// are scoped by `user_id`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub name: String,
    pub dietary_prefs: Vec<DietaryPref>,
    pub allergens: Vec<Allergen>,
    pub is_active: bool,
}

/// Recognized dietary preferences.
///
/// Stored as lowercase strings in the database for forward-compatibility.
/// Unknown values are preserved via `Other(String)`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum DietaryPref {
    Vegetarian,
    Vegan,
    Pescatarian,
    GlutenFree,
    DairyFree,
    NutFree,
    LowSodium,
    Kosher,
    Halal,
    /// Forward-compatible catch-all for user-defined preferences.
    #[serde(untagged)]
    Other(String),
}

impl DietaryPref {
    /// Parse from a user-supplied string (case-insensitive).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "vegetarian" => Self::Vegetarian,
            "vegan" => Self::Vegan,
            "pescatarian" => Self::Pescatarian,
            "gluten-free" | "glutenfree" => Self::GlutenFree,
            "dairy-free" | "dairyfree" => Self::DairyFree,
            "nut-free" | "nutfree" => Self::NutFree,
            "low-sodium" | "lowsodium" => Self::LowSodium,
            "kosher" => Self::Kosher,
            "halal" => Self::Halal,
            other => Self::Other(other.to_string()),
        }
    }

    /// Canonical string representation for DB storage.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Vegetarian => "vegetarian",
            Self::Vegan => "vegan",
            Self::Pescatarian => "pescatarian",
            Self::GlutenFree => "gluten-free",
            Self::DairyFree => "dairy-free",
            Self::NutFree => "nut-free",
            Self::LowSodium => "low-sodium",
            Self::Kosher => "kosher",
            Self::Halal => "halal",
            Self::Other(s) => s.as_str(),
        }
    }
}

impl fmt::Display for DietaryPref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Recognized food allergens (based on FDA top allergens + sesame).
///
/// Stored as lowercase strings in the database for forward-compatibility.
/// Unknown values are preserved via `Other(String)`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Allergen {
    Dairy,
    Egg,
    Fish,
    Gluten,
    Peanut,
    Sesame,
    Shellfish,
    Soy,
    TreeNut,
    /// Forward-compatible catch-all for user-defined allergens.
    #[serde(untagged)]
    Other(String),
}

impl Allergen {
    /// Parse from a user-supplied string (case-insensitive).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "dairy" | "milk" => Self::Dairy,
            "egg" | "eggs" => Self::Egg,
            "fish" => Self::Fish,
            "gluten" | "wheat" => Self::Gluten,
            "peanut" | "peanuts" => Self::Peanut,
            "sesame" => Self::Sesame,
            "shellfish" => Self::Shellfish,
            "soy" | "soya" => Self::Soy,
            "tree-nut" | "treenut" | "tree-nuts" | "treenuts" | "nuts" => Self::TreeNut,
            other => Self::Other(other.to_string()),
        }
    }

    /// Canonical string representation for DB storage.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Dairy => "dairy",
            Self::Egg => "egg",
            Self::Fish => "fish",
            Self::Gluten => "gluten",
            Self::Peanut => "peanut",
            Self::Sesame => "sesame",
            Self::Shellfish => "shellfish",
            Self::Soy => "soy",
            Self::TreeNut => "tree-nut",
            Self::Other(s) => s.as_str(),
        }
    }
}

impl fmt::Display for Allergen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Safety disclaimer for allergen information (F9).
pub const ALLERGEN_DISCLAIMER: &str = "⚠ Allergen information is for reference only — not medical advice. \
     Always verify ingredients if you have food allergies.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_allergens() {
        assert_eq!(Allergen::parse("dairy"), Allergen::Dairy);
        assert_eq!(Allergen::parse("MILK"), Allergen::Dairy);
        assert_eq!(Allergen::parse("Peanut"), Allergen::Peanut);
        assert_eq!(Allergen::parse("tree-nut"), Allergen::TreeNut);
        assert_eq!(Allergen::parse("nuts"), Allergen::TreeNut);
        assert_eq!(Allergen::parse("wheat"), Allergen::Gluten);
    }

    #[test]
    fn parse_unknown_allergen() {
        assert_eq!(
            Allergen::parse("mustard"),
            Allergen::Other("mustard".to_string())
        );
    }

    #[test]
    fn allergen_roundtrip() {
        let allergen = Allergen::Dairy;
        assert_eq!(Allergen::parse(allergen.as_str()), allergen);
    }

    #[test]
    fn parse_known_dietary_prefs() {
        assert_eq!(DietaryPref::parse("vegetarian"), DietaryPref::Vegetarian);
        assert_eq!(DietaryPref::parse("VEGAN"), DietaryPref::Vegan);
        assert_eq!(DietaryPref::parse("gluten-free"), DietaryPref::GlutenFree);
        assert_eq!(DietaryPref::parse("glutenfree"), DietaryPref::GlutenFree);
    }

    #[test]
    fn parse_unknown_dietary_pref() {
        assert_eq!(
            DietaryPref::parse("paleo"),
            DietaryPref::Other("paleo".to_string())
        );
    }

    #[test]
    fn dietary_pref_roundtrip() {
        let pref = DietaryPref::Vegetarian;
        assert_eq!(DietaryPref::parse(pref.as_str()), pref);
    }

    #[test]
    fn display_formats() {
        assert_eq!(Allergen::Dairy.to_string(), "dairy");
        assert_eq!(Allergen::TreeNut.to_string(), "tree-nut");
        assert_eq!(DietaryPref::GlutenFree.to_string(), "gluten-free");
        assert_eq!(DietaryPref::Other("paleo".into()).to_string(), "paleo");
    }
}
