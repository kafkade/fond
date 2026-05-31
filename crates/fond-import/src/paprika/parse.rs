use std::io::Read;
use std::path::Path;

use super::types::{PaprikaRecipe, ParsedEntry};
use crate::ImportError;

/// Parse a single `.paprikarecipe` file (gzip-compressed JSON).
pub fn parse_paprikarecipe(data: &[u8]) -> Result<PaprikaRecipe, String> {
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut json = String::new();
    decoder
        .read_to_string(&mut json)
        .map_err(|e| format!("gzip decode failed: {e}"))?;
    serde_json::from_str(&json).map_err(|e| format!("JSON parse failed: {e}"))
}

/// Parse a `.paprikarecipes` ZIP archive, yielding each entry one at a time.
///
/// Entries are processed individually to limit memory usage — the `photo`
/// field is skipped during deserialization, and each recipe is independent.
///
/// Returns all successfully parsed recipes and any per-entry errors.
pub fn parse_paprikarecipes_archive(data: &[u8]) -> (Vec<PaprikaRecipe>, Vec<ParsedEntry>) {
    let cursor = std::io::Cursor::new(data);
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(e) => {
            return (
                Vec::new(),
                vec![ParsedEntry {
                    entry_name: "<archive>".to_string(),
                    result: Err(format!("invalid ZIP archive: {e}")),
                }],
            );
        }
    };

    let mut recipes = Vec::new();
    let mut errors = Vec::new();

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(e) => {
                errors.push(ParsedEntry {
                    entry_name: format!("entry #{i}"),
                    result: Err(format!("failed to read ZIP entry: {e}")),
                });
                continue;
            }
        };

        let name = entry.name().to_string();

        if entry.is_dir() {
            continue;
        }

        let mut compressed = Vec::new();
        if let Err(e) = entry.read_to_end(&mut compressed) {
            errors.push(ParsedEntry {
                entry_name: name,
                result: Err(format!("failed to read entry data: {e}")),
            });
            continue;
        }

        match parse_paprikarecipe(&compressed) {
            Ok(recipe) => recipes.push(recipe),
            Err(e) => {
                errors.push(ParsedEntry {
                    entry_name: name,
                    result: Err(e),
                });
            }
        }
    }

    (recipes, errors)
}

/// Read and parse a Paprika export file from disk.
///
/// Accepts both `.paprikarecipe` (single) and `.paprikarecipes` (archive).
/// The format is detected by file extension.
pub fn read_paprika_file(
    path: &Path,
) -> Result<(Vec<PaprikaRecipe>, Vec<ParsedEntry>), ImportError> {
    let data = std::fs::read(path)?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "paprikarecipes" => Ok(parse_paprikarecipes_archive(&data)),
        "paprikarecipe" => match parse_paprikarecipe(&data) {
            Ok(recipe) => Ok((vec![recipe], Vec::new())),
            Err(e) => {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("<unknown>")
                    .to_string();
                Ok((
                    Vec::new(),
                    vec![ParsedEntry {
                        entry_name: file_name,
                        result: Err(e),
                    }],
                ))
            }
        },
        _ => Err(ImportError::InvalidArchive(format!(
            "unsupported file extension '.{ext}' — expected .paprikarecipe or .paprikarecipes"
        ))),
    }
}
