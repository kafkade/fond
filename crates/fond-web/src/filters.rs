//! Custom Askama filters for fond templates.

/// Pluralise a word based on count.
#[allow(dead_code)]
pub fn pluralize(count: &usize, singular: &str, plural: &str) -> askama::Result<String> {
    Ok(if *count == 1 {
        singular.to_string()
    } else {
        plural.to_string()
    })
}
