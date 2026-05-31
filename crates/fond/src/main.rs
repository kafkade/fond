use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use comfy_table::{ContentArrangement, Table};
use fond_domain::{RecipeFilter, escape_fts5_query};
use fond_import::paprika;
use fond_store::{FondDb, FondPaths, RecipeRepository};
use serde::Serialize;

/// fond — a private, local-first personal cooking & recipe manager.
#[derive(Parser)]
#[command(name = "fond", version, about)]
struct Cli {
    /// Data directory (default: platform-specific)
    #[arg(long, env = "FOND_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,

    /// Output format (table or json)
    #[arg(long, default_value = "table", global = true)]
    format: OutputFormat,

    /// Shorthand for --format json
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Table,
    Json,
}

#[derive(Clone, ValueEnum)]
enum ExportFormat {
    Json,
    Paprika,
}

impl Cli {
    /// Resolve the effective output format (--json overrides --format).
    fn output_format(&self) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else {
            self.format.clone()
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise the fond data directory and default structure.
    Init,

    /// Add a recipe to the collection.
    Add {
        /// Path to an existing .cook file to ingest
        #[arg(long, short)]
        file: Option<PathBuf>,

        /// Title for a new recipe (creates a minimal .cook file)
        #[arg(long, short)]
        title: Option<String>,
    },

    /// Open a recipe in your editor.
    Edit {
        /// Recipe slug (e.g., "chicken-adobo")
        slug: String,
    },

    /// Display a recipe by slug.
    View {
        /// Recipe slug (e.g., "chicken-adobo")
        slug: String,
    },

    /// List all indexed recipes.
    List {
        /// Filter by tag (repeatable, AND semantics)
        #[arg(long, short)]
        tag: Vec<String>,

        /// Maximum total time in minutes
        #[arg(long)]
        max_time: Option<u32>,

        /// Filter by cuisine (matches tags)
        #[arg(long)]
        cuisine: Option<String>,

        /// Filter by source (substring match)
        #[arg(long)]
        source: Option<String>,
    },

    /// Search recipes by keyword.
    Search {
        /// Search query
        query: String,

        /// Filter by tag (repeatable, AND semantics)
        #[arg(long, short)]
        tag: Vec<String>,

        /// Maximum total time in minutes
        #[arg(long)]
        max_time: Option<u32>,

        /// Filter by cuisine (matches tags)
        #[arg(long)]
        cuisine: Option<String>,

        /// Filter by source (substring match)
        #[arg(long)]
        source: Option<String>,
    },

    /// Manage recipe tags.
    Tag {
        /// Recipe slug (omit to list all tags)
        slug: Option<String>,

        /// Tags to add (comma-separated)
        #[arg(long)]
        add: Option<String>,

        /// Tags to remove (comma-separated)
        #[arg(long)]
        remove: Option<String>,

        /// List all tags with counts
        #[arg(long, short)]
        list: bool,
    },

    /// Remove a recipe (file and index entry).
    Rm {
        /// Recipe slug
        slug: String,

        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },

    /// Rebuild the search index from .cook files on disk.
    Reindex,

    /// Manage your pantry (what's in your kitchen).
    Pantry {
        #[command(subcommand)]
        action: PantryAction,
    },

    /// Generate a grocery (shopping) list.
    Grocery {
        #[command(subcommand)]
        action: GroceryAction,
    },

    /// Import recipes from an external source.
    Import {
        #[command(subcommand)]
        source: ImportSource,
    },

    /// Export recipes to JSON or Paprika format.
    Export {
        /// Export format (json or paprika)
        #[arg(long = "export-format", default_value = "json")]
        export_format: ExportFormat,

        /// Export a single recipe by slug (omit for full collection)
        #[arg(long)]
        recipe: Option<String>,

        /// Output file path (required for Paprika; optional for JSON, defaults to stdout)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },

    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum PantryAction {
    /// Add items to your pantry (mark as available).
    Add {
        /// Items to add (e.g., flour eggs "olive oil")
        #[arg(required = true)]
        items: Vec<String>,
    },

    /// Remove items from your pantry (mark as unavailable).
    Rm {
        /// Items to remove
        #[arg(required = true)]
        items: Vec<String>,
    },

    /// List items in your pantry.
    List {
        /// Show absent items too (not just present)
        #[arg(long)]
        all: bool,
    },

    /// Check pantry coverage for a recipe.
    Check {
        /// Recipe slug (e.g., "chicken-adobo")
        slug: String,
    },
}

#[derive(Subcommand)]
enum GroceryAction {
    /// Generate a shopping list from a recipe.
    ///
    /// Subtracts pantry items and groups by aisle/category.
    FromRecipe {
        /// Recipe slug (e.g., "chicken-adobo")
        slug: String,

        /// Include items already in pantry (marked as covered)
        #[arg(long)]
        include_pantry: bool,
    },
}

#[derive(Subcommand)]
enum ImportSource {
    /// Import recipes from a Paprika export file.
    ///
    /// Accepts .paprikarecipes (batch export) or .paprikarecipe (single).
    Paprika {
        /// Path to the Paprika export file
        path: PathBuf,

        /// Preview what would be imported without writing any files
        #[arg(long)]
        dry_run: bool,
    },

    /// Import a recipe from a URL (schema.org/JSON-LD extraction).
    ///
    /// Fetches the page, extracts structured recipe data from JSON-LD,
    /// and falls back to HTML scraping if no structured data is found.
    Url {
        /// URL to import from
        url: String,

        /// Preview what would be imported without writing any files
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = FondPaths::resolve(cli.data_dir.clone());
    let fmt = cli.output_format();

    match cli.command {
        Commands::Init => cmd_init(&paths),
        Commands::Add { file, title } => cmd_add(&paths, file, title, &fmt),
        Commands::Edit { slug } => cmd_edit(&paths, &slug),
        Commands::View { slug } => cmd_view(&paths, &slug, &fmt),
        Commands::List {
            tag,
            max_time,
            cuisine,
            source,
        } => cmd_list(&paths, &fmt, tag, max_time, cuisine, source),
        Commands::Search {
            query,
            tag,
            max_time,
            cuisine,
            source,
        } => cmd_search(&paths, &query, &fmt, tag, max_time, cuisine, source),
        Commands::Tag {
            slug,
            add,
            remove,
            list,
        } => cmd_tag(&paths, slug, add, remove, list, &fmt),
        Commands::Rm { slug, yes } => cmd_rm(&paths, &slug, yes, &fmt),
        Commands::Reindex => cmd_reindex(&paths, &fmt),
        Commands::Pantry { action } => match action {
            PantryAction::Add { items } => cmd_pantry_add(&paths, &items, &fmt),
            PantryAction::Rm { items } => cmd_pantry_rm(&paths, &items, &fmt),
            PantryAction::List { all } => cmd_pantry_list(&paths, all, &fmt),
            PantryAction::Check { slug } => cmd_pantry_check(&paths, &slug, &fmt),
        },
        Commands::Grocery { action } => match action {
            GroceryAction::FromRecipe {
                slug,
                include_pantry,
            } => cmd_grocery_from_recipe(&paths, &slug, include_pantry, &fmt),
        },
        Commands::Import { source } => match source {
            ImportSource::Paprika { path, dry_run } => {
                cmd_import_paprika(&paths, &path, dry_run, &fmt)
            }
            ImportSource::Url { url, dry_run } => cmd_import_url(&paths, &url, dry_run, &fmt),
        },
        Commands::Export {
            export_format,
            recipe,
            output,
        } => cmd_export(&paths, &export_format, recipe, output),
        Commands::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "fond", &mut io::stdout());
            Ok(())
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

fn open_db(paths: &FondPaths) -> Result<FondDb> {
    let db_path = paths.data_dir.join("fond.db");
    FondDb::open(&db_path).context("failed to open database")
}

fn recipes_dir(paths: &FondPaths) -> PathBuf {
    paths.data_dir.join("recipes")
}

fn content_hash(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn open_editor(file_path: &std::path::Path) -> Result<bool> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "notepad".to_string()
            } else {
                "nano".to_string()
            }
        });

    let status = std::process::Command::new(&editor)
        .arg(file_path)
        .status()
        .with_context(|| format!("failed to open editor '{editor}'"))?;

    Ok(status.success())
}

fn confirm(prompt: &str) -> bool {
    eprint!("{prompt} [y/N] ");
    io::stderr().flush().ok();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_ok() {
        let answer = line.trim().to_lowercase();
        answer == "y" || answer == "yes"
    } else {
        false
    }
}

// ═══════════════════════════════════════════════════════════════════
// Commands
// ═══════════════════════════════════════════════════════════════════

fn cmd_init(paths: &FondPaths) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    println!("Initialised fond at {}", paths.data_dir.display());
    println!("  recipes/  — your .cook recipe files");
    println!("  config/   — fond configuration");
    Ok(())
}

fn cmd_add(
    paths: &FondPaths,
    file: Option<PathBuf>,
    title: Option<String>,
    fmt: &OutputFormat,
) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let is_json = matches!(fmt, OutputFormat::Json);
    let dest_dir = recipes_dir(paths);

    let dest_path = if let Some(source) = file {
        // Mode 1: ingest an existing .cook file
        add_from_file(&source, &dest_dir)?
    } else if let Some(ref t) = title {
        // Mode 2: create from title
        add_from_title(t, &dest_dir, is_json)?
    } else if is_json {
        anyhow::bail!("JSON mode is non-interactive — pass --file <path> or --title <name>");
    } else {
        // Mode 3: interactive — ask for title, then open editor
        eprint!("Recipe title: ");
        io::stderr().flush().ok();
        let mut t = String::new();
        io::stdin()
            .lock()
            .read_line(&mut t)
            .context("failed to read title")?;
        let t = t.trim().to_string();
        if t.is_empty() {
            anyhow::bail!("title cannot be empty");
        }
        add_from_title(&t, &dest_dir, is_json)?
    };

    // Parse and index the new recipe
    let content = std::fs::read_to_string(&dest_path).context("failed to read new recipe file")?;
    let stem = dest_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("recipe");
    let recipe = fond_domain::parse_cook(&content, stem)
        .map_err(|e| anyhow::anyhow!("failed to parse new recipe: {e}"))?;

    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);
    let hash = content_hash(&content);
    let file_name = dest_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("recipe.cook");
    repo.upsert_recipe(file_name, &recipe, &hash)
        .context("failed to index recipe")?;

    match fmt {
        OutputFormat::Json => {
            let out = serde_json::json!({
                "action": "added",
                "slug": recipe.slug,
                "title": recipe.title,
                "file": file_name,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            println!("Added: {} ({})", recipe.title, recipe.slug);
            println!("  File: {}", dest_path.display());
        }
    }

    Ok(())
}

fn add_from_file(source: &std::path::Path, dest_dir: &std::path::Path) -> Result<PathBuf> {
    if !source.exists() {
        anyhow::bail!("file not found: {}", source.display());
    }

    let file_name = source
        .file_name()
        .context("source path has no filename")?
        .to_str()
        .context("filename is not valid UTF-8")?;

    if !file_name.ends_with(".cook") {
        anyhow::bail!(
            "expected a .cook file, got '{file_name}' — rename it or use --title instead"
        );
    }

    // Validate the file parses before copying
    let content = std::fs::read_to_string(source).context("failed to read source .cook file")?;
    let stem = file_name.trim_end_matches(".cook");
    let recipe = fond_domain::parse_cook(&content, stem)
        .map_err(|e| anyhow::anyhow!("file is not valid Cooklang: {e}"))?;

    // Check for filename collision
    let dest = dest_dir.join(file_name);
    if dest.exists() {
        anyhow::bail!(
            "a recipe file named '{file_name}' already exists — rename the source file or remove the existing one with `fond rm {}`",
            recipe.slug
        );
    }

    std::fs::copy(source, &dest)
        .with_context(|| format!("failed to copy {} → {}", source.display(), dest.display()))?;

    Ok(dest)
}

fn add_from_title(title: &str, dest_dir: &std::path::Path, is_json: bool) -> Result<PathBuf> {
    let slug = fond_domain::slugify(title);
    let file_name = format!("{slug}.cook");
    let dest = dest_dir.join(&file_name);

    if dest.exists() {
        anyhow::bail!(
            "a recipe file named '{file_name}' already exists — choose a different title or remove the existing one with `fond rm {slug}`"
        );
    }

    // Create a minimal .cook file
    let content = format!(
        "---\ntitle: {title}\nservings: 4\ntags: \n---\n\n\
         -- Add your ingredients and steps below.\n\
         -- See https://cooklang.org for Cooklang syntax.\n\n"
    );
    std::fs::write(&dest, &content)
        .with_context(|| format!("failed to write {}", dest.display()))?;

    // Open editor unless in JSON mode
    if !is_json {
        eprintln!("Opening {} in your editor...", dest.display());
        if !open_editor(&dest)? {
            eprintln!("Editor exited with an error — file saved but may need editing.");
        }
    }

    Ok(dest)
}

fn cmd_edit(paths: &FondPaths, slug: &str) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let record = repo
        .get_recipe_by_slug(slug)
        .context("database query failed")?
        .with_context(|| {
            format!("no recipe found with slug '{slug}' — run `fond list` to see available recipes")
        })?;

    let dir = recipes_dir(paths);
    let file_path = dir.join(&record.file_path);

    if !file_path.exists() {
        anyhow::bail!(
            "recipe file not found at {} — run `fond reindex` to repair the index",
            file_path.display()
        );
    }

    // Open in editor
    if !open_editor(&file_path)? {
        eprintln!("Editor exited with an error — changes may not have been saved.");
        return Ok(());
    }

    // Re-parse and re-index after editing
    let content =
        std::fs::read_to_string(&file_path).context("failed to read edited recipe file")?;
    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(slug);

    match fond_domain::parse_cook(&content, stem) {
        Ok(recipe) => {
            let hash = content_hash(&content);
            repo.upsert_recipe(&record.file_path, &recipe, &hash)
                .context("failed to re-index recipe after edit")?;

            if recipe.slug != record.slug {
                eprintln!(
                    "Note: slug changed from '{}' to '{}'",
                    record.slug, recipe.slug
                );
            }
            println!("Updated: {} ({})", recipe.title, recipe.slug);
        }
        Err(e) => {
            eprintln!(
                "Warning: edited file has parse errors — index not updated.\n  \
                 Error: {e}\n  \
                 Fix the file and run `fond reindex` to repair."
            );
        }
    }

    Ok(())
}

fn cmd_view(paths: &FondPaths, slug: &str, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let record = repo
        .get_recipe_by_slug(slug)
        .context("database query failed")?
        .with_context(|| {
            format!("no recipe found with slug '{slug}' — run `fond list` to see available recipes")
        })?;

    // Parse from file for full fidelity
    let dir = recipes_dir(paths);
    let file_path = dir.join(&record.file_path);

    let content = if file_path.exists() {
        std::fs::read_to_string(&file_path).context("failed to read recipe file")?
    } else if !record.raw_source.is_empty() {
        record.raw_source.clone()
    } else {
        anyhow::bail!(
            "recipe file not found: {} — run `fond reindex` to repair",
            record.file_path
        );
    };

    let recipe = fond_domain::parse_cook(&content, slug)
        .map_err(|e| anyhow::anyhow!("failed to parse recipe: {e}"))?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&recipe)?);
        }
        OutputFormat::Table => {
            print_recipe_human(&recipe);
        }
    }

    Ok(())
}

fn print_recipe_human(recipe: &fond_domain::Recipe) {
    println!("# {}", recipe.title);
    if let Some(ref source) = recipe.source {
        println!("Source: {source}");
    }
    if let Some(ref s) = recipe.servings {
        println!("Servings: {s}");
    }
    let mut timing = Vec::new();
    if let Some(ref t) = recipe.prep_time {
        timing.push(format!("Prep: {t}"));
    }
    if let Some(ref t) = recipe.cook_time {
        timing.push(format!("Cook: {t}"));
    }
    if let Some(ref t) = recipe.total_time {
        timing.push(format!("Total: {t}"));
    }
    if !timing.is_empty() {
        println!("{}", timing.join("  "));
    }
    if !recipe.tags.is_empty() {
        println!("Tags: {}", recipe.tags.join(", "));
    }

    println!("\n## Ingredients\n");
    for ing in &recipe.ingredients {
        let qty = match (&ing.quantity, &ing.unit) {
            (Some(q), Some(u)) => format!("{q} {u} "),
            (Some(q), None) => format!("{q} "),
            _ => String::new(),
        };
        println!("  - {qty}{}", ing.name);
    }

    println!("\n## Steps\n");
    let mut current_section: Option<&str> = None;
    for step in &recipe.steps {
        let section = step.section.as_deref();
        if section != current_section {
            if let Some(name) = section
                && !name.is_empty()
            {
                println!("\n### {name}\n");
            }
            current_section = section;
        }
        println!("  {}. {}", step.order + 1, step.body);
    }
}

fn cmd_list(
    paths: &FondPaths,
    fmt: &OutputFormat,
    tags: Vec<String>,
    max_time: Option<u32>,
    cuisine: Option<String>,
    source: Option<String>,
) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let filter = build_cli_filter(tags, max_time, cuisine, source);
    let recipes = repo
        .list_recipes_filtered(&filter)
        .context("failed to list recipes")?;

    if recipes.is_empty() {
        match fmt {
            OutputFormat::Json => println!("[]"),
            OutputFormat::Table => {
                if filter.is_empty() {
                    println!("No recipes indexed. Add .cook files and run `fond reindex`.");
                } else {
                    println!("No recipes match the given filters.");
                }
            }
        }
        return Ok(());
    }

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&recipes)?);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["Slug", "Title", "Source", "Tags", "Time"]);

            for r in &recipes {
                let tags = if r.tags.is_empty() {
                    String::new()
                } else {
                    r.tags.join(", ")
                };
                let source = if r.source.is_empty() {
                    "\u{2014}".to_string()
                } else {
                    r.source.clone()
                };
                let time = if r.total_time.is_empty() {
                    "\u{2014}".to_string()
                } else {
                    r.total_time.clone()
                };
                table.add_row(vec![&r.slug, &r.title, &source, &tags, &time]);
            }

            println!("{table}");
            println!("\n{} recipe(s)", recipes.len());
        }
    }
    Ok(())
}

fn cmd_search(
    paths: &FondPaths,
    query: &str,
    fmt: &OutputFormat,
    tags: Vec<String>,
    max_time: Option<u32>,
    cuisine: Option<String>,
    source: Option<String>,
) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let filter = build_cli_filter(tags, max_time, cuisine, source);

    // Escape user input for safe FTS5 MATCH
    let escaped_query = escape_fts5_query(query);
    if escaped_query.is_empty() {
        match fmt {
            OutputFormat::Json => println!("[]"),
            OutputFormat::Table => println!("Empty search query."),
        }
        return Ok(());
    }

    let results = repo
        .search_filtered(&escaped_query, &filter)
        .context("search failed")?;

    if results.is_empty() {
        match fmt {
            OutputFormat::Json => println!("[]"),
            OutputFormat::Table => println!("No results for '{query}'."),
        }
        return Ok(());
    }

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["Slug", "Title", "Source", "Tags"]);

            for r in &results {
                let tags = if r.tags.is_empty() {
                    String::new()
                } else {
                    r.tags.join(", ")
                };
                let source = if r.source.is_empty() {
                    "\u{2014}".to_string()
                } else {
                    r.source.clone()
                };
                table.add_row(vec![&r.slug, &r.title, &source, &tags]);
            }

            println!("{table}");
            println!("\n{} result(s)", results.len());
        }
    }
    Ok(())
}

fn cmd_tag(
    paths: &FondPaths,
    slug: Option<String>,
    add: Option<String>,
    remove: Option<String>,
    list: bool,
    fmt: &OutputFormat,
) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    // Mode 1: list all tags
    if list || (slug.is_none() && add.is_none() && remove.is_none()) {
        let tags = repo.list_tags().context("failed to list tags")?;

        if tags.is_empty() {
            match fmt {
                OutputFormat::Json => println!("[]"),
                OutputFormat::Table => println!("No tags found."),
            }
            return Ok(());
        }

        match fmt {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&tags)?);
            }
            OutputFormat::Table => {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["Tag", "Recipes"]);

                for t in &tags {
                    table.add_row(vec![&t.name, &t.count.to_string()]);
                }

                println!("{table}");
                println!("\n{} tag(s)", tags.len());
            }
        }
        return Ok(());
    }

    // Mode 2: modify tags on a specific recipe
    let slug = slug.context(
        "recipe slug is required for --add / --remove — use `fond tag --list` to list all tags",
    )?;

    // Parse comma-separated tag lists
    let tags_to_add: Vec<String> = add
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let tags_to_remove: Vec<String> = remove
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if tags_to_add.is_empty() && tags_to_remove.is_empty() {
        // Show tags for this recipe
        let result = repo
            .get_tags_for_slug(&slug)
            .context("failed to query tags")?
            .with_context(|| format!("no recipe found with slug '{slug}'"))?;

        let (_, current_tags) = result;

        match fmt {
            OutputFormat::Json => {
                let out = serde_json::json!({
                    "slug": slug,
                    "tags": current_tags,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
            OutputFormat::Table => {
                if current_tags.is_empty() {
                    println!("{slug}: (no tags)");
                } else {
                    println!("{slug}: {}", current_tags.join(", "));
                }
            }
        }
        return Ok(());
    }

    // Get current tags and file path
    let record = repo
        .get_recipe_by_slug(&slug)
        .context("database query failed")?
        .with_context(|| format!("no recipe found with slug '{slug}'"))?;

    let result = repo
        .get_tags_for_slug(&slug)
        .context("failed to query tags")?
        .with_context(|| format!("no recipe found with slug '{slug}'"))?;

    let (_, current_tags) = result;

    // Compute new tag set
    let mut new_tags: Vec<String> = current_tags.clone();
    for tag in &tags_to_add {
        if !new_tags.contains(tag) {
            new_tags.push(tag.clone());
        }
    }
    new_tags.retain(|t| !tags_to_remove.contains(t));
    new_tags.sort();

    // Update the .cook file on disk (source of truth)
    let dir = recipes_dir(paths);
    let file_path = dir.join(&record.file_path);

    if !file_path.exists() {
        anyhow::bail!(
            "recipe file not found at {} — run `fond reindex` to repair",
            file_path.display()
        );
    }

    let content = std::fs::read_to_string(&file_path).context("failed to read recipe file")?;
    let updated_content = fond_domain::update_tags_in_cook_source(&content, &new_tags);

    // Write atomically: temp file then rename
    let tmp_path = file_path.with_extension("cook.tmp");
    std::fs::write(&tmp_path, &updated_content)
        .with_context(|| format!("failed to write temp file: {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &file_path)
        .with_context(|| format!("failed to rename temp file to {}", file_path.display()))?;

    // Re-parse and re-index from the updated file
    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&slug);

    let recipe = fond_domain::parse_cook(&updated_content, stem)
        .map_err(|e| anyhow::anyhow!("failed to parse updated recipe: {e}"))?;

    let hash = content_hash(&updated_content);
    repo.upsert_recipe(&record.file_path, &recipe, &hash)
        .context("failed to re-index recipe after tag update")?;

    match fmt {
        OutputFormat::Json => {
            let out = serde_json::json!({
                "slug": slug,
                "tags": new_tags,
                "added": tags_to_add,
                "removed": tags_to_remove,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            if !tags_to_add.is_empty() {
                println!("Added: {}", tags_to_add.join(", "));
            }
            if !tags_to_remove.is_empty() {
                println!("Removed: {}", tags_to_remove.join(", "));
            }
            println!(
                "Tags for {slug}: {}",
                if new_tags.is_empty() {
                    "(none)".to_string()
                } else {
                    new_tags.join(", ")
                }
            );
        }
    }

    Ok(())
}

/// Build a `RecipeFilter` from CLI flags.
fn build_cli_filter(
    mut tags: Vec<String>,
    max_time: Option<u32>,
    cuisine: Option<String>,
    source: Option<String>,
) -> RecipeFilter {
    // --cuisine is sugar for --tag (cuisines are tags in Cooklang)
    if let Some(c) = cuisine {
        let normalized = c.trim().to_lowercase();
        if !normalized.is_empty() && !tags.contains(&normalized) {
            tags.push(normalized);
        }
    }

    RecipeFilter {
        tags,
        max_time_minutes: max_time,
        source,
    }
}

fn cmd_rm(paths: &FondPaths, slug: &str, skip_confirm: bool, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let record = repo
        .get_recipe_by_slug(slug)
        .context("database query failed")?
        .with_context(|| {
            format!("no recipe found with slug '{slug}' — run `fond list` to see available recipes")
        })?;

    let dir = recipes_dir(paths);
    let file_path = dir.join(&record.file_path);

    if !skip_confirm && !matches!(fmt, OutputFormat::Json) {
        let prompt = format!(
            "Remove '{}' ({})?\n  File: {}",
            record.title,
            record.slug,
            file_path.display()
        );
        if !confirm(&prompt) {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Delete file first (source of truth)
    if file_path.exists() {
        std::fs::remove_file(&file_path)
            .with_context(|| format!("failed to remove file: {}", file_path.display()))?;
    }

    // Then delete from DB
    match repo.delete_recipe_by_slug(slug) {
        Ok(_) => {}
        Err(e) => {
            eprintln!(
                "Warning: file removed but index cleanup failed: {e}\n  \
                 Run `fond reindex` to repair."
            );
        }
    }

    match fmt {
        OutputFormat::Json => {
            let out = serde_json::json!({
                "action": "removed",
                "slug": record.slug,
                "title": record.title,
                "file": record.file_path,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            println!("Removed: {} ({})", record.title, record.slug);
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Pantry
// ═══════════════════════════════════════════════════════════════════

fn cmd_pantry_add(paths: &FondPaths, items: &[String], fmt: &OutputFormat) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db = open_db(paths)?;
    let pantry = fond_store::PantryRepository::new(&db);

    let item_refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    let added = pantry
        .add_items(&item_refs)
        .context("failed to add pantry items")?;

    match fmt {
        OutputFormat::Json => {
            let out = serde_json::json!({
                "action": "add",
                "items": added,
                "count": added.len(),
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            if added.is_empty() {
                println!("No items to add.");
            } else {
                println!(
                    "Added {} item(s) to pantry: {}",
                    added.len(),
                    added.join(", ")
                );
            }
        }
    }

    Ok(())
}

fn cmd_pantry_rm(paths: &FondPaths, items: &[String], fmt: &OutputFormat) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db = open_db(paths)?;
    let pantry = fond_store::PantryRepository::new(&db);

    let item_refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    let removed = pantry
        .remove_items(&item_refs)
        .context("failed to remove pantry items")?;

    match fmt {
        OutputFormat::Json => {
            let out = serde_json::json!({
                "action": "remove",
                "items": removed,
                "count": removed.len(),
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            if removed.is_empty() {
                println!("No matching items found in pantry.");
            } else {
                println!(
                    "Removed {} item(s) from pantry: {}",
                    removed.len(),
                    removed.join(", ")
                );
            }
        }
    }

    Ok(())
}

fn cmd_pantry_list(paths: &FondPaths, show_all: bool, fmt: &OutputFormat) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db = open_db(paths)?;
    let pantry = fond_store::PantryRepository::new(&db);

    let items = pantry
        .list_items(show_all)
        .context("failed to list pantry items")?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&items)?);
        }
        OutputFormat::Table => {
            if items.is_empty() {
                if show_all {
                    println!("Pantry is empty. Use `fond pantry add` to add items.");
                } else {
                    println!("No items in pantry. Use `fond pantry add` to add items.");
                }
            } else {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);

                if show_all {
                    table.set_header(vec!["Item", "Status"]);
                    for item in &items {
                        let status = if item.present {
                            "\u{2713} have".to_string()
                        } else {
                            "\u{2717} need".to_string()
                        };
                        table.add_row(vec![item.name.clone(), status]);
                    }
                } else {
                    table.set_header(vec!["Item"]);
                    for item in &items {
                        table.add_row(vec![item.name.clone()]);
                    }
                }

                println!("{table}");
                println!("{} item(s)", items.len());
            }
        }
    }

    Ok(())
}

fn cmd_pantry_check(paths: &FondPaths, slug: &str, fmt: &OutputFormat) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db = open_db(paths)?;
    let pantry = fond_store::PantryRepository::new(&db);

    let coverage = pantry
        .check_coverage(slug)
        .context("failed to check pantry coverage")?;

    let Some(coverage) = coverage else {
        anyhow::bail!("recipe not found: {slug}");
    };

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&coverage)?);
        }
        OutputFormat::Table => {
            println!(
                "{} — {:.0}% coverage",
                coverage.recipe_title, coverage.coverage_pct
            );
            println!(
                "{}/{} ingredients available",
                coverage.matched_count, coverage.total_ingredients
            );
            println!();

            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["Ingredient", "Status", "Matched By"]);

            for ing in &coverage.ingredients {
                let status = if ing.matched {
                    "\u{2713} have".to_string()
                } else if ing.optional {
                    "? optional".to_string()
                } else {
                    "\u{2717} need".to_string()
                };
                let matched_by = ing.matched_pantry_item.as_deref().unwrap_or("").to_string();
                table.add_row(vec![ing.ingredient.clone(), status, matched_by]);
            }

            println!("{table}");

            if coverage.missing_count > 0 {
                let missing: Vec<&str> = coverage
                    .ingredients
                    .iter()
                    .filter(|i| !i.matched && !i.optional)
                    .map(|i| i.ingredient.as_str())
                    .collect();
                if !missing.is_empty() {
                    println!("\nMissing: {}", missing.join(", "));
                }
            }
        }
    }

    Ok(())
}

fn cmd_grocery_from_recipe(
    paths: &FondPaths,
    slug: &str,
    include_pantry: bool,
    fmt: &OutputFormat,
) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db = open_db(paths)?;
    let grocery = fond_store::GroceryRepository::new(&db);

    let list = grocery
        .from_recipe(slug, include_pantry)
        .context("failed to generate grocery list")?;

    let Some(list) = list else {
        anyhow::bail!("recipe not found: {slug}");
    };

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&list)?);
        }
        OutputFormat::Table => {
            println!(
                "Grocery list for: {} ({})",
                list.recipe_title, list.recipe_slug
            );
            println!(
                "{} ingredient(s), {} in pantry, {} to buy\n",
                list.total_recipe_ingredients, list.pantry_covered_count, list.items_to_buy
            );

            if list.items.is_empty() {
                if list.pantry_covered_count > 0 {
                    println!("Everything is already in your pantry! 🎉");
                } else {
                    println!("No ingredients found for this recipe.");
                }
            } else {
                let mut current_category = "";

                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["", "Qty", "Unit", "Ingredient", "Note"]);

                for item in &list.items {
                    if item.category != current_category {
                        current_category = &item.category;
                        // Add a category separator row
                        table.add_row(vec![
                            format!("── {current_category} ──"),
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                        ]);
                    }

                    let status = if item.pantry_covered {
                        "\u{2713}".to_string()
                    } else if item.optional {
                        "?".to_string()
                    } else {
                        "\u{2717}".to_string()
                    };

                    let qty = item.quantity.as_deref().unwrap_or("").to_string();
                    let unit = item.unit.as_deref().unwrap_or("").to_string();
                    let note = item.note.as_deref().unwrap_or("").to_string();

                    table.add_row(vec![status, qty, unit, item.name.clone(), note]);
                }

                println!("{table}");
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Export
// ═══════════════════════════════════════════════════════════════════

/// JSON export envelope with schema version and metadata.
#[derive(Serialize)]
struct ExportEnvelope {
    schema_version: u32,
    fond_version: String,
    exported_at: String,
    recipe_count: usize,
    recipes: Vec<fond_domain::Recipe>,
}

/// Build a full Recipe from DB record + parsed .cook content.
///
/// Uses DB for authoritative metadata (timestamps, slug, tags) and
/// parses raw_source for ingredients, steps, cookware.
fn build_export_recipe(
    record: &fond_store::RecipeRecord,
    tags: &[String],
    paths: &FondPaths,
) -> Result<fond_domain::Recipe> {
    let content = {
        let dir = recipes_dir(paths);
        let file_path = dir.join(&record.file_path);
        if file_path.exists() {
            std::fs::read_to_string(&file_path).context("failed to read recipe file")?
        } else if !record.raw_source.is_empty() {
            record.raw_source.clone()
        } else {
            anyhow::bail!("recipe file not found: {}", record.file_path);
        }
    };

    let mut recipe = fond_domain::parse_cook(&content, &record.slug)
        .map_err(|e| anyhow::anyhow!("failed to parse recipe '{}': {e}", record.slug))?;

    // Override with DB-authoritative fields
    recipe.slug = record.slug.clone();
    recipe.title = record.title.clone();
    recipe.source = if record.source.is_empty() {
        None
    } else {
        Some(record.source.clone())
    };
    recipe.source_url = if record.source_url.is_empty() {
        None
    } else {
        Some(record.source_url.clone())
    };
    if let Ok(dt) = record.created_at.parse::<chrono::DateTime<chrono::Utc>>() {
        recipe.created_at = dt;
    }
    if let Ok(dt) = record.updated_at.parse::<chrono::DateTime<chrono::Utc>>() {
        recipe.updated_at = dt;
    }
    recipe.tags = tags.to_vec();

    Ok(recipe)
}

/// Collect all recipes for export.
fn collect_export_recipes(paths: &FondPaths) -> Result<Vec<fond_domain::Recipe>> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let summaries = repo.list_recipes().context("failed to list recipes")?;

    let mut recipes = Vec::with_capacity(summaries.len());

    for summary in &summaries {
        let record = repo
            .get_recipe_by_slug(&summary.slug)
            .context("database query failed")?
            .with_context(|| format!("recipe '{}' disappeared during export", summary.slug))?;

        let tags = repo
            .get_tags_for_slug(&summary.slug)
            .context("failed to get tags")?
            .map(|(_, tags)| tags)
            .unwrap_or_default();

        match build_export_recipe(&record, &tags, paths) {
            Ok(recipe) => recipes.push(recipe),
            Err(e) => {
                eprintln!("Warning: skipping '{}': {e}", summary.slug);
            }
        }
    }

    Ok(recipes)
}

/// Collect a single recipe for export.
fn collect_single_recipe(paths: &FondPaths, slug: &str) -> Result<fond_domain::Recipe> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let record = repo
        .get_recipe_by_slug(slug)
        .context("database query failed")?
        .with_context(|| format!("recipe not found: {slug}"))?;

    let tags = repo
        .get_tags_for_slug(slug)
        .context("failed to get tags")?
        .map(|(_, tags)| tags)
        .unwrap_or_default();

    build_export_recipe(&record, &tags, paths)
}

/// Convert a domain Recipe to a PaprikaRecipe.
fn recipe_to_paprika(recipe: &fond_domain::Recipe) -> paprika::PaprikaRecipe {
    // Build ingredients text (one per line)
    let ingredients = recipe
        .ingredients
        .iter()
        .map(|ing| {
            let qty = match (&ing.quantity, &ing.unit) {
                (Some(q), Some(u)) => format!("{q} {u} "),
                (Some(q), None) => format!("{q} "),
                _ => String::new(),
            };
            format!("{qty}{}", ing.name)
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Build directions text (one step per line)
    let directions = recipe
        .steps
        .iter()
        .map(|s| s.body.clone())
        .collect::<Vec<_>>()
        .join("\n\n");

    paprika::PaprikaRecipe {
        name: recipe.title.clone(),
        uid: Some(uuid::Uuid::now_v7().to_string()),
        description: recipe.description.clone(),
        ingredients: Some(ingredients),
        directions: Some(directions),
        notes: None,
        servings: recipe.servings.clone(),
        prep_time: recipe.prep_time.clone(),
        cook_time: recipe.cook_time.clone(),
        total_time: recipe.total_time.clone(),
        source: recipe.source.clone(),
        source_url: recipe.source_url.clone(),
        image_url: None,
        photo: None,
        photo_url: None,
        photo_hash: None,
        categories: if recipe.tags.is_empty() {
            None
        } else {
            Some(recipe.tags.clone())
        },
        nutrition: None,
        rating: None,
        difficulty: None,
        recipe_yield: recipe.recipe_yield.clone(),
        on_favorites: None,
        created: Some(recipe.created_at.to_rfc3339()),
        hash: None,
        scale: None,
        extra: serde_json::Map::new(),
    }
}

/// Write a single PaprikaRecipe as gzip-compressed JSON bytes.
fn paprika_recipe_to_gzip(recipe: &paprika::PaprikaRecipe) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(recipe).context("failed to serialize Paprika recipe")?;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    io::Write::write_all(&mut encoder, &json)?;
    encoder
        .finish()
        .context("failed to compress Paprika recipe")
}

fn cmd_export(
    paths: &FondPaths,
    export_format: &ExportFormat,
    recipe_slug: Option<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let recipes = if let Some(ref slug) = recipe_slug {
        vec![collect_single_recipe(paths, slug)?]
    } else {
        collect_export_recipes(paths)?
    };

    match export_format {
        ExportFormat::Json => {
            let envelope = ExportEnvelope {
                schema_version: 1,
                fond_version: env!("CARGO_PKG_VERSION").to_string(),
                exported_at: chrono::Utc::now().to_rfc3339(),
                recipe_count: recipes.len(),
                recipes,
            };

            let json =
                serde_json::to_string_pretty(&envelope).context("failed to serialize export")?;

            if let Some(ref path) = output {
                std::fs::write(path, &json)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                eprintln!(
                    "Exported {} recipe(s) to {}",
                    envelope.recipe_count,
                    path.display()
                );
            } else {
                println!("{json}");
            }
        }
        ExportFormat::Paprika => {
            let output_path = output.as_deref().unwrap_or_else(|| {
                if recipe_slug.is_some() {
                    std::path::Path::new("export.paprikarecipe")
                } else {
                    std::path::Path::new("export.paprikarecipes")
                }
            });

            if recipes.len() == 1
                && output_path
                    .extension()
                    .is_some_and(|e| e == "paprikarecipe")
            {
                // Single recipe → .paprikarecipe (gzip'd JSON, no ZIP)
                let paprika = recipe_to_paprika(&recipes[0]);
                let compressed = paprika_recipe_to_gzip(&paprika)?;
                std::fs::write(output_path, compressed)
                    .with_context(|| format!("failed to write {}", output_path.display()))?;
            } else {
                // Collection → .paprikarecipes (ZIP archive)
                let file = std::fs::File::create(output_path)
                    .with_context(|| format!("failed to create {}", output_path.display()))?;
                let mut archive = zip::ZipWriter::new(file);
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored);

                for recipe in &recipes {
                    let paprika = recipe_to_paprika(recipe);
                    let entry_name = format!("{}.paprikarecipe", recipe.slug);
                    let compressed = paprika_recipe_to_gzip(&paprika)?;

                    archive.start_file(&entry_name, options)?;
                    io::Write::write_all(&mut archive, &compressed)?;
                }

                archive.finish()?;
            }

            eprintln!(
                "Exported {} recipe(s) to {}",
                recipes.len(),
                output_path.display()
            );
        }
    }

    Ok(())
}

fn cmd_import_paprika(
    paths: &FondPaths,
    source_path: &std::path::Path,
    dry_run: bool,
    fmt: &OutputFormat,
) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    if !source_path.exists() {
        anyhow::bail!("file not found: {}", source_path.display());
    }

    // Parse the Paprika export
    let (paprika_recipes, parse_errors) =
        paprika::read_paprika_file(source_path).context("failed to read Paprika export")?;

    if paprika_recipes.is_empty() && parse_errors.is_empty() {
        match fmt {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&fond_import::ImportReport::new())?
                );
            }
            OutputFormat::Table => println!("No recipes found in the export file."),
        }
        return Ok(());
    }

    // Gather existing slugs and source URLs for duplicate detection
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);
    let existing = repo
        .list_recipes()
        .context("failed to list existing recipes")?;
    let existing_slugs: Vec<String> = existing.iter().map(|r| r.slug.clone()).collect();
    let existing_source_urls: Vec<String> = existing
        .iter()
        .filter_map(|r| {
            let record = repo.get_recipe_by_slug(&r.slug).ok()??;
            let url = record.source_url.trim().to_lowercase();
            if url.is_empty() { None } else { Some(url) }
        })
        .collect();

    // Convert and prepare recipes
    let (prepared, mut report) =
        paprika::convert_paprika_batch(paprika_recipes, &existing_slugs, &existing_source_urls);

    // Add parse errors to report
    for err in &parse_errors {
        if let Err(ref e) = err.result {
            report.add(fond_import::ImportResult::Failed {
                entry_name: err.entry_name.clone(),
                error: e.clone(),
            });
        }
    }

    if dry_run {
        // Dry-run: just report what would happen
        match fmt {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            OutputFormat::Table => {
                print_import_report(&report, true);
            }
        }
        return Ok(());
    }

    // Write .cook files and index them
    let dest_dir = recipes_dir(paths);
    for prep in &prepared {
        let dest = dest_dir.join(&prep.file_name);
        std::fs::write(&dest, &prep.cook_text)
            .with_context(|| format!("failed to write {}", dest.display()))?;

        let hash = content_hash(&prep.cook_text);
        repo.upsert_recipe(&prep.file_name, &prep.recipe, &hash)
            .with_context(|| format!("failed to index {}", prep.file_name))?;
    }

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Table => {
            print_import_report(&report, false);
        }
    }

    Ok(())
}

fn cmd_import_url(paths: &FondPaths, url: &str, dry_run: bool, fmt: &OutputFormat) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    // Validate URL scheme
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("invalid URL: only http:// and https:// are supported");
    }

    // Fetch HTML via curl subprocess (HTTP stays outside fond-import)
    let html = fetch_url(url).context("failed to fetch URL")?;

    // Gather existing data for dedup
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);
    let existing = repo
        .list_recipes()
        .context("failed to list existing recipes")?;
    let existing_slugs: Vec<String> = existing.iter().map(|r| r.slug.clone()).collect();
    let existing_source_urls: Vec<String> = existing
        .iter()
        .filter_map(|r| {
            let record = repo.get_recipe_by_slug(&r.slug).ok()??;
            let url = record.source_url.trim().to_lowercase();
            if url.is_empty() { None } else { Some(url) }
        })
        .collect();

    // Extract and convert
    let (prepared, report) =
        fond_import::schema_org::import_html(&html, url, &existing_slugs, &existing_source_urls);

    if dry_run {
        match fmt {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            OutputFormat::Table => {
                print_import_report(&report, true);
            }
        }
        return Ok(());
    }

    // Write .cook files and index them
    let dest_dir = recipes_dir(paths);
    for prep in &prepared {
        let dest = dest_dir.join(&prep.file_name);
        std::fs::write(&dest, &prep.cook_text)
            .with_context(|| format!("failed to write {}", dest.display()))?;

        let hash = content_hash(&prep.cook_text);
        repo.upsert_recipe(&prep.file_name, &prep.recipe, &hash)
            .with_context(|| format!("failed to index {}", prep.file_name))?;
    }

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Table => {
            print_import_report(&report, false);
        }
    }

    Ok(())
}

/// Fetch HTML from a URL using curl subprocess.
fn fetch_url(url: &str) -> Result<String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "30",
            "--max-redirs",
            "5",
            "-H",
            "User-Agent: fond/0.3.0 (recipe importer)",
            "-H",
            "Accept: text/html,application/xhtml+xml",
            "-w",
            "\n%{http_code}",
            url,
        ])
        .output()
        .context("failed to run curl — is curl installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("curl failed: {stderr}");
    }

    let raw = String::from_utf8(output.stdout).context("response is not valid UTF-8")?;

    // Extract HTTP status code from the last line (via -w flag)
    let (body, status_line) = raw.rsplit_once('\n').unwrap_or((&raw, ""));
    let status: u16 = status_line.trim().parse().unwrap_or(0);

    if !(200..300).contains(&status) {
        anyhow::bail!("HTTP {status} for {url}");
    }

    Ok(body.to_string())
}

fn print_import_report(report: &fond_import::ImportReport, dry_run: bool) {
    let prefix = if dry_run { "[dry-run] " } else { "" };

    if report.imported > 0 {
        println!("{prefix}Imported: {} recipe(s)", report.imported);
    }
    if report.skipped > 0 {
        println!("{prefix}Skipped:  {} recipe(s)", report.skipped);
    }
    if report.failed > 0 {
        eprintln!("{prefix}Failed:   {} recipe(s)", report.failed);
    }
    println!("{prefix}Total:    {} recipe(s)", report.total);

    // Show details for skipped/failed
    for detail in &report.details {
        match detail {
            fond_import::ImportResult::Skipped { title, reason } => {
                eprintln!("  Skipped: {title} — {reason}");
            }
            fond_import::ImportResult::Failed { entry_name, error } => {
                eprintln!("  Failed:  {entry_name} — {error}");
            }
            _ => {}
        }
    }
}

fn cmd_reindex(paths: &FondPaths, fmt: &OutputFormat) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db = open_db(paths)?;
    let dir = recipes_dir(paths);
    let report = fond_store::reindex(&db, &dir).context("reindex failed")?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Table => {
            println!("Reindexed {} recipe(s)", report.indexed);
            if !report.errors.is_empty() {
                eprintln!("\nWarnings:");
                for (file, err) in &report.errors {
                    eprintln!("  {file}: {err}");
                }
            }
        }
    }
    Ok(())
}
