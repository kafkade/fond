use serde::{Deserialize, Serialize};

/// Filters for searching and listing recipes.
///
/// All fields are optional — only set fields constrain the results.
/// Multiple tags use AND semantics (recipe must have all specified tags).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecipeFilter {
    /// Only include recipes with ALL of these tags.
    pub tags: Vec<String>,
    /// Maximum total time in minutes.
    pub max_time_minutes: Option<u32>,
    /// Filter by source (case-insensitive substring match).
    pub source: Option<String>,
}

impl RecipeFilter {
    /// Returns true if no filters are set.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.max_time_minutes.is_none() && self.source.is_none()
    }
}

/// Parse a human-readable time string into minutes.
///
/// Supports common patterns from Cooklang metadata:
/// - "45 min", "45 minutes", "45min"
/// - "1 hour", "2 hours", "1.5 hours"
/// - "1 hour 30 minutes", "1h 30m", "1h30m"
/// - Bare numbers assumed to be minutes: "30"
///
/// Returns `None` if the string cannot be parsed.
pub fn parse_time_minutes(s: &str) -> Option<u32> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }

    let mut total_minutes: f64 = 0.0;
    let mut found_any = false;

    // Try compound patterns: "1 hour 30 minutes", "1h30m"
    let mut remaining = s.as_str();

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        // Extract leading number
        let (num_str, rest) = split_number(remaining);
        if num_str.is_empty() {
            // Skip non-numeric prefix (e.g. "and", ",")
            let skip = remaining
                .find(|c: char| c.is_ascii_digit())
                .unwrap_or(remaining.len());
            remaining = &remaining[skip..];
            continue;
        }

        let num: f64 = num_str.parse().ok()?;
        let rest = rest.trim_start();

        // Extract unit
        let (unit, after_unit) = split_unit(rest);

        match classify_unit(&unit) {
            TimeUnit::Hours => {
                total_minutes += num * 60.0;
                found_any = true;
            }
            TimeUnit::Minutes => {
                total_minutes += num;
                found_any = true;
            }
            TimeUnit::Unknown => {
                if !found_any {
                    // Bare number — assume minutes
                    total_minutes += num;
                    found_any = true;
                }
                // If we already found a unit, skip unknown suffixes
            }
        }

        remaining = after_unit;
    }

    if found_any && total_minutes >= 0.0 {
        Some(total_minutes.round() as u32)
    } else {
        None
    }
}

/// Escape a user-provided search string for safe use in FTS5 MATCH.
///
/// Wraps each whitespace-delimited token in double quotes to prevent
/// FTS5 syntax operators (AND, OR, NOT, NEAR, column:, *, ^) from
/// being interpreted as commands.
pub fn escape_fts5_query(input: &str) -> String {
    let tokens: Vec<String> = input
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|token| {
            // Strip internal double-quotes to prevent injection
            let clean: String = token.chars().filter(|&c| c != '"').collect();
            if clean.is_empty() {
                return String::new();
            }
            format!("\"{clean}\"")
        })
        .filter(|t| !t.is_empty())
        .collect();

    tokens.join(" ")
}

// ─────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────

#[derive(Debug)]
enum TimeUnit {
    Hours,
    Minutes,
    Unknown,
}

fn classify_unit(s: &str) -> TimeUnit {
    match s {
        "h" | "hr" | "hrs" | "hour" | "hours" => TimeUnit::Hours,
        "m" | "min" | "mins" | "minute" | "minutes" => TimeUnit::Minutes,
        _ => TimeUnit::Unknown,
    }
}

/// Split a leading numeric value (integer or decimal) from the rest.
fn split_number(s: &str) -> (&str, &str) {
    let end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}

/// Split a leading alphabetical unit from the rest.
fn split_unit(s: &str) -> (String, &str) {
    let end = s
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(s.len());
    (s[..end].to_string(), &s[end..])
}

/// Update the `tags:` line in raw `.cook` file content.
///
/// Performs a targeted edit of the metadata block, preserving all
/// other content byte-for-byte. Returns the modified content, or
/// the original if the metadata block cannot be located.
pub fn update_tags_in_cook_source(content: &str, new_tags: &[String]) -> String {
    // Cooklang metadata is between `---` fences at the top of the file
    let lines: Vec<&str> = content.lines().collect();

    // Find metadata boundaries
    let mut fence_start = None;
    let mut fence_end = None;

    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "---" {
            if fence_start.is_none() {
                fence_start = Some(i);
            } else {
                fence_end = Some(i);
                break;
            }
        }
    }

    let (Some(start), Some(end)) = (fence_start, fence_end) else {
        // No metadata block — prepend one with tags
        if new_tags.is_empty() {
            return content.to_string();
        }
        let tags_line = format!("tags: {}", new_tags.join(", "));
        return format!("---\n{tags_line}\n---\n{content}");
    };

    // Find existing tags line in metadata
    let mut tag_line_idx = None;
    let mut tag_end_idx = None; // for multi-line YAML list tags

    for i in (start + 1)..end {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("tags:") {
            tag_line_idx = Some(i);

            // Check if it's a YAML list (next lines start with "  -")
            let mut j = i + 1;
            while j < end && lines[j].trim().starts_with("- ") {
                j += 1;
            }
            tag_end_idx = Some(j);
            break;
        }
    }

    let tags_value = if new_tags.is_empty() {
        String::new()
    } else {
        new_tags.join(", ")
    };

    let mut result_lines: Vec<String> = Vec::new();

    if let Some(tag_start) = tag_line_idx {
        let tag_end = tag_end_idx.unwrap_or(tag_start + 1);

        for (i, line) in lines.iter().enumerate() {
            if i == tag_start {
                if new_tags.is_empty() {
                    // Remove the tags line entirely
                    continue;
                }
                result_lines.push(format!("tags: {tags_value}"));
            } else if i > tag_start && i < tag_end {
                // Skip YAML list continuation lines
                continue;
            } else {
                result_lines.push(line.to_string());
            }
        }
    } else if !new_tags.is_empty() {
        // No existing tags line — insert before closing fence
        for (i, line) in lines.iter().enumerate() {
            if i == end {
                result_lines.push(format!("tags: {tags_value}"));
            }
            result_lines.push(line.to_string());
        }
    } else {
        return content.to_string();
    }

    // Preserve trailing newline
    let mut result = result_lines.join("\n");
    if content.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── parse_time_minutes ──────────────────────────────

    #[test]
    fn parse_minutes_only() {
        assert_eq!(parse_time_minutes("45 min"), Some(45));
        assert_eq!(parse_time_minutes("45 minutes"), Some(45));
        assert_eq!(parse_time_minutes("45min"), Some(45));
        assert_eq!(parse_time_minutes("30"), Some(30));
    }

    #[test]
    fn parse_hours_only() {
        assert_eq!(parse_time_minutes("1 hour"), Some(60));
        assert_eq!(parse_time_minutes("2 hours"), Some(120));
        assert_eq!(parse_time_minutes("1.5 hours"), Some(90));
    }

    #[test]
    fn parse_compound() {
        assert_eq!(parse_time_minutes("1 hour 30 minutes"), Some(90));
        assert_eq!(parse_time_minutes("1h 30m"), Some(90));
        assert_eq!(parse_time_minutes("1h30m"), Some(90));
        assert_eq!(parse_time_minutes("2 hours 15 min"), Some(135));
    }

    #[test]
    fn parse_empty_and_invalid() {
        assert_eq!(parse_time_minutes(""), None);
        assert_eq!(parse_time_minutes("   "), None);
    }

    // ─── escape_fts5_query ───────────────────────────────

    #[test]
    fn escape_simple_terms() {
        assert_eq!(escape_fts5_query("chicken adobo"), "\"chicken\" \"adobo\"");
    }

    #[test]
    fn escape_fts5_operators() {
        assert_eq!(
            escape_fts5_query("chicken AND garlic"),
            "\"chicken\" \"AND\" \"garlic\""
        );
    }

    #[test]
    fn escape_empty() {
        assert_eq!(escape_fts5_query(""), "");
        assert_eq!(escape_fts5_query("   "), "");
    }

    #[test]
    fn escape_quotes() {
        assert_eq!(escape_fts5_query("\"test\""), "\"test\"");
    }

    // ─── update_tags_in_cook_source ──────────────────────

    #[test]
    fn update_tags_inline() {
        let content = "---\ntitle: Test\ntags: old, tags\n---\n\nStep 1.\n";
        let result = update_tags_in_cook_source(content, &["new".into(), "tags".into()]);
        assert!(result.contains("tags: new, tags"));
        assert!(!result.contains("old"));
        assert!(result.contains("Step 1."));
    }

    #[test]
    fn update_tags_yaml_list() {
        let content = "---\ntitle: Test\ntags:\n  - old\n  - tags\n---\n\nStep 1.\n";
        let result = update_tags_in_cook_source(content, &["a".into(), "b".into()]);
        assert!(result.contains("tags: a, b"));
        assert!(!result.contains("  - old"));
    }

    #[test]
    fn add_tags_to_metadata_without_tags() {
        let content = "---\ntitle: Test\n---\n\nStep 1.\n";
        let result = update_tags_in_cook_source(content, &["new".into()]);
        assert!(result.contains("tags: new"));
    }

    #[test]
    fn remove_all_tags() {
        let content = "---\ntitle: Test\ntags: old\n---\n\nStep 1.\n";
        let result = update_tags_in_cook_source(content, &[]);
        assert!(!result.contains("tags:"));
    }

    // ─── RecipeFilter ────────────────────────────────────

    #[test]
    fn filter_default_is_empty() {
        let f = RecipeFilter::default();
        assert!(f.is_empty());
    }

    #[test]
    fn filter_with_tag_is_not_empty() {
        let f = RecipeFilter {
            tags: vec!["chicken".into()],
            ..Default::default()
        };
        assert!(!f.is_empty());
    }
}
