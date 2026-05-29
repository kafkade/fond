/// Generate a URL-friendly slug from a recipe title.
///
/// Lowercases, replaces non-alphanumeric characters with hyphens,
/// collapses consecutive hyphens, and trims leading/trailing hyphens.
///
/// # Examples
///
/// ```
/// # use fond_domain::slugify;
/// assert_eq!(slugify("Classic Chicken Adobo"), "classic-chicken-adobo");
/// assert_eq!(slugify("Crème Brûlée"), "crme-brle");
/// assert_eq!(slugify("Mapo Tofu (四川麻婆豆腐)"), "mapo-tofu");
/// ```
pub fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());

    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' || ch == ' ' {
            slug.push('-');
        }
        // Non-ASCII and other punctuation are dropped
    }

    // Collapse consecutive hyphens
    let mut result = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for ch in slug.chars() {
        if ch == '-' {
            if !prev_hyphen {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(ch);
            prev_hyphen = false;
        }
    }

    // Trim leading/trailing hyphens
    result.trim_matches('-').to_string()
}

/// Derive a title from a filename stem (e.g., "chicken-adobo" → "Chicken Adobo").
pub fn title_from_stem(stem: &str) -> String {
    stem.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let mut s = first.to_uppercase().to_string();
                    s.extend(chars);
                    s
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_slugify() {
        assert_eq!(slugify("Classic Chicken Adobo"), "classic-chicken-adobo");
    }

    #[test]
    fn slugify_with_special_chars() {
        assert_eq!(slugify("Pasta alla Norma!"), "pasta-alla-norma");
    }

    #[test]
    fn slugify_collapses_hyphens() {
        assert_eq!(slugify("one---two"), "one-two");
    }

    #[test]
    fn slugify_trims_edges() {
        assert_eq!(slugify("  hello  "), "hello");
    }

    #[test]
    fn slugify_drops_non_ascii() {
        assert_eq!(slugify("Crème Brûlée"), "crme-brle");
    }

    #[test]
    fn slugify_mixed_unicode() {
        assert_eq!(slugify("Mapo Tofu (四川麻婆豆腐)"), "mapo-tofu");
    }

    #[test]
    fn title_from_stem_basic() {
        assert_eq!(title_from_stem("chicken-adobo"), "Chicken Adobo");
    }

    #[test]
    fn title_from_stem_single_word() {
        assert_eq!(title_from_stem("sourdough"), "Sourdough");
    }
}
