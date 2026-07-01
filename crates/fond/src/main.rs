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
use fond_store::{
    FondDb, FondPaths, ImportReviewRecord, ImportReviewRepository, NewImportReview,
    RecipeRepository,
};
use serde::Serialize;

mod ocr;
mod tui;

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

        /// Exclude recipes containing the current user's allergens
        #[arg(long)]
        exclude_allergens: bool,
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

        /// Exclude recipes containing the current user's allergens
        #[arg(long)]
        exclude_allergens: bool,
    },

    /// Suggest recipes you can make now, ranked by pantry coverage.
    ///
    /// Deterministic (no ML): scores recipes by how many ingredients your
    /// pantry already covers (ADR-009 presence-first), sorted by coverage %
    /// then total time. Works fully offline.
    Suggest {
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

        /// Only show recipes missing at most this many required ingredients
        /// (default: 2)
        #[arg(long)]
        max_missing: Option<usize>,

        /// Limit the number of suggestions shown
        #[arg(long)]
        limit: Option<usize>,
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

    /// Review queued import drafts before saving them as recipes.
    Review {
        #[command(subcommand)]
        action: ReviewAction,
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

    /// Plan a cooking timeline or enter interactive cook mode.
    Cook {
        /// Recipe slug (e.g., "chicken-adobo")
        slug: String,

        /// Target serve time (HH:MM format, local time)
        #[arg(long)]
        serve_at: Option<String>,

        /// Print static timeline table instead of entering TUI mode
        #[arg(long)]
        plan: bool,
    },

    /// Scale a recipe's quantities by a multiplier or target servings.
    Scale {
        /// Recipe slug (e.g., "chicken-adobo")
        slug: String,

        /// Scale multiplier (e.g., "2x", "0.5x", "3")
        #[arg(long, group = "scale_mode")]
        to: Option<String>,

        /// Target number of servings
        #[arg(long, group = "scale_mode")]
        servings: Option<u32>,
    },

    /// Add a note to a recipe, or list existing notes.
    Note {
        /// Recipe slug (e.g., "chicken-adobo")
        slug: String,

        /// Note text (omit to list existing notes)
        text: Vec<String>,

        /// Delete a note by ID
        #[arg(long)]
        delete: Option<String>,
    },

    /// Rate a recipe (1-5 stars), or show current rating.
    Rate {
        /// Recipe slug (e.g., "chicken-adobo")
        slug: String,

        /// Rating score (1-5, omit to show current rating)
        score: Option<i32>,
    },

    /// Show cooking scoreboard — most cooked, highest rated, recent activity.
    Scoreboard {
        /// Only show activity since this date (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,

        /// Number of entries per section (default: 10)
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Manage family member profiles (users, dietary prefs, allergens).
    User {
        #[command(subcommand)]
        action: UserAction,
    },

    /// Create and manage meal plans.
    Plan {
        #[command(subcommand)]
        action: PlanAction,
    },

    /// Show estimated nutrition facts for a recipe (informational).
    Nutrition {
        /// Recipe slug (e.g., "chicken-adobo")
        slug: String,
    },

    /// Launch the web UI (Axum + HTMX, server-rendered).
    ///
    /// Starts a local HTTP server for household members who prefer a browser.
    /// Designed for trusted LAN / self-host — no authentication.
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "3000", env = "FOND_PORT")]
        port: u16,

        /// Address to bind to (use 0.0.0.0 for LAN access)
        #[arg(long, default_value = "127.0.0.1", env = "FOND_BIND")]
        bind: String,
    },
}

#[derive(Subcommand)]
enum UserAction {
    /// Add a new family member profile.
    Add {
        /// Name of the user
        name: String,

        /// Allergens (comma-separated, e.g., "peanut,dairy")
        #[arg(long)]
        allergen: Option<String>,

        /// Dietary preferences (comma-separated, e.g., "vegetarian,gluten-free")
        #[arg(long)]
        diet: Option<String>,
    },

    /// List all family member profiles.
    List,

    /// Show details for a specific family member.
    Show {
        /// Name of the user
        name: String,
    },

    /// Remove a family member profile (soft-delete, preserves data).
    Rm {
        /// Name of the user to remove
        name: String,

        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },

    /// Switch the active user for commands that scope by user.
    Set {
        /// Name of the user to switch to
        name: String,
    },

    /// Update allergens or dietary preferences for a user.
    Update {
        /// Name of the user to update
        name: String,

        /// Allergens (comma-separated, replaces existing)
        #[arg(long)]
        allergen: Option<String>,

        /// Dietary preferences (comma-separated, replaces existing)
        #[arg(long)]
        diet: Option<String>,
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

    /// Generate a consolidated shopping list from a meal plan.
    ///
    /// Combines ingredients across all recipes in the plan,
    /// subtracts pantry items, and groups by aisle/category.
    FromPlan {
        /// Plan name (e.g., "week")
        plan: String,

        /// Include items already in pantry (marked as covered)
        #[arg(long)]
        include_pantry: bool,
    },
}

#[derive(Subcommand)]
enum PlanAction {
    /// Add a recipe to a meal plan.
    ///
    /// Use `day:meal=recipe-slug` format (e.g., `monday:dinner=chicken-adobo`).
    /// Day can be a weekday name (resolved to current week) or ISO date (YYYY-MM-DD).
    /// Meal must be: breakfast, lunch, dinner, or snack.
    Add {
        /// Plan name (e.g., "week")
        plan: String,

        /// Assignment in `day:meal=recipe-slug` format
        assignment: String,
    },

    /// Show a meal plan.
    Show {
        /// Plan name (e.g., "week")
        plan: String,
    },

    /// Remove a recipe from a meal plan.
    ///
    /// Use the same `day:meal=recipe-slug` format as `add`.
    Rm {
        /// Plan name (e.g., "week")
        plan: String,

        /// Assignment to remove in `day:meal=recipe-slug` format
        assignment: String,
    },

    /// List all meal plans.
    List,

    /// Clear all entries from a meal plan (keep the plan itself).
    Clear {
        /// Plan name (e.g., "week")
        plan: String,

        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },

    /// Delete a meal plan and all its entries.
    Delete {
        /// Plan name (e.g., "week")
        plan: String,

        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
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

    /// Import a recipe from a local photo or scanned image.
    Photo {
        /// Path to the image file
        path: PathBuf,

        /// Preview what would be queued without writing review data
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum ReviewAction {
    /// List pending review drafts.
    List,

    /// Show a queued draft in detail.
    Show {
        /// Review item ID
        id: String,
    },

    /// Edit a queued Cooklang draft in your editor.
    Edit {
        /// Review item ID
        id: String,
    },

    /// Accept a queued draft and write it as a recipe.
    Accept {
        /// Review item ID
        id: String,
    },

    /// Reject a queued draft.
    Reject {
        /// Review item ID
        id: String,

        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
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
            exclude_allergens,
        } => cmd_list(
            &paths,
            &fmt,
            tag,
            max_time,
            cuisine,
            source,
            exclude_allergens,
        ),
        Commands::Search {
            query,
            tag,
            max_time,
            cuisine,
            source,
            exclude_allergens,
        } => cmd_search(
            &paths,
            &query,
            &fmt,
            tag,
            max_time,
            cuisine,
            source,
            exclude_allergens,
        ),
        Commands::Tag {
            slug,
            add,
            remove,
            list,
        } => cmd_tag(&paths, slug, add, remove, list, &fmt),
        Commands::Suggest {
            tag,
            max_time,
            cuisine,
            source,
            max_missing,
            limit,
        } => cmd_suggest(
            &paths,
            &fmt,
            tag,
            max_time,
            cuisine,
            source,
            max_missing,
            limit,
        ),
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
            GroceryAction::FromPlan {
                plan,
                include_pantry,
            } => cmd_grocery_from_plan(&paths, &plan, include_pantry, &fmt),
        },
        Commands::Import { source } => match source {
            ImportSource::Paprika { path, dry_run } => {
                cmd_import_paprika(&paths, &path, dry_run, &fmt)
            }
            ImportSource::Url { url, dry_run } => cmd_import_url(&paths, &url, dry_run, &fmt),
            ImportSource::Photo { path, dry_run } => cmd_import_photo(&paths, &path, dry_run, &fmt),
        },
        Commands::Review { action } => match action {
            ReviewAction::List => cmd_review_list(&paths, &fmt),
            ReviewAction::Show { id } => cmd_review_show(&paths, &id, &fmt),
            ReviewAction::Edit { id } => cmd_review_edit(&paths, &id),
            ReviewAction::Accept { id } => cmd_review_accept(&paths, &id, &fmt),
            ReviewAction::Reject { id, yes } => cmd_review_reject(&paths, &id, yes, &fmt),
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
        Commands::Cook {
            slug,
            serve_at,
            plan,
        } => cmd_cook(&paths, &slug, serve_at.as_deref(), plan, &fmt),
        Commands::Scale { slug, to, servings } => {
            cmd_scale(&paths, &slug, to.as_deref(), servings, &fmt)
        }
        Commands::Note { slug, text, delete } => cmd_note(&paths, &slug, &text, delete, &fmt),
        Commands::Rate { slug, score } => cmd_rate(&paths, &slug, score, &fmt),
        Commands::Scoreboard { since, limit } => {
            cmd_scoreboard(&paths, since.as_deref(), limit, &fmt)
        }
        Commands::User { action } => match action {
            UserAction::Add {
                name,
                allergen,
                diet,
            } => cmd_user_add(&paths, &name, allergen.as_deref(), diet.as_deref(), &fmt),
            UserAction::List => cmd_user_list(&paths, &fmt),
            UserAction::Show { name } => cmd_user_show(&paths, &name, &fmt),
            UserAction::Rm { name, yes } => cmd_user_rm(&paths, &name, yes, &fmt),
            UserAction::Set { name } => cmd_user_set(&paths, &name, &fmt),
            UserAction::Update {
                name,
                allergen,
                diet,
            } => cmd_user_update(&paths, &name, allergen.as_deref(), diet.as_deref(), &fmt),
        },
        Commands::Plan { action } => match action {
            PlanAction::Add { plan, assignment } => cmd_plan_add(&paths, &plan, &assignment, &fmt),
            PlanAction::Show { plan } => cmd_plan_show(&paths, &plan, &fmt),
            PlanAction::Rm { plan, assignment } => cmd_plan_rm(&paths, &plan, &assignment, &fmt),
            PlanAction::List => cmd_plan_list(&paths, &fmt),
            PlanAction::Clear { plan, yes } => cmd_plan_clear(&paths, &plan, yes, &fmt),
            PlanAction::Delete { plan, yes } => cmd_plan_delete(&paths, &plan, yes, &fmt),
        },
        Commands::Nutrition { slug } => cmd_nutrition(&paths, &slug, &fmt),
        Commands::Serve { port, bind } => cmd_serve(&paths, port, &bind),
    }
}
// ═══════════════════════════════════════════════════════════════════

fn open_db(paths: &FondPaths) -> Result<FondDb> {
    let db_path = paths.data_dir.join("fond.db");
    FondDb::open(&db_path).context("failed to open database")
}

fn recipes_dir(paths: &FondPaths) -> PathBuf {
    paths.data_dir.join("recipes")
}

fn review_assets_dir(paths: &FondPaths) -> PathBuf {
    paths.data_dir.join("photos").join("review")
}

fn hash_value<T: Hash + ?Sized>(value: &T) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn content_hash(content: &str) -> String {
    hash_value(content)
}

fn bytes_hash(bytes: &[u8]) -> String {
    hash_value(bytes)
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
    println!("  photos/   — review/import image assets");
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
            // Include allergen flags in JSON output
            let user_repo = fond_store::UserRepository::new(&db);
            user_repo.seed_ingredient_allergens().ok();
            let flags = user_repo
                .check_recipe_allergens(record.id)
                .unwrap_or_default();

            #[derive(Serialize)]
            struct ViewOutput {
                #[serde(flatten)]
                recipe: fond_domain::Recipe,
                #[serde(skip_serializing_if = "Vec::is_empty")]
                allergen_flags: Vec<fond_store::AllergenFlag>,
                #[serde(skip_serializing_if = "Option::is_none")]
                allergen_disclaimer: Option<String>,
            }
            let has_flags = !flags.is_empty();
            let out = ViewOutput {
                allergen_flags: flags,
                allergen_disclaimer: if has_flags {
                    Some(fond_domain::ALLERGEN_DISCLAIMER.to_string())
                } else {
                    None
                },
                recipe,
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            print_recipe_human(&recipe);

            // Show allergen warnings
            let user_repo = fond_store::UserRepository::new(&db);
            user_repo.seed_ingredient_allergens().ok();

            if let Some(user_id) = default_user_id(&db) {
                let flags = user_repo
                    .check_recipe_allergens_for_user(record.id, user_id)
                    .unwrap_or_default();
                if !flags.is_empty() {
                    println!("\n⚠ Allergen warnings:");
                    for flag in &flags {
                        println!("  • {} → {}", flag.ingredient, flag.allergen);
                    }
                    println!("\n{}", fond_domain::ALLERGEN_DISCLAIMER);
                }
            }
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
    exclude_allergens: bool,
) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let filter = build_cli_filter(tags, max_time, cuisine, source);
    let mut recipes = repo
        .list_recipes_filtered(&filter)
        .context("failed to list recipes")?;

    // Filter out recipes containing the current user's allergens
    if exclude_allergens {
        let user_repo = fond_store::UserRepository::new(&db);
        user_repo
            .seed_ingredient_allergens()
            .context("failed to seed allergen data")?;

        if let Some(user_id) = default_user_id(&db) {
            let flagged = user_repo
                .filter_recipes_excluding_allergens(user_id)
                .context("failed to check allergens")?;
            let before = recipes.len();
            recipes.retain(|r| !flagged.contains(&r.id));
            let excluded = before - recipes.len();

            if excluded > 0
                && let OutputFormat::Table = fmt
            {
                eprintln!(
                    "ℹ Excluded {excluded} recipe(s) containing your allergens. \
                     {}\n",
                    fond_domain::ALLERGEN_DISCLAIMER
                );
            }
        }
    }

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

#[allow(clippy::too_many_arguments)]
fn cmd_search(
    paths: &FondPaths,
    query: &str,
    fmt: &OutputFormat,
    tags: Vec<String>,
    max_time: Option<u32>,
    cuisine: Option<String>,
    source: Option<String>,
    exclude_allergens: bool,
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

    let mut results = repo
        .search_filtered(&escaped_query, &filter)
        .context("search failed")?;

    // Filter out recipes containing the current user's allergens
    if exclude_allergens {
        let user_repo = fond_store::UserRepository::new(&db);
        user_repo
            .seed_ingredient_allergens()
            .context("failed to seed allergen data")?;

        if let Some(user_id) = default_user_id(&db) {
            let flagged = user_repo
                .filter_recipes_excluding_allergens(user_id)
                .context("failed to check allergens")?;
            let before = results.len();
            results.retain(|r| !flagged.contains(&r.recipe_id));
            let excluded = before - results.len();

            if excluded > 0
                && let OutputFormat::Table = fmt
            {
                eprintln!(
                    "ℹ Excluded {excluded} result(s) containing your allergens. \
                     {}\n",
                    fond_domain::ALLERGEN_DISCLAIMER
                );
            }
        }
    }

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

/// Default max required-missing ingredients for `fond suggest` when unset.
const DEFAULT_SUGGEST_MAX_MISSING: usize = 2;

#[allow(clippy::too_many_arguments)]
fn cmd_suggest(
    paths: &FondPaths,
    fmt: &OutputFormat,
    tags: Vec<String>,
    max_time: Option<u32>,
    cuisine: Option<String>,
    source: Option<String>,
    max_missing: Option<usize>,
    limit: Option<usize>,
) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);
    let pantry = fond_store::PantryRepository::new(&db);

    // Guide the user if the pantry is empty — coverage ranking needs items.
    let present = pantry.list_items(false).context("failed to read pantry")?;
    if present.is_empty() {
        match fmt {
            OutputFormat::Json => println!("[]"),
            OutputFormat::Table => {
                println!(
                    "Your pantry is empty. Add items with `fond pantry add <item>...` \
                     then run `fond suggest` again."
                );
            }
        }
        return Ok(());
    }

    let filter = build_cli_filter(tags, max_time, cuisine, source);
    let candidates = repo
        .list_recipes_filtered(&filter)
        .context("failed to list recipes")?;

    let cap = max_missing.unwrap_or(DEFAULT_SUGGEST_MAX_MISSING);
    let mut suggestions = pantry
        .suggest(&candidates, Some(cap))
        .context("failed to rank recipes by pantry coverage")?;

    if let Some(n) = limit {
        suggestions.truncate(n);
    }

    if suggestions.is_empty() {
        match fmt {
            OutputFormat::Json => println!("[]"),
            OutputFormat::Table => {
                if candidates.is_empty() && !filter.is_empty() {
                    println!("No recipes match the given filters.");
                } else if candidates.is_empty() {
                    println!("No recipes indexed. Add .cook files and run `fond reindex`.");
                } else {
                    println!(
                        "No recipes are within {cap} missing ingredient(s). \
                         Try raising --max-missing or stocking your pantry."
                    );
                }
            }
        }
        return Ok(());
    }

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&suggestions)?);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["Coverage", "Slug", "Title", "Time", "Missing"]);

            for s in &suggestions {
                let coverage = format!(
                    "{:.0}% ({}/{})",
                    s.coverage_pct, s.matched_count, s.total_ingredients
                );
                let time = if s.total_time.is_empty() {
                    "\u{2014}".to_string()
                } else {
                    s.total_time.clone()
                };
                let missing = if s.missing.is_empty() {
                    "\u{2713} make now".to_string()
                } else {
                    s.missing.join(", ")
                };
                table.add_row(vec![
                    coverage,
                    s.slug.clone(),
                    s.title.clone(),
                    time,
                    missing,
                ]);
            }

            println!("{table}");
            println!("\n{} suggestion(s)", suggestions.len());
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

// ═══════════════════════════════════════════════════════════════════
// Cook (timeline)
// ═══════════════════════════════════════════════════════════════════

fn cmd_cook(
    paths: &FondPaths,
    slug: &str,
    serve_at_str: Option<&str>,
    plan: bool,
    fmt: &OutputFormat,
) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let record = repo
        .get_recipe_by_slug(slug)
        .context("database query failed")?
        .with_context(|| {
            format!("no recipe found with slug '{slug}' — run `fond list` to see available recipes")
        })?;

    // Parse the .cook file for full recipe with steps/timers
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

    // Build timeline DAG
    let timeline = fond_timeline::build_timeline(&recipe);

    if timeline.nodes.is_empty() {
        eprintln!("No steps found in recipe '{slug}'.");
        return Ok(());
    }

    // Parse optional serve-at time
    let scheduled = if let Some(sat) = serve_at_str {
        let serve_time = chrono::NaiveTime::parse_from_str(sat, "%H:%M")
            .or_else(|_| chrono::NaiveTime::parse_from_str(sat, "%H:%M:%S"))
            .context("Invalid time format — use HH:MM (e.g., 19:00)")?;

        let today = chrono::Local::now().date_naive();
        let mut serve_at = today.and_time(serve_time);
        if serve_at < chrono::Local::now().naive_local() {
            serve_at = (today + chrono::Duration::days(1)).and_time(serve_time);
        }
        Some(fond_timeline::schedule_backward(&timeline, serve_at))
    } else {
        None
    };

    // Decide output mode: --plan or --format json → static; otherwise TUI
    let use_static = plan || matches!(fmt, OutputFormat::Json) || !atty::is(atty::Stream::Stdout);

    if use_static {
        if let Some(ref sched) = scheduled {
            match fmt {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(sched)?);
                }
                OutputFormat::Table => {
                    print_timeline(sched);
                }
            }
        } else {
            anyhow::bail!("--serve-at is required for --plan or --format json output");
        }
        return Ok(());
    }

    // TUI mode
    let cook_result = tui::run_cook_mode(recipe, scheduled)?;

    // Post-TUI: show summary and offer to save cook log
    println!();
    println!("  Cook session complete: {}", cook_result.recipe_title);
    let dur_mins = cook_result.cook_duration.as_secs() / 60;
    let dur_secs = cook_result.cook_duration.as_secs() % 60;
    println!(
        "  Steps: {}/{} | Duration: {}:{:02}",
        cook_result.steps_completed, cook_result.total_steps, dur_mins, dur_secs,
    );

    // Save cook log
    print!("  Save to cook log? [Y/n] ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let save = input.trim().is_empty() || input.trim().eq_ignore_ascii_case("y");

    if save {
        // Prompt for an optional note before saving
        print!("  Add a note? (enter text, or press Enter to skip): ");
        io::stdout().flush()?;
        let mut note_input = String::new();
        io::stdin().lock().read_line(&mut note_input)?;
        let note_text = note_input.trim().to_string();

        let user_id = default_user_id(&db);
        let now = chrono::Local::now();
        let started = now - chrono::Duration::seconds(cook_result.cook_duration.as_secs() as i64);
        let entry = fond_store::NewCookLog {
            recipe_slug: record.slug.clone(),
            user_id,
            started_at: started.format("%Y-%m-%dT%H:%M:%S").to_string(),
            finished_at: now.format("%Y-%m-%dT%H:%M:%S").to_string(),
            steps_completed: cook_result.steps_completed as i32,
            total_steps: cook_result.total_steps as i32,
            notes: note_text,
        };
        let log_repo = fond_store::CookLogRepository::new(&db);
        log_repo.save(&entry).context("failed to save cook log")?;
        println!("  Cook log saved.");
    }
    println!();

    Ok(())
}

fn print_timeline(sched: &fond_timeline::ScheduledTimeline) {
    use fond_timeline::duration::format_duration;

    println!();
    println!("  Cooking Timeline: {}", sched.recipe_title);
    println!(
        "  Serve at {} — Start at {}",
        sched.serve_at.format("%H:%M"),
        sched.start_at.format("%H:%M"),
    );
    println!();

    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Start", "Duration", "Type", "Step"]);

    for sn in &sched.nodes {
        let start = sn.scheduled_start.format("%H:%M").to_string();
        let dur = match &sn.node.duration {
            Some(d) => format_duration(d.seconds),
            None => "—".to_string(),
        };
        let task_type = sn.node.task_type.label().to_string();
        let label = sn.node.label.clone();
        table.add_row(vec![start, dur, task_type, label]);
    }

    println!("{table}");
    println!();

    // Summary
    let active_str = format_duration(sched.total_active_seconds);
    let passive_str = format_duration(sched.total_passive_seconds);
    let total = sched.total_active_seconds + sched.total_passive_seconds;
    let total_str = format_duration(total);

    println!(
        "  Active: {} | Passive: {} | Timed total: {}",
        active_str, passive_str, total_str
    );

    if sched.has_untimed_steps {
        println!("  Note: Some steps have unknown duration (shown as —).");
    }
    println!();
}

// ═══════════════════════════════════════════════════════════════════
// Scale
// ═══════════════════════════════════════════════════════════════════

fn cmd_scale(
    paths: &FondPaths,
    slug: &str,
    to: Option<&str>,
    servings: Option<u32>,
    fmt: &OutputFormat,
) -> Result<()> {
    use fond_core::scale::{ScaleError, ScaleFactor, parse_scale_arg, scale_recipe};

    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);

    let record = repo
        .get_recipe_by_slug(slug)
        .context("database query failed")?
        .with_context(|| {
            format!("no recipe found with slug '{slug}' — run `fond list` to see available recipes")
        })?;

    // Parse from file for full fidelity (same pattern as cmd_view)
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

    // Determine scale factor
    let factor = match (to, servings) {
        (Some(to_str), None) => {
            let multiplier = parse_scale_arg(to_str).with_context(|| {
                format!("invalid scale factor '{to_str}' — use a number like '2x' or '0.5'")
            })?;
            ScaleFactor::Multiplier(multiplier)
        }
        (None, Some(s)) => ScaleFactor::ToServings(s),
        (None, None) => {
            anyhow::bail!("specify either --to <multiplier> or --servings <count>");
        }
        (Some(_), Some(_)) => {
            // clap's group should prevent this, but handle gracefully
            anyhow::bail!("cannot use both --to and --servings at the same time");
        }
    };

    let scaled = scale_recipe(&recipe, factor).map_err(|e| match e {
        ScaleError::NoServingsMetadata => anyhow::anyhow!("{e}"),
        ScaleError::UnparseableServings(_) => anyhow::anyhow!("{e}"),
        ScaleError::InvalidFactor(_) => anyhow::anyhow!("{e}"),
        ScaleError::InvalidServings(_) => anyhow::anyhow!("{e}"),
    })?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&scaled)?);
        }
        OutputFormat::Table => {
            print_scaled_recipe(&scaled);
        }
    }

    Ok(())
}

fn print_scaled_recipe(scaled: &fond_core::scale::ScaledRecipe) {
    println!(
        "# {} (×{})",
        scaled.title,
        format_scale(scaled.scale_factor)
    );

    if let Some(ref orig) = scaled.original_servings {
        if let Some(ref target) = scaled.scaled_servings {
            println!("Servings: {orig} → {target}");
        } else {
            println!("Servings: {orig}");
        }
    }

    let mut timing = Vec::new();
    if let Some(ref t) = scaled.prep_time {
        timing.push(format!("Prep: {t}"));
    }
    if let Some(ref t) = scaled.cook_time {
        timing.push(format!("Cook: {t}"));
    }
    if let Some(ref t) = scaled.total_time {
        timing.push(format!("Total: {t}"));
    }
    if !timing.is_empty() {
        println!("{} (times unchanged)", timing.join("  "));
    }

    println!("\n## Scaled Ingredients\n");
    for ing in &scaled.ingredients {
        let qty = match (&ing.scaled_quantity, &ing.unit) {
            (Some(q), Some(u)) => format!("{q} {u} "),
            (Some(q), None) => format!("{q} "),
            _ => String::new(),
        };
        let marker = if ing.warning.is_some() { " ⚠" } else { "" };
        println!("  - {qty}{}{marker}", ing.name);
    }

    if !scaled.warnings.is_empty() {
        println!("\n## Scaling Warnings\n");
        for w in &scaled.warnings {
            println!("  ⚠ {}: {}", w.ingredient, w.message);
        }
    }

    println!();
}

fn format_scale(factor: f64) -> String {
    if (factor - factor.round()).abs() < 0.01 {
        format!("{}", factor.round() as u32)
    } else {
        format!("{factor:.1}")
    }
}

// ═══════════════════════════════════════════════════════════════════
// Notes, Ratings, Scoreboard
// ═══════════════════════════════════════════════════════════════════

/// Get the current user ID (from app_settings, falls back to 'default' user).
fn default_user_id(db: &FondDb) -> Option<i64> {
    let user_repo = fond_store::UserRepository::new(db);
    if let Ok(Some(id)) = user_repo.get_current_user_id() {
        return Some(id);
    }
    // Fallback: query for the 'default' user from V005
    db.conn()
        .query_row("SELECT id FROM users WHERE name = 'default'", [], |row| {
            row.get(0)
        })
        .ok()
}

fn cmd_note(
    paths: &FondPaths,
    slug: &str,
    text: &[String],
    delete: Option<String>,
    fmt: &OutputFormat,
) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);
    let note_repo = fond_store::NoteRepository::new(&db);

    let record = repo
        .get_recipe_by_slug(slug)
        .context("database query failed")?
        .with_context(|| {
            format!("no recipe found with slug '{slug}' — run `fond list` to see available recipes")
        })?;

    let user_id = default_user_id(&db);

    // Delete mode
    if let Some(note_id) = delete {
        let deleted = note_repo
            .delete(&note_id, user_id)
            .context("failed to delete note")?;
        if deleted {
            match fmt {
                OutputFormat::Json => println!(r#"{{"deleted": true, "id": "{note_id}"}}"#),
                OutputFormat::Table => println!("Note {note_id} deleted."),
            }
        } else {
            anyhow::bail!("note {note_id} not found or not owned by you");
        }
        return Ok(());
    }

    // Add mode (text provided)
    if !text.is_empty() {
        let body = text.join(" ");
        let id = note_repo
            .add(&record.slug, user_id, &body)
            .context("failed to add note")?;

        match fmt {
            OutputFormat::Json => {
                #[derive(Serialize)]
                struct Added {
                    id: String,
                    recipe: String,
                    note: String,
                }
                let added = Added {
                    id,
                    recipe: slug.to_string(),
                    note: body,
                };
                println!("{}", serde_json::to_string_pretty(&added)?);
            }
            OutputFormat::Table => {
                println!("Note added to '{}' ({id}).", record.title);
            }
        }
        return Ok(());
    }

    // List mode (no text, no delete)
    let notes = note_repo
        .list_for_recipe(&record.slug, user_id)
        .context("failed to list notes")?;

    match fmt {
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct NoteOut {
                id: String,
                body: String,
                created_at: String,
            }
            let out: Vec<NoteOut> = notes
                .into_iter()
                .map(|n| NoteOut {
                    id: n.id,
                    body: n.body,
                    created_at: n.created_at,
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            if notes.is_empty() {
                println!("No notes for '{}'.", record.title);
            } else {
                println!("Notes for '{}':\n", record.title);
                for n in &notes {
                    println!("  {} ({})", n.id, n.created_at);
                    println!("    {}\n", n.body);
                }
            }
        }
    }

    Ok(())
}

fn cmd_rate(paths: &FondPaths, slug: &str, score: Option<i32>, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let repo = RecipeRepository::new(&db);
    let rating_repo = fond_store::RatingRepository::new(&db);

    let record = repo
        .get_recipe_by_slug(slug)
        .context("database query failed")?
        .with_context(|| {
            format!("no recipe found with slug '{slug}' — run `fond list` to see available recipes")
        })?;

    let user_id = default_user_id(&db);

    // Rate mode
    if let Some(s) = score {
        if !(1..=5).contains(&s) {
            anyhow::bail!("rating must be between 1 and 5 (got {s})");
        }
        rating_repo
            .rate(&record.slug, user_id, s)
            .context("failed to save rating")?;

        let stars = "★".repeat(s as usize) + &"☆".repeat(5 - s as usize);
        match fmt {
            OutputFormat::Json => {
                #[derive(Serialize)]
                struct Rated {
                    recipe: String,
                    score: i32,
                }
                let rated = Rated {
                    recipe: slug.to_string(),
                    score: s,
                };
                println!("{}", serde_json::to_string_pretty(&rated)?);
            }
            OutputFormat::Table => {
                println!("Rated '{}': {stars}", record.title);
            }
        }
        return Ok(());
    }

    // Show mode (no score)
    let rating = rating_repo
        .get_for_recipe(&record.slug, user_id)
        .context("failed to get rating")?;
    let avg = rating_repo
        .average_for_recipe(&record.slug)
        .context("failed to compute average")?;

    match fmt {
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct RatingOut {
                recipe: String,
                your_score: Option<i32>,
                average: Option<f64>,
            }
            let out = RatingOut {
                recipe: slug.to_string(),
                your_score: rating.as_ref().map(|r| r.score),
                average: avg,
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            if let Some(r) = rating {
                let stars = "★".repeat(r.score as usize) + &"☆".repeat(5 - r.score as usize);
                println!("Your rating for '{}': {stars}", record.title);
                if let Some(a) = avg {
                    println!("Average rating: {a:.1}/5");
                }
            } else {
                println!(
                    "No rating for '{}'. Use `fond rate {} <1-5>` to rate.",
                    record.title, slug
                );
            }
        }
    }

    Ok(())
}

fn cmd_scoreboard(
    paths: &FondPaths,
    since: Option<&str>,
    limit: usize,
    fmt: &OutputFormat,
) -> Result<()> {
    let db = open_db(paths)?;
    let sb_repo = fond_store::ScoreboardRepository::new(&db);

    let scoreboard = sb_repo
        .scoreboard(limit, since)
        .context("failed to build scoreboard")?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&scoreboard)?);
        }
        OutputFormat::Table => {
            print_scoreboard(&scoreboard, since);
        }
    }

    Ok(())
}

fn print_scoreboard(sb: &fond_store::Scoreboard, since: Option<&str>) {
    let period = since.map(|s| format!(" (since {s})")).unwrap_or_default();

    // Most Cooked
    println!("🍳 Most Cooked{period}\n");
    if sb.most_cooked.is_empty() {
        println!("  No cook logs yet.\n");
    } else {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["#", "Recipe", "Times Cooked"]);
        for (i, entry) in sb.most_cooked.iter().enumerate() {
            table.add_row(vec![
                (i + 1).to_string(),
                entry.title.clone(),
                entry.cook_count.to_string(),
            ]);
        }
        println!("{table}\n");
    }

    // Highest Rated
    println!("⭐ Highest Rated{period}\n");
    if sb.highest_rated.is_empty() {
        println!("  No ratings yet.\n");
    } else {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["#", "Recipe", "Rating", "Votes"]);
        for (i, entry) in sb.highest_rated.iter().enumerate() {
            let stars = format!("{:.1}/5", entry.avg_score);
            table.add_row(vec![
                (i + 1).to_string(),
                entry.title.clone(),
                stars,
                entry.rating_count.to_string(),
            ]);
        }
        println!("{table}\n");
    }

    // Recent Activity
    println!("📋 Recent Activity{period}\n");
    if sb.recent_activity.is_empty() {
        println!("  No activity yet.\n");
    } else {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["When", "Recipe", "Action", "Detail"]);
        for entry in &sb.recent_activity {
            let when = entry.timestamp.get(..16).unwrap_or(&entry.timestamp);
            table.add_row(vec![
                when.to_string(),
                entry.title.clone(),
                entry.activity_type.clone(),
                entry.detail.clone(),
            ]);
        }
        println!("{table}\n");
    }
}

// ═══════════════════════════════════════════════════════════════════
// User Profiles
// ═══════════════════════════════════════════════════════════════════

/// Parse comma-separated values into a Vec of trimmed lowercase strings.
fn parse_csv_values(input: Option<&str>) -> Vec<String> {
    input
        .map(|s| {
            s.split(',')
                .map(|v| v.trim().to_lowercase())
                .filter(|v| !v.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn cmd_user_add(
    paths: &FondPaths,
    name: &str,
    allergen: Option<&str>,
    diet: Option<&str>,
    fmt: &OutputFormat,
) -> Result<()> {
    let db = open_db(paths)?;
    let user_repo = fond_store::UserRepository::new(&db);

    let allergens: Vec<String> = parse_csv_values(allergen)
        .iter()
        .map(|s| fond_domain::Allergen::parse(s).as_str().to_string())
        .collect();
    let dietary_prefs: Vec<String> = parse_csv_values(diet)
        .iter()
        .map(|s| fond_domain::DietaryPref::parse(s).as_str().to_string())
        .collect();

    let id = user_repo
        .add(name, &allergens, &dietary_prefs)
        .context("failed to create user")?;

    // Seed allergen reference data on first user creation
    user_repo
        .seed_ingredient_allergens()
        .context("failed to seed allergen data")?;

    match fmt {
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct UserAdded {
                id: i64,
                name: String,
                allergens: Vec<String>,
                dietary_prefs: Vec<String>,
            }
            let out = UserAdded {
                id,
                name: name.to_string(),
                allergens,
                dietary_prefs,
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            println!("Added user '{name}' (id: {id}).");
            if !allergens.is_empty() {
                println!("  Allergens: {}", allergens.join(", "));
            }
            if !dietary_prefs.is_empty() {
                println!("  Dietary preferences: {}", dietary_prefs.join(", "));
            }
        }
    }

    Ok(())
}

fn cmd_user_list(paths: &FondPaths, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let user_repo = fond_store::UserRepository::new(&db);

    let users = user_repo.list().context("failed to list users")?;
    let current_id = user_repo.get_current_user_id().unwrap_or(None);

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&users)?);
        }
        OutputFormat::Table => {
            if users.is_empty() {
                println!("No users. Add one with `fond user add <name>`.");
                return Ok(());
            }

            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["", "Name", "Allergens", "Dietary Prefs"]);

            for u in &users {
                let marker = if Some(u.id) == current_id { "→" } else { " " };
                let allergens = if u.allergens.is_empty() {
                    "—".to_string()
                } else {
                    u.allergens.join(", ")
                };
                let prefs = if u.dietary_prefs.is_empty() {
                    "—".to_string()
                } else {
                    u.dietary_prefs.join(", ")
                };
                table.add_row(vec![marker, &u.name, &allergens, &prefs]);
            }

            println!("{table}");
            println!("\n{} user(s)", users.len());
        }
    }

    Ok(())
}

fn cmd_user_show(paths: &FondPaths, name: &str, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let user_repo = fond_store::UserRepository::new(&db);

    let user = user_repo
        .get_by_name(name)
        .context("database query failed")?
        .with_context(|| format!("no user found with name '{name}'"))?;

    let current_id = user_repo.get_current_user_id().unwrap_or(None);
    let is_current = Some(user.id) == current_id;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&user)?);
        }
        OutputFormat::Table => {
            println!(
                "👤 {}{}",
                user.name,
                if is_current { " (active)" } else { "" }
            );
            println!("  Created: {}", user.created_at);
            if user.allergens.is_empty() {
                println!("  Allergens: (none)");
            } else {
                println!("  Allergens: {}", user.allergens.join(", "));
            }
            if user.dietary_prefs.is_empty() {
                println!("  Dietary preferences: (none)");
            } else {
                println!("  Dietary preferences: {}", user.dietary_prefs.join(", "));
            }
        }
    }

    Ok(())
}

fn cmd_user_rm(paths: &FondPaths, name: &str, yes: bool, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let user_repo = fond_store::UserRepository::new(&db);

    let user = user_repo
        .get_by_name(name)
        .context("database query failed")?
        .with_context(|| format!("no user found with name '{name}'"))?;

    if !yes
        && !confirm(&format!(
            "Remove user '{name}'? (Notes, ratings, and cook logs will be preserved.)"
        ))
    {
        println!("Cancelled.");
        return Ok(());
    }

    let current_id = user_repo.get_current_user_id().unwrap_or(None);
    let was_current = Some(user.id) == current_id;

    user_repo
        .deactivate(user.id)
        .context("failed to deactivate user")?;

    // If we removed the current user, reset to default
    if was_current {
        user_repo.set_current_user(1).ok();
    }

    match fmt {
        OutputFormat::Json => {
            println!(r#"{{"removed": true, "name": "{}"}}"#, name);
        }
        OutputFormat::Table => {
            println!("Removed user '{name}'. Notes, ratings, and cook logs are preserved.");
        }
    }

    Ok(())
}

fn cmd_user_set(paths: &FondPaths, name: &str, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let user_repo = fond_store::UserRepository::new(&db);

    let user = user_repo
        .get_by_name(name)
        .context("database query failed")?
        .with_context(|| {
            format!("no user found with name '{name}' — add them with `fond user add {name}`")
        })?;

    user_repo
        .set_current_user(user.id)
        .context("failed to set current user")?;

    match fmt {
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct SetUser {
                current_user: String,
                id: i64,
            }
            let out = SetUser {
                current_user: user.name.clone(),
                id: user.id,
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            println!("Switched active user to '{}'.", user.name);
        }
    }

    Ok(())
}

fn cmd_user_update(
    paths: &FondPaths,
    name: &str,
    allergen: Option<&str>,
    diet: Option<&str>,
    fmt: &OutputFormat,
) -> Result<()> {
    if allergen.is_none() && diet.is_none() {
        anyhow::bail!("specify --allergen and/or --diet to update");
    }

    let db = open_db(paths)?;
    let user_repo = fond_store::UserRepository::new(&db);

    let user = user_repo
        .get_by_name(name)
        .context("database query failed")?
        .with_context(|| format!("no user found with name '{name}'"))?;

    if let Some(a) = allergen {
        let allergens: Vec<String> = parse_csv_values(Some(a))
            .iter()
            .map(|s| fond_domain::Allergen::parse(s).as_str().to_string())
            .collect();
        user_repo
            .set_allergens(user.id, &allergens)
            .context("failed to update allergens")?;
    }

    if let Some(d) = diet {
        let prefs: Vec<String> = parse_csv_values(Some(d))
            .iter()
            .map(|s| fond_domain::DietaryPref::parse(s).as_str().to_string())
            .collect();
        user_repo
            .set_dietary_prefs(user.id, &prefs)
            .context("failed to update dietary preferences")?;
    }

    // Show updated profile
    let updated = user_repo.get_by_id(user.id).unwrap_or(None);

    match fmt {
        OutputFormat::Json => {
            if let Some(u) = updated {
                println!("{}", serde_json::to_string_pretty(&u)?);
            }
        }
        OutputFormat::Table => {
            println!("Updated user '{name}'.");
            if let Some(u) = updated {
                if !u.allergens.is_empty() {
                    println!("  Allergens: {}", u.allergens.join(", "));
                }
                if !u.dietary_prefs.is_empty() {
                    println!("  Dietary preferences: {}", u.dietary_prefs.join(", "));
                }
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Meal Planning
// ═══════════════════════════════════════════════════════════════════

/// Parse an assignment string like "monday:dinner=chicken-adobo".
///
/// Returns (day_or_date, meal, recipe_slug).
fn parse_plan_assignment(s: &str) -> Result<(String, String, String)> {
    // Split on '='
    let (day_meal, recipe_slug) = s.split_once('=').with_context(|| {
        format!(
            "invalid assignment format '{s}' — expected day:meal=recipe-slug \
                 (e.g., monday:dinner=chicken-adobo)"
        )
    })?;

    let recipe_slug = recipe_slug.trim().to_string();
    if recipe_slug.is_empty() {
        anyhow::bail!("recipe slug cannot be empty");
    }

    // Split on ':'
    let (day, meal) = day_meal.split_once(':').with_context(|| {
        format!(
            "invalid assignment format '{s}' — expected day:meal=recipe-slug \
                 (e.g., monday:dinner=chicken-adobo)"
        )
    })?;

    let day = day.trim().to_string();
    let meal = meal.trim().to_lowercase();

    if day.is_empty() || meal.is_empty() {
        anyhow::bail!("invalid assignment format '{s}' — day and meal cannot be empty");
    }

    Ok((day, meal, recipe_slug))
}

/// Resolve a day string to an ISO date.
///
/// Accepts either a weekday name ("monday") resolved to the current week,
/// or an ISO date ("2025-06-02") passed through.
fn resolve_day_to_date(day: &str) -> Result<String> {
    if fond_store::is_weekday(day) {
        fond_store::weekday_to_date(day).context("failed to resolve weekday to date")
    } else if day.len() == 10 && day.chars().nth(4) == Some('-') {
        // Looks like an ISO date
        Ok(day.to_string())
    } else {
        anyhow::bail!(
            "invalid day '{}' — use a weekday name (monday-sunday) or ISO date (YYYY-MM-DD)",
            day
        );
    }
}

fn cmd_plan_add(
    paths: &FondPaths,
    plan_name: &str,
    assignment: &str,
    fmt: &OutputFormat,
) -> Result<()> {
    let (day, meal, recipe_slug) = parse_plan_assignment(assignment)?;
    let plan_date = resolve_day_to_date(&day)?;

    let db = open_db(paths)?;
    let plan_repo = fond_store::MealPlanRepository::new(&db);

    let plan_id = plan_repo
        .get_or_create(plan_name)
        .context("failed to create/get plan")?;

    plan_repo
        .add_entry(plan_id, &plan_date, &meal, &recipe_slug)
        .context("failed to add plan entry")?;

    match fmt {
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct PlanEntry {
                plan: String,
                date: String,
                day: String,
                meal: String,
                recipe: String,
            }
            let out = PlanEntry {
                plan: plan_name.to_string(),
                date: plan_date.clone(),
                day,
                meal: meal.clone(),
                recipe: recipe_slug.clone(),
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Table => {
            println!(
                "Added {} {} → {} ({})",
                plan_date, meal, recipe_slug, plan_name
            );
        }
    }

    Ok(())
}

fn cmd_plan_show(paths: &FondPaths, plan_name: &str, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let plan_repo = fond_store::MealPlanRepository::new(&db);

    let plan = plan_repo
        .get_plan(plan_name)
        .context("database query failed")?
        .with_context(|| {
            format!("no plan found with name '{plan_name}' — create one with `fond plan add {plan_name} day:meal=recipe`")
        })?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
        OutputFormat::Table => {
            println!("📅 Meal Plan: {}\n", plan.name);

            if plan.entries.is_empty() {
                println!(
                    "  (empty — add entries with `fond plan add {} day:meal=recipe`)",
                    plan_name
                );
                return Ok(());
            }

            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["Date", "Meal", "Recipe"]);

            let mut current_date = String::new();
            for entry in &plan.entries {
                let date_display = if entry.plan_date != current_date {
                    current_date = entry.plan_date.clone();
                    // Try to add weekday name
                    format_plan_date(&entry.plan_date)
                } else {
                    String::new()
                };

                let recipe_display = entry.recipe_title.as_deref().unwrap_or(&entry.recipe_slug);

                table.add_row(vec![
                    date_display,
                    entry.meal.clone(),
                    recipe_display.to_string(),
                ]);
            }

            println!("{table}");
            println!("\n{} meal(s) planned", plan.entries.len());
        }
    }

    Ok(())
}

/// Format a plan date for display, adding the weekday name.
fn format_plan_date(date_str: &str) -> String {
    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        let weekday = date.format("%A").to_string();
        format!("{weekday} ({date_str})")
    } else {
        date_str.to_string()
    }
}

fn cmd_plan_rm(
    paths: &FondPaths,
    plan_name: &str,
    assignment: &str,
    fmt: &OutputFormat,
) -> Result<()> {
    let (day, meal, recipe_slug) = parse_plan_assignment(assignment)?;
    let plan_date = resolve_day_to_date(&day)?;

    let db = open_db(paths)?;
    let plan_repo = fond_store::MealPlanRepository::new(&db);

    let plan = plan_repo
        .get_plan(plan_name)
        .context("database query failed")?
        .with_context(|| format!("no plan found with name '{plan_name}'"))?;

    let removed = plan_repo
        .remove_entry(plan.id, &plan_date, &meal, &recipe_slug)
        .context("failed to remove entry")?;

    if !removed {
        anyhow::bail!(
            "no matching entry found for {} {} {} in plan '{}'",
            plan_date,
            meal,
            recipe_slug,
            plan_name
        );
    }

    match fmt {
        OutputFormat::Json => {
            println!(
                r#"{{"removed": true, "plan": "{}", "date": "{}", "meal": "{}", "recipe": "{}"}}"#,
                plan_name, plan_date, meal, recipe_slug
            );
        }
        OutputFormat::Table => {
            println!(
                "Removed {} {} ← {} ({})",
                plan_date, meal, recipe_slug, plan_name
            );
        }
    }

    Ok(())
}

fn cmd_plan_list(paths: &FondPaths, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let plan_repo = fond_store::MealPlanRepository::new(&db);

    let plans = plan_repo.list_plans().context("failed to list plans")?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&plans)?);
        }
        OutputFormat::Table => {
            if plans.is_empty() {
                println!("No meal plans. Create one with `fond plan add <name> day:meal=recipe`.");
                return Ok(());
            }

            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["Name", "Meals", "Created"]);

            for p in &plans {
                table.add_row(vec![&p.name, &p.entry_count.to_string(), &p.created_at]);
            }

            println!("{table}");
            println!("\n{} plan(s)", plans.len());
        }
    }

    Ok(())
}

fn cmd_plan_clear(paths: &FondPaths, plan_name: &str, yes: bool, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let plan_repo = fond_store::MealPlanRepository::new(&db);

    if !yes && !confirm(&format!("Clear all entries from plan '{plan_name}'?")) {
        println!("Cancelled.");
        return Ok(());
    }

    let cleared = plan_repo
        .clear_plan(plan_name)
        .context("failed to clear plan")?;

    match fmt {
        OutputFormat::Json => {
            println!(
                r#"{{"cleared": true, "plan": "{}", "entries_removed": {}}}"#,
                plan_name, cleared
            );
        }
        OutputFormat::Table => {
            println!("Cleared {cleared} entry(ies) from plan '{plan_name}'.");
        }
    }

    Ok(())
}

fn cmd_plan_delete(
    paths: &FondPaths,
    plan_name: &str,
    yes: bool,
    fmt: &OutputFormat,
) -> Result<()> {
    let db = open_db(paths)?;
    let plan_repo = fond_store::MealPlanRepository::new(&db);

    if !yes && !confirm(&format!("Delete plan '{plan_name}' and all its entries?")) {
        println!("Cancelled.");
        return Ok(());
    }

    let deleted = plan_repo
        .delete_plan(plan_name)
        .context("failed to delete plan")?;

    if !deleted {
        anyhow::bail!("no plan found with name '{plan_name}'");
    }

    match fmt {
        OutputFormat::Json => {
            println!(r#"{{"deleted": true, "plan": "{}"}}"#, plan_name);
        }
        OutputFormat::Table => {
            println!("Deleted plan '{plan_name}'.");
        }
    }

    Ok(())
}

fn cmd_grocery_from_plan(
    paths: &FondPaths,
    plan_name: &str,
    include_pantry: bool,
    fmt: &OutputFormat,
) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db = open_db(paths)?;
    let grocery = fond_store::GroceryRepository::new(&db);

    let list = grocery
        .from_plan(plan_name, include_pantry)
        .context("failed to generate consolidated grocery list")?;

    let Some(list) = list else {
        anyhow::bail!("plan not found: '{plan_name}'");
    };

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&list)?);
        }
        OutputFormat::Table => {
            println!(
                "🛒 Consolidated grocery list for plan: {}\n",
                list.plan_name
            );

            if list.recipe_count == 0 {
                println!(
                    "  Plan is empty — add entries with `fond plan add {} day:meal=recipe`.",
                    plan_name
                );
                return Ok(());
            }

            println!(
                "  {} recipe(s): {}",
                list.recipe_count,
                list.recipe_slugs.join(", ")
            );
            println!(
                "  {} total ingredient(s), {} consolidated, {} in pantry, {} to buy\n",
                list.total_ingredients,
                list.consolidated_items,
                list.pantry_covered_count,
                list.items_to_buy
            );

            if list.items.is_empty() {
                if list.pantry_covered_count > 0 {
                    println!("Everything is already in your pantry! 🎉");
                } else {
                    println!("No ingredients found in the planned recipes.");
                }
            } else {
                let mut current_category = "";

                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["", "Qty", "Unit", "Ingredient", "For", "Note"]);

                for item in &list.items {
                    if item.category != current_category {
                        current_category = &item.category;
                        table.add_row(vec![
                            format!("── {current_category} ──"),
                            String::new(),
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
                    let from = item.from_recipes.join(", ");

                    table.add_row(vec![status, qty, unit, item.name.clone(), from, note]);
                }

                println!("{table}");
            }
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

    // Fetch HTML via fond-scrape HTTP client
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

fn cmd_import_photo(
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
    if !source_path.is_file() {
        anyhow::bail!("expected an image file, got: {}", source_path.display());
    }
    if !is_supported_image_path(source_path) {
        anyhow::bail!(
            "unsupported image format for OCR: {}",
            source_path.display()
        );
    }

    let source_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("imported-image")
        .to_string();

    let ocr_text = ocr::extract_text(source_path)
        .with_context(|| format!("failed to OCR {}", source_path.display()))?;
    let draft = fond_import::ocr::build_review_draft(&ocr_text, &source_name);

    let mut report = fond_import::ImportReport::new();
    let reason = queue_reason(&draft.warnings);

    if dry_run {
        report.add(fond_import::ImportResult::Queued {
            title: draft.title.clone(),
            review_id: None,
            reason,
        });
        return print_import_report_for_format(fmt, &report, true);
    }

    let asset_path = copy_review_asset(paths, source_path)?;
    let db = open_db(paths)?;
    let review_repo = ImportReviewRepository::new(&db);
    let created = review_repo.create(&NewImportReview {
        source_type: "ocr-photo".to_string(),
        source_name,
        asset_path,
        title: draft.title.clone(),
        draft_cook_text: draft.cook_text,
        ocr_text: draft.raw_text,
        warnings: draft.warnings.clone(),
    })?;

    report.add(fond_import::ImportResult::Queued {
        title: created.title,
        review_id: Some(created.id),
        reason,
    });

    print_import_report_for_format(fmt, &report, false)
}

/// Fetch HTML from a URL using the fond-scrape HTTP client.
fn fetch_url(url: &str) -> Result<String> {
    let client = fond_scrape::ScrapeClient::new().context("failed to build HTTP client")?;
    client.fetch_html(url).map_err(|e| anyhow::anyhow!("{e}"))
}

fn print_import_report_for_format(
    fmt: &OutputFormat,
    report: &fond_import::ImportReport,
    dry_run: bool,
) -> Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
        OutputFormat::Table => {
            print_import_report(report, dry_run);
        }
    }
    Ok(())
}

fn print_import_report(report: &fond_import::ImportReport, dry_run: bool) {
    let prefix = if dry_run { "[dry-run] " } else { "" };

    if report.imported > 0 {
        println!("{prefix}Imported: {} recipe(s)", report.imported);
    }
    if report.queued > 0 {
        println!("{prefix}Queued:   {} recipe(s)", report.queued);
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
            fond_import::ImportResult::Queued {
                title,
                review_id,
                reason,
            } => {
                let suffix = review_id
                    .as_ref()
                    .map(|id| format!(" (review id: {id})"))
                    .unwrap_or_default();
                println!("  Queued:  {title} — {reason}{suffix}");
            }
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

fn cmd_review_list(paths: &FondPaths, fmt: &OutputFormat) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db = open_db(paths)?;
    let repo = ImportReviewRepository::new(&db);
    let items = repo.list_pending().context("failed to list review queue")?;

    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&items)?),
        OutputFormat::Table => {
            if items.is_empty() {
                println!("No queued review drafts.");
            } else {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["ID", "Title", "Source", "Warnings", "Created"]);
                for item in &items {
                    table.add_row(vec![
                        item.id.clone(),
                        item.title.clone(),
                        item.source_name.clone(),
                        item.warnings.len().to_string(),
                        item.created_at.clone(),
                    ]);
                }
                println!("{table}");
                println!("{} pending draft(s)", items.len());
            }
        }
    }

    Ok(())
}

fn cmd_review_show(paths: &FondPaths, id: &str, fmt: &OutputFormat) -> Result<()> {
    let item = load_review_item(paths, id)?;

    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&item)?),
        OutputFormat::Table => {
            println!("Review ID: {}", item.id);
            println!("Title: {}", item.title);
            println!("Status: {}", item.status);
            println!("Source: {}", item.source_name);
            if !item.asset_path.is_empty() {
                println!("Asset: {}", item.asset_path);
            }
            if !item.warnings.is_empty() {
                println!("\nWarnings:");
                for warning in &item.warnings {
                    println!("  • {warning}");
                }
            }
            println!("\n--- Cooklang draft ---\n");
            println!("{}", item.draft_cook_text.trim_end());
            if !item.ocr_text.trim().is_empty() {
                println!("\n--- Raw OCR text ---\n");
                println!("{}", item.ocr_text.trim_end());
            }
        }
    }

    Ok(())
}

fn cmd_review_edit(paths: &FondPaths, id: &str) -> Result<()> {
    let item = load_pending_review_item(paths, id)?;
    let temp_path = paths.data_dir.join(format!("review-edit-{id}.cook"));

    std::fs::write(&temp_path, &item.draft_cook_text)
        .with_context(|| format!("failed to write {}", temp_path.display()))?;

    let opened = open_editor(&temp_path)?;
    let updated = std::fs::read_to_string(&temp_path)
        .with_context(|| format!("failed to read {}", temp_path.display()))?;
    let _ = std::fs::remove_file(&temp_path);

    if !opened {
        anyhow::bail!("editor exited with an error");
    }

    if updated == item.draft_cook_text {
        println!("No changes saved for review draft {id}.");
        return Ok(());
    }

    let title = extract_title_from_cook_text(&updated).unwrap_or(item.title);
    let db = open_db(paths)?;
    let repo = ImportReviewRepository::new(&db);
    if !repo
        .update_draft(id, &title, &updated)
        .context("failed to update queued draft")?
    {
        anyhow::bail!("review draft '{id}' is no longer pending");
    }

    if let Err(err) = fond_domain::parse_cook(&updated, &slug_or_fallback(&title)) {
        eprintln!(
            "Saved draft {id}, but it still has parse issues and may need more editing before accept.\n  Error: {err}"
        );
    } else {
        println!("Updated review draft {id}.");
    }

    Ok(())
}

fn cmd_review_accept(paths: &FondPaths, id: &str, fmt: &OutputFormat) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let item = load_pending_review_item(paths, id)?;
    let provisional_stem = slug_or_fallback(&item.title);
    let parsed = fond_domain::parse_cook(&item.draft_cook_text, &provisional_stem).map_err(|e| {
        anyhow::anyhow!("queued draft could not be parsed as Cooklang: {e}. Use `fond review edit {id}` to fix it")
    })?;

    let db = open_db(paths)?;
    let review_repo = ImportReviewRepository::new(&db);
    let recipe_repo = RecipeRepository::new(&db);
    let existing = recipe_repo
        .list_recipes()
        .context("failed to list existing recipes")?;
    let existing_slugs: Vec<String> = existing.iter().map(|recipe| recipe.slug.clone()).collect();
    let final_slug = resolve_slug_collision(&parsed.slug, &existing_slugs);
    let file_name = format!("{final_slug}.cook");
    let cook_text = item.draft_cook_text.clone();
    let recipe = fond_domain::Recipe {
        slug: final_slug.clone(),
        raw_source: Some(cook_text.clone()),
        ..parsed
    };

    let dest = recipes_dir(paths).join(&file_name);
    std::fs::write(&dest, &cook_text)
        .with_context(|| format!("failed to write {}", dest.display()))?;
    let hash = content_hash(&cook_text);
    recipe_repo
        .upsert_recipe(&file_name, &recipe, &hash)
        .with_context(|| format!("failed to index {}", file_name))?;

    if !review_repo
        .mark_accepted(id, &final_slug, &file_name)
        .context("failed to update review queue status")?
    {
        anyhow::bail!("review draft '{id}' is no longer pending");
    }

    #[derive(Serialize)]
    struct AcceptedReview<'a> {
        review_id: &'a str,
        title: &'a str,
        slug: &'a str,
        file_name: &'a str,
    }

    let accepted = AcceptedReview {
        review_id: id,
        title: &recipe.title,
        slug: &final_slug,
        file_name: &file_name,
    };

    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&accepted)?),
        OutputFormat::Table => {
            println!("Accepted review draft {id}.");
            println!("Imported: {} ({})", recipe.title, final_slug);
        }
    }

    Ok(())
}

fn cmd_review_reject(paths: &FondPaths, id: &str, yes: bool, fmt: &OutputFormat) -> Result<()> {
    let item = load_pending_review_item(paths, id)?;
    if !yes && !confirm(&format!("Reject queued draft '{}' ?", item.title)) {
        println!("Aborted.");
        return Ok(());
    }

    let db = open_db(paths)?;
    let repo = ImportReviewRepository::new(&db);
    if !repo
        .mark_rejected(id)
        .context("failed to reject review draft")?
    {
        anyhow::bail!("review draft '{id}' is no longer pending");
    }

    #[derive(Serialize)]
    struct RejectedReview<'a> {
        review_id: &'a str,
        title: &'a str,
        status: &'static str,
    }

    let rejected = RejectedReview {
        review_id: id,
        title: &item.title,
        status: "rejected",
    };

    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rejected)?),
        OutputFormat::Table => println!("Rejected review draft {id} ({}).", item.title),
    }

    Ok(())
}

fn load_review_item(paths: &FondPaths, id: &str) -> Result<ImportReviewRecord> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;
    let db = open_db(paths)?;
    let repo = ImportReviewRepository::new(&db);
    repo.get(id)
        .context("failed to load review queue item")?
        .with_context(|| format!("no review draft found with id '{id}'"))
}

fn load_pending_review_item(paths: &FondPaths, id: &str) -> Result<ImportReviewRecord> {
    let item = load_review_item(paths, id)?;
    if item.status != "pending" {
        anyhow::bail!(
            "review draft '{id}' is not pending (status: {})",
            item.status
        );
    }
    Ok(item)
}

fn queue_reason(warnings: &[String]) -> String {
    if warnings.is_empty() {
        "queued for manual review".to_string()
    } else {
        format!("queued for manual review ({} warning(s))", warnings.len())
    }
}

fn is_supported_image_path(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff")
    )
}

fn copy_review_asset(paths: &FondPaths, source_path: &std::path::Path) -> Result<String> {
    let bytes = std::fs::read(source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let hash = bytes_hash(&bytes);
    let ext = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_else(|| "img".to_string());

    let dir = review_assets_dir(paths);
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let file_name = format!("{hash}.{ext}");
    let dest = dir.join(&file_name);
    if !dest.exists() {
        std::fs::write(&dest, bytes)
            .with_context(|| format!("failed to write {}", dest.display()))?;
    }

    Ok(format!("photos/review/{file_name}"))
}

fn extract_title_from_cook_text(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }

    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':')
            && key.trim().eq_ignore_ascii_case("title")
        {
            let title = value.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }

    None
}

fn slug_or_fallback(title: &str) -> String {
    let slug = fond_domain::slugify(title);
    if slug.is_empty() {
        "imported-recipe".to_string()
    } else {
        slug
    }
}

fn resolve_slug_collision(slug: &str, existing: &[String]) -> String {
    if !existing.iter().any(|existing_slug| existing_slug == slug) {
        return slug.to_string();
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{slug}-{suffix}");
        if !existing
            .iter()
            .any(|existing_slug| existing_slug == &candidate)
        {
            return candidate;
        }
        suffix += 1;
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

// ═══════════════════════════════════════════════════════════════════
// Nutrition
// ═══════════════════════════════════════════════════════════════════

fn cmd_nutrition(paths: &FondPaths, slug: &str, fmt: &OutputFormat) -> Result<()> {
    let db = open_db(paths)?;
    let repo = fond_store::NutritionRepository::new(&db);
    repo.seed_nutrition_facts()?;

    let result = repo
        .estimate_recipe_nutrition(slug)?
        .ok_or_else(|| anyhow::anyhow!("recipe '{}' not found", slug))?;

    match fmt {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&result)?;
            println!("{json}");
        }
        OutputFormat::Table => {
            println!("Nutrition estimate for: {}\n", result.recipe_slug);

            if let Some(servings) = result.servings {
                println!("Servings: {servings}");
            }

            // Matched ingredients table
            if !result.matched.is_empty() {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["Ingredient", "USDA Match", "Conf.", "Grams", "kcal"]);

                for m in &result.matched {
                    let conf = match m.confidence {
                        fond_store::MatchConfidence::High => "high",
                        fond_store::MatchConfidence::Medium => "med",
                    };
                    let rounded = m.contribution.rounded();
                    table.add_row(vec![
                        m.ingredient_name.clone(),
                        truncate_str(&m.usda_description, 30),
                        conf.to_string(),
                        format!("{:.0}", m.grams),
                        format!("{:.0}", rounded.kcal),
                    ]);
                }
                println!("{table}");
            }

            // Totals
            let total = result.total.rounded();
            println!("\n── Totals ──");
            println!("  Calories:  {:.0} kcal", total.kcal);
            println!("  Protein:   {:.0} g", total.protein_g);
            println!("  Fat:       {:.0} g", total.fat_g);
            println!("  Carbs:     {:.0} g", total.carb_g);
            if let Some(fiber) = total.fiber_g {
                println!("  Fiber:     {:.0} g", fiber);
            }
            if let Some(sugar) = total.sugar_g {
                println!("  Sugar:     {:.0} g", sugar);
            }
            if let Some(sodium) = total.sodium_mg {
                println!("  Sodium:    {:.0} mg", sodium);
            }

            if let Some(ref per_serving) = result.per_serving {
                let ps = per_serving.rounded();
                println!(
                    "\n── Per Serving ({} servings) ──",
                    result.servings.unwrap()
                );
                println!("  Calories:  {:.0} kcal", ps.kcal);
                println!("  Protein:   {:.0} g", ps.protein_g);
                println!("  Fat:       {:.0} g", ps.fat_g);
                println!("  Carbs:     {:.0} g", ps.carb_g);
                if let Some(fiber) = ps.fiber_g {
                    println!("  Fiber:     {:.0} g", fiber);
                }
                if let Some(sugar) = ps.sugar_g {
                    println!("  Sugar:     {:.0} g", sugar);
                }
                if let Some(sodium) = ps.sodium_mg {
                    println!("  Sodium:    {:.0} mg", sodium);
                }
            }

            // Coverage
            println!(
                "\nCoverage: {} of {} ingredients matched ({:.0}%)",
                result.matched_count, result.ingredient_count, result.coverage_pct
            );

            // Unmatched
            if !result.unmatched.is_empty() {
                println!("\nUnmatched ingredients:");
                for u in &result.unmatched {
                    println!("  • {} — {}", u.name, u.reason);
                }
            }

            // Disclaimer
            println!("\n⚠ {}", fond_store::NUTRITION_DISCLAIMER);
        }
    }

    Ok(())
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

fn cmd_serve(paths: &FondPaths, port: u16, bind: &str) -> Result<()> {
    let db_path = paths.data_dir.join("fond.db");
    if !db_path.exists() {
        anyhow::bail!(
            "no fond database found at {} — run `fond init` first",
            db_path.display()
        );
    }

    let config = fond_web::ServeConfig {
        bind: bind.to_string(),
        port,
        data_dir: paths.data_dir.clone(),
    };

    eprintln!("Starting fond web UI on http://{}:{}", bind, port);
    eprintln!("Press Ctrl+C to stop.");

    tokio::runtime::Runtime::new()
        .context("failed to create async runtime")?
        .block_on(fond_web::serve(config))
}
