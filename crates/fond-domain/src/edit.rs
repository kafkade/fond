//! Lossless in-place editing of `.cook` documents.
//!
//! [`CookDocument`] models a `.cook` file as an editable *frontmatter* (an
//! ordered list of YAML-ish `key: value` lines) plus a *body* of ordered
//! [`Block`]s (steps, section headers, quotes, comments). It is the write-side
//! counterpart to [`crate::parser::parse_cook`].
//!
//! ## Round-trip guarantee
//!
//! Editing is *surgical* and *dirty-tracked*:
//!
//! - A document that is parsed and re-emitted without edits reproduces its
//!   input **byte-for-byte** (`emit() == original`).
//! - Metadata-only edits rewrite just the affected frontmatter line(s) and
//!   preserve the body **byte-for-byte** (unknown/extra keys, blank lines,
//!   quotes and comments included).
//! - Step edits re-serialise the body from its blocks; non-step blocks
//!   (section headers, `>` quotes, `--`/`[- -]` comments) are preserved
//!   verbatim and only the edited steps change.
//!
//! Ingredients, cookware and timers live *inline* in step text as Cooklang
//! markup (`@name{qty%unit}`, `#cookware{}`, `~timer{}`); there is no separate
//! ingredient list in a `.cook` file, so ingredient edits are made by editing
//! the step text and re-parsed for display via [`crate::parser::parse_cook`].

use crate::slug::slugify;

/// The kind of a body [`Block`], inferred from its leading markup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// A cooking step / instruction paragraph (may contain inline Cooklang).
    Step,
    /// A section header (`= Name` or `== Name ==`).
    Section,
    /// A block quote / tip (`> ...`).
    Note,
    /// A line comment paragraph (`-- ...`).
    Comment,
    /// A block comment (`[- ... -]`).
    BlockComment,
}

/// A single paragraph-level block of a recipe body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// The raw text of the block, verbatim (inline Cooklang markup preserved).
    pub text: String,
}

impl Block {
    /// Build a block, classifying its kind from the leading markup.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let kind = classify(&text);
        Self { kind, text }
    }

    /// The section name, if this block is a section header.
    pub fn section_name(&self) -> Option<String> {
        if self.kind != BlockKind::Section {
            return None;
        }
        let name = self.text.trim().trim_matches('=').trim();
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    }
}

fn classify(text: &str) -> BlockKind {
    let t = text.trim_start();
    if t.starts_with('=') {
        BlockKind::Section
    } else if t.starts_with('>') {
        BlockKind::Note
    } else if t.starts_with("[-") {
        BlockKind::BlockComment
    } else if t.starts_with("--") {
        BlockKind::Comment
    } else {
        BlockKind::Step
    }
}

/// A body block paired with the section it belongs to (resolved by scanning
/// section headers in order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionedBlock {
    pub kind: BlockKind,
    pub text: String,
    /// The section in effect at this block, if any.
    pub section: Option<String>,
}

/// An editable `.cook` document: frontmatter + ordered body blocks.
#[derive(Debug, Clone)]
pub struct CookDocument {
    /// Whether the source had a `--- ... ---` frontmatter fence.
    fm_present: bool,
    /// The inner frontmatter lines (without the fences), verbatim.
    fm_lines: Vec<String>,
    /// The original frontmatter incl. fences and trailing newline (or empty).
    prefix_original: String,
    /// Everything after the frontmatter prefix, verbatim.
    body_original: String,
    /// Whitespace prefix of `body_original` (blank line after the fence).
    body_leading: String,
    /// Parsed body blocks.
    blocks: Vec<Block>,
    /// The document's dominant line ending (`"\n"` or `"\r\n"`), preserved so
    /// edits don't silently rewrite a CRLF file with LF fences (and vice
    /// versa) — Windows checkouts and hand-authored files may use CRLF.
    newline: String,
    fm_dirty: bool,
    body_dirty: bool,
}

impl CookDocument {
    /// Parse raw `.cook` text into an editable document.
    pub fn parse(raw: &str) -> Self {
        let newline = detect_newline(raw);
        if let Some((inner, prefix_len)) = parse_frontmatter(raw) {
            let prefix_original = raw[..prefix_len].to_string();
            let body_original = raw[prefix_len..].to_string();
            let (body_leading, blocks) = parse_blocks(&body_original);
            Self {
                fm_present: true,
                fm_lines: inner,
                prefix_original,
                body_original,
                body_leading,
                blocks,
                newline,
                fm_dirty: false,
                body_dirty: false,
            }
        } else {
            let (body_leading, blocks) = parse_blocks(raw);
            Self {
                fm_present: false,
                fm_lines: Vec::new(),
                prefix_original: String::new(),
                body_original: raw.to_string(),
                body_leading,
                blocks,
                newline,
                fm_dirty: false,
                body_dirty: false,
            }
        }
    }

    /// Build a brand-new document from structured fields.
    ///
    /// `steps` are raw Cooklang step texts (inline `@ingredient{}` markup and
    /// all).
    pub fn new_recipe(
        title: &str,
        servings: Option<&str>,
        tags: &[String],
        description: Option<&str>,
        source: Option<&str>,
        steps: &[String],
    ) -> Self {
        let mut fm_lines = vec![format!("title: {}", yaml_scalar(title))];
        if let Some(s) = servings.filter(|s| !s.trim().is_empty()) {
            fm_lines.push(format!("servings: {}", yaml_scalar(s)));
        }
        if let Some(s) = source.filter(|s| !s.trim().is_empty()) {
            fm_lines.push(format!("source: {}", yaml_scalar(s)));
        }
        if let Some(s) = description.filter(|s| !s.trim().is_empty()) {
            fm_lines.push(format!("description: {}", yaml_scalar(s)));
        }
        let clean_tags: Vec<String> = tags
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect();
        if !clean_tags.is_empty() {
            fm_lines.push(format!("tags: {}", clean_tags.join(", ")));
        }

        let blocks: Vec<Block> = steps
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(Block::new)
            .collect();

        Self {
            fm_present: true,
            fm_lines,
            prefix_original: String::new(),
            body_original: String::new(),
            body_leading: "\n".to_string(),
            blocks,
            newline: "\n".to_string(),
            fm_dirty: true,
            body_dirty: true,
        }
    }

    /// The body blocks, in order.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The body blocks with their resolved section context.
    pub fn sectioned_blocks(&self) -> Vec<SectionedBlock> {
        let mut current: Option<String> = None;
        let mut out = Vec::with_capacity(self.blocks.len());
        for b in &self.blocks {
            if b.kind == BlockKind::Section {
                current = b.section_name();
            }
            out.push(SectionedBlock {
                kind: b.kind,
                text: b.text.clone(),
                section: current.clone(),
            });
        }
        out
    }

    /// Replace the entire body with a new ordered list of blocks.
    pub fn set_blocks(&mut self, blocks: Vec<Block>) {
        self.blocks = blocks;
        self.body_dirty = true;
    }

    // ── Frontmatter accessors ─────────────────────────────────────

    /// Read a scalar metadata value by any of `keys` (first match wins).
    pub fn get(&self, keys: &[&str]) -> Option<String> {
        for line in &self.fm_lines {
            if let Some((k, v)) = split_kv(line)
                && keys.iter().any(|key| k.eq_ignore_ascii_case(key))
            {
                let v = unquote(v.trim());
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
        None
    }

    /// The recipe title, or `None` if unset.
    pub fn title(&self) -> Option<String> {
        self.get(&["title"])
    }

    /// The parsed tag list (supports inline `a, b` and YAML block form).
    pub fn tags(&self) -> Vec<String> {
        let Some(idx) = self.find_key("tags") else {
            return Vec::new();
        };
        let line = &self.fm_lines[idx];
        let inline = split_kv(line)
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default();
        if !inline.is_empty() {
            return inline
                .split(',')
                .map(|t| t.trim())
                .filter(|t| !t.is_empty())
                .map(|t| t.to_string())
                .collect();
        }
        // YAML block form: subsequent `  - item` lines.
        let mut out = Vec::new();
        for l in &self.fm_lines[idx + 1..] {
            let trimmed = l.trim_start();
            if let Some(item) = trimmed.strip_prefix("- ") {
                out.push(unquote(item.trim()));
            } else if trimmed == "-" {
                continue;
            } else {
                break;
            }
        }
        out
    }

    /// Set (or clear, when `value` is `None`/empty) a scalar metadata field.
    ///
    /// `primary` is the key used when inserting a new line; `keys` are all the
    /// recognised aliases used when locating an existing line. No-ops when the
    /// value is unchanged, so unrelated formatting is never disturbed.
    pub fn set_scalar(&mut self, primary: &str, keys: &[&str], value: Option<&str>) {
        let value = value.map(str::trim).filter(|s| !s.is_empty());
        if value.map(str::to_string) == self.get(keys) {
            return;
        }
        self.fm_dirty = true;
        let existing = self.find_key_any(keys);
        match (existing, value) {
            (Some(idx), Some(v)) => {
                self.fm_lines[idx] = format!("{primary}: {}", yaml_scalar(v));
            }
            (Some(idx), None) => {
                self.fm_lines.remove(idx);
            }
            (None, Some(v)) => {
                self.insert_fm_line(format!("{primary}: {}", yaml_scalar(v)));
            }
            (None, None) => {}
        }
    }

    /// Replace the tag list, preserving inline vs. block style where possible.
    pub fn set_tags(&mut self, tags: &[String]) {
        let clean: Vec<String> = tags
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect();
        if clean == self.tags() {
            return;
        }
        self.fm_dirty = true;

        // Remove any existing tags representation (line + block items).
        if let Some(idx) = self.find_key("tags") {
            let inline_empty = split_kv(&self.fm_lines[idx])
                .map(|(_, v)| v.trim().is_empty())
                .unwrap_or(true);
            let block_style = inline_empty;
            // Remove following block items first.
            let mut end = idx + 1;
            if block_style {
                while end < self.fm_lines.len() {
                    let t = self.fm_lines[end].trim_start();
                    if t.starts_with("- ") || t == "-" {
                        end += 1;
                    } else {
                        break;
                    }
                }
            }
            self.fm_lines.drain(idx..end);
            if clean.is_empty() {
                return;
            }
            if block_style {
                let mut insert = vec!["tags:".to_string()];
                for t in &clean {
                    insert.push(format!("  - {}", yaml_scalar(t)));
                }
                for (offset, line) in insert.into_iter().enumerate() {
                    self.fm_lines.insert(idx + offset, line);
                }
            } else {
                self.fm_lines
                    .insert(idx, format!("tags: {}", clean.join(", ")));
            }
            return;
        }

        if !clean.is_empty() {
            self.insert_fm_line(format!("tags: {}", clean.join(", ")));
        }
    }

    fn find_key(&self, key: &str) -> Option<usize> {
        self.find_key_any(&[key])
    }

    fn find_key_any(&self, keys: &[&str]) -> Option<usize> {
        self.fm_lines.iter().position(|line| {
            split_kv(line)
                .map(|(k, _)| keys.iter().any(|key| k.eq_ignore_ascii_case(key)))
                .unwrap_or(false)
        })
    }

    /// Insert a new frontmatter line after `title` (or at the end).
    fn insert_fm_line(&mut self, line: String) {
        match self.find_key("title") {
            Some(idx) => self.fm_lines.insert(idx + 1, line),
            None => self.fm_lines.push(line),
        }
    }

    fn original(&self) -> String {
        format!("{}{}", self.prefix_original, self.body_original)
    }

    /// Emit the document back to `.cook` text.
    pub fn emit(&self) -> String {
        if !self.fm_dirty && !self.body_dirty {
            return self.original();
        }

        let mut out = String::new();
        let nl = self.newline.as_str();

        // A document may gain a frontmatter fence it didn't originally have
        // (e.g. the first metadata field set on a bare `.cook` file).
        let has_fm = self.fm_present || !self.fm_lines.is_empty();
        let created_fm = has_fm && !self.fm_present;

        if has_fm {
            if self.fm_dirty || created_fm {
                out.push_str("---");
                out.push_str(nl);
                for line in &self.fm_lines {
                    out.push_str(line);
                    out.push_str(nl);
                }
                out.push_str("---");
                out.push_str(nl);
            } else {
                out.push_str(&self.prefix_original);
            }
        }

        if self.body_dirty {
            let mut leading = if self.fm_present {
                // Ensure at least one blank line separates fence from body.
                if self.body_leading.is_empty() {
                    nl.to_string()
                } else {
                    self.body_leading.clone()
                }
            } else {
                self.body_leading.clone()
            };
            // When the frontmatter was rebuilt it already ends in a newline;
            // the body_leading supplies the blank separator line.
            if self.blocks.is_empty() {
                leading.push_str(nl);
                out.push_str(&leading);
                return normalize_trailing(out, nl);
            }
            out.push_str(&leading);
            let separator = format!("{nl}{nl}");
            let joined = self
                .blocks
                .iter()
                .map(|b| b.text.trim_end().replace("\r\n", "\n").replace('\n', nl))
                .collect::<Vec<_>>()
                .join(&separator);
            out.push_str(&joined);
            out.push_str(nl);
        } else {
            // A newly-created frontmatter needs a blank line before a body
            // that never had one.
            if created_fm && !self.body_original.starts_with('\n') && !self.body_original.is_empty()
            {
                out.push_str(nl);
            }
            out.push_str(&self.body_original);
        }

        normalize_trailing(out, nl)
    }

    /// Emit and compute the resulting slug from the title.
    pub fn slug(&self) -> String {
        self.title().map(|t| slugify(&t)).unwrap_or_default()
    }
}

fn normalize_trailing(mut s: String, nl: &str) -> String {
    // Collapse accidental trailing blank lines, keeping exactly one newline.
    let double = format!("{nl}{nl}");
    while s.ends_with(&double) {
        for _ in 0..nl.len() {
            s.pop();
        }
    }
    if !s.ends_with('\n') {
        s.push_str(nl);
    }
    s
}

/// Detect a raw document's dominant line ending. Returns `"\r\n"` if the first
/// line break is a CRLF, otherwise `"\n"`.
fn detect_newline(raw: &str) -> String {
    match raw.find('\n') {
        Some(i) if i > 0 && raw.as_bytes()[i - 1] == b'\r' => "\r\n".to_string(),
        _ => "\n".to_string(),
    }
}

/// Detect a leading `--- ... ---` frontmatter fence.
///
/// Returns the inner lines (verbatim, without fences) and the byte length of
/// the whole frontmatter prefix (including the closing fence and its newline).
fn parse_frontmatter(raw: &str) -> Option<(Vec<String>, usize)> {
    let mut lines = raw.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let mut inner = Vec::new();
    let mut consumed = first.len();
    let mut closed = false;
    for line in lines {
        consumed += line.len();
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        inner.push(
            line.trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string(),
        );
    }
    if closed {
        Some((inner, consumed))
    } else {
        None
    }
}

/// Split a body string into its leading whitespace and paragraph blocks.
fn parse_blocks(body: &str) -> (String, Vec<Block>) {
    let leading: String = body.chars().take_while(|c| c.is_whitespace()).collect();
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return (leading, Vec::new());
    }

    let mut blocks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in trimmed.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(Block::new(current.join("\n")));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(Block::new(current.join("\n")));
    }
    (leading, blocks)
}

/// Split a `key: value` line. Returns `None` for list items / blank / non-kv.
fn split_kv(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('-') || trimmed.starts_with('#') {
        return None;
    }
    // Keys are unindented; indented lines belong to a block value.
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let idx = line.find(':')?;
    let key = line[..idx].trim();
    if key.is_empty() {
        return None;
    }
    Some((key, &line[idx + 1..]))
}

/// Strip surrounding matching quotes from a scalar value.
fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

/// Quote a scalar value for YAML frontmatter when it would otherwise be
/// ambiguous (contains `:`/`#`, leading/trailing space, or starts with a
/// YAML indicator).
fn yaml_scalar(value: &str) -> String {
    let needs_quote = value.is_empty()
        || value != value.trim()
        || value.contains(": ")
        || value.ends_with(':')
        || value.contains(" #")
        || value.starts_with([
            '#', '"', '\'', '[', ']', '{', '}', '&', '*', '!', '|', '>', '%', '@', '`',
        ])
        || value.starts_with('-') && value.len() > 1 && value.as_bytes()[1] == b' ';
    if needs_quote {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}
