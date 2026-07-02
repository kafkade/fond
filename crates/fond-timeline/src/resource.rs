//! Kitchen resource model for multi-recipe meal coordination.
//!
//! When several dishes are cooked for one meal, they compete for finite
//! kitchen resources: the oven (one appliance, one temperature at a time),
//! a fixed number of stove burners, and the cook's own attention (only so
//! many hands-on tasks can run at once). This module models those resources
//! and the requirement each timeline step places on them.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A class of finite kitchen resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    /// The oven. Temperature-exclusive: it can only hold one temperature at
    /// a time, so two oven tasks may only overlap if they share a compatible
    /// temperature.
    Oven,
    /// A stovetop burner. A kitchen has a fixed number of these.
    Stove,
    /// The cook's hands-on attention. Only active (hands-on) tasks consume it.
    Cook,
}

impl ResourceKind {
    /// Short lowercase label for display (e.g. table cells).
    pub fn label(&self) -> &'static str {
        match self {
            ResourceKind::Oven => "oven",
            ResourceKind::Stove => "stove",
            ResourceKind::Cook => "cook",
        }
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceKind::Oven => write!(f, "Oven"),
            ResourceKind::Stove => write!(f, "Stove"),
            ResourceKind::Cook => write!(f, "Cook"),
        }
    }
}

/// An oven temperature, normalized to Fahrenheit for comparison.
///
/// Two oven tasks are considered compatible (able to share the oven) when
/// their temperatures are within [`OvenTemp::COMPAT_TOLERANCE_F`] of each other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OvenTemp {
    /// Temperature in degrees Fahrenheit.
    pub fahrenheit: i32,
    /// The original text the temperature was parsed from (e.g. "425°F", "180C").
    pub original: String,
}

impl OvenTemp {
    /// Two oven temperatures within this many °F are treated as compatible
    /// (dishes can share the oven).
    pub const COMPAT_TOLERANCE_F: i32 = 25;

    /// Build a temperature from a Fahrenheit value.
    pub fn from_fahrenheit(fahrenheit: i32, original: impl Into<String>) -> Self {
        Self {
            fahrenheit,
            original: original.into(),
        }
    }

    /// Whether two temperatures are close enough to share the oven.
    pub fn is_compatible_with(&self, other: &OvenTemp) -> bool {
        (self.fahrenheit - other.fahrenheit).abs() <= Self::COMPAT_TOLERANCE_F
    }
}

impl fmt::Display for OvenTemp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}°F", self.fahrenheit)
    }
}

/// The resource a single timeline step requires while it runs.
///
/// A step occupies exactly one primary appliance (oven or stove) and may also
/// demand the cook's attention when it is a hands-on (active) task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceRequirement {
    /// The primary appliance the step occupies, if any. `None` means the step
    /// needs no appliance (e.g. marinating in the fridge, resting on the
    /// counter), though it may still need the cook.
    pub appliance: Option<ResourceKind>,
    /// Oven temperature, when the appliance is the oven and a temperature was
    /// detected. `None` on an oven step means the temperature is unknown.
    pub oven_temp: Option<OvenTemp>,
    /// Whether the step needs the cook's hands-on attention while it runs.
    pub needs_cook: bool,
}

impl ResourceRequirement {
    /// A requirement that occupies no appliance and needs no attention.
    pub fn none() -> Self {
        Self {
            appliance: None,
            oven_temp: None,
            needs_cook: false,
        }
    }

    /// An oven requirement at an optional temperature.
    pub fn oven(temp: Option<OvenTemp>) -> Self {
        Self {
            appliance: Some(ResourceKind::Oven),
            oven_temp: temp,
            needs_cook: false,
        }
    }

    /// A stove requirement.
    pub fn stove(needs_cook: bool) -> Self {
        Self {
            appliance: Some(ResourceKind::Stove),
            oven_temp: None,
            needs_cook,
        }
    }

    /// Whether this requirement occupies the oven.
    pub fn is_oven(&self) -> bool {
        self.appliance == Some(ResourceKind::Oven)
    }

    /// Whether this requirement occupies a stove burner.
    pub fn is_stove(&self) -> bool {
        self.appliance == Some(ResourceKind::Stove)
    }

    /// A compact human-readable summary (e.g. "oven 425°F", "stove", "cook").
    pub fn summary(&self) -> String {
        match self.appliance {
            Some(ResourceKind::Oven) => match &self.oven_temp {
                Some(t) => format!("oven {t}"),
                None => "oven".to_string(),
            },
            Some(ResourceKind::Stove) => "stove".to_string(),
            Some(ResourceKind::Cook) => "cook".to_string(),
            None if self.needs_cook => "cook".to_string(),
            None => "—".to_string(),
        }
    }
}

/// The finite resources available in the kitchen.
///
/// Defaults model a common home kitchen: a single oven (temperature-exclusive),
/// four stove burners, and one cook. All are overridable from the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KitchenResources {
    /// Number of ovens available. Each oven holds one temperature at a time.
    pub ovens: u32,
    /// Number of stove burners available.
    pub burners: u32,
    /// Number of cooks (hands-on tasks that can run simultaneously).
    pub cooks: u32,
}

impl Default for KitchenResources {
    fn default() -> Self {
        Self {
            ovens: 1,
            burners: 4,
            cooks: 1,
        }
    }
}

impl KitchenResources {
    /// Capacity for a given resource kind.
    pub fn capacity(&self, kind: ResourceKind) -> u32 {
        match kind {
            ResourceKind::Oven => self.ovens,
            ResourceKind::Stove => self.burners,
            ResourceKind::Cook => self.cooks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_kitchen_matches_spec() {
        let k = KitchenResources::default();
        assert_eq!(k.ovens, 1);
        assert_eq!(k.burners, 4);
        assert_eq!(k.cooks, 1);
        assert_eq!(k.capacity(ResourceKind::Oven), 1);
        assert_eq!(k.capacity(ResourceKind::Stove), 4);
        assert_eq!(k.capacity(ResourceKind::Cook), 1);
    }

    #[test]
    fn oven_temp_compatibility() {
        let a = OvenTemp::from_fahrenheit(425, "425F");
        let b = OvenTemp::from_fahrenheit(450, "450F");
        let c = OvenTemp::from_fahrenheit(350, "350F");
        assert!(a.is_compatible_with(&b)); // within 25
        assert!(!a.is_compatible_with(&c)); // 75 apart
    }

    #[test]
    fn requirement_summaries() {
        assert_eq!(
            ResourceRequirement::oven(Some(OvenTemp::from_fahrenheit(400, "400F"))).summary(),
            "oven 400°F"
        );
        assert_eq!(ResourceRequirement::oven(None).summary(), "oven");
        assert_eq!(ResourceRequirement::stove(true).summary(), "stove");
        assert_eq!(ResourceRequirement::none().summary(), "—");
    }
}
