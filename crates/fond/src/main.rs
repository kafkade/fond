use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use comfy_table::{ContentArrangement, Table};
use fond_store::{FondDb, FondPaths, RecipeRepository};

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
    List,

    /// Search recipes by keyword.
    Search {
        /// Search query
        query: String,
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

    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for
        shell: Shell,
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
        Commands::List => cmd_list(&paths, &fmt),
        Commands::Search { query } => cmd_search(&paths, &query, &fmt),
        Commands::Rm { slug, yes } => cmd_rm(&paths, &slug, yes, &fmt),
        Commands::Reindex => cmd_reindex(&paths, &fmt),
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

fn cmd_list(paths: &FondPaths, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let recipes = repo.list_recipes().context("failed to list recipes")?;

    if recipes.is_empty() {
        match fmt {
            OutputFormat::Json => println!("[]"),
            OutputFormat::Table => {
                println!("No recipes indexed. Add .cook files and run `fond reindex`.");
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
            table.set_header(vec!["Slug", "Title", "Source", "Tags"]);

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
                table.add_row(vec![&r.slug, &r.title, &source, &tags]);
            }

            println!("{table}");
            println!("\n{} recipe(s)", recipes.len());
        }
    }
    Ok(())
}

fn cmd_search(paths: &FondPaths, query: &str, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let results = repo.search(query).context("search failed")?;

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
            table.set_header(vec!["Slug", "Title"]);

            for r in &results {
                table.add_row(vec![&r.slug, &r.title]);
            }

            println!("{table}");
            println!("\n{} result(s)", results.len());
        }
    }
    Ok(())
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
