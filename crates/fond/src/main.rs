use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fond_store::{FondDb, FondPaths, RecipeRepository};

/// fond — a private, local-first personal cooking & recipe manager.
#[derive(Parser)]
#[command(name = "fond", version, about)]
struct Cli {
    /// Data directory (default: platform-specific)
    #[arg(long, env = "FOND_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise the fond data directory and default structure.
    Init,

    /// Rebuild the search index from .cook files on disk.
    Reindex,

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = FondPaths::resolve(cli.data_dir);

    match cli.command {
        Commands::Init => cmd_init(&paths)?,
        Commands::Reindex => cmd_reindex(&paths)?,
        Commands::View { slug } => cmd_view(&paths, &slug)?,
        Commands::List => cmd_list(&paths)?,
        Commands::Search { query } => cmd_search(&paths, &query)?,
    }

    Ok(())
}

fn cmd_init(paths: &FondPaths) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    println!("Initialised fond at {}", paths.data_dir.display());
    println!("  recipes/  — your .cook recipe files");
    println!("  config/   — fond configuration");
    Ok(())
}

fn cmd_reindex(paths: &FondPaths) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    let db_path = paths.data_dir.join("fond.db");
    let db = FondDb::open(&db_path).context("failed to open database")?;

    let recipes_dir = paths.data_dir.join("recipes");
    let report = fond_store::reindex(&db, &recipes_dir).context("reindex failed")?;

    println!("Reindexed {} recipe(s)", report.indexed);
    if !report.errors.is_empty() {
        eprintln!("\nWarnings:");
        for (file, err) in &report.errors {
            eprintln!("  {file}: {err}");
        }
    }
    Ok(())
}

fn cmd_view(paths: &FondPaths, slug: &str) -> Result<()> {
    let db_path = paths.data_dir.join("fond.db");
    let db = FondDb::open(&db_path).context("failed to open database")?;
    let repo = RecipeRepository::new(&db);

    let record = repo
        .get_recipe_by_slug(slug)
        .context("database query failed")?
        .with_context(|| format!("no recipe found with slug '{slug}'"))?;

    // Parse from raw_source (file content) for full fidelity
    let recipes_dir = paths.data_dir.join("recipes");
    let file_path = recipes_dir.join(&record.file_path);

    let content = if file_path.exists() {
        std::fs::read_to_string(&file_path).context("failed to read recipe file")?
    } else if !record.raw_source.is_empty() {
        record.raw_source.clone()
    } else {
        anyhow::bail!("recipe file not found: {}", record.file_path);
    };

    let recipe = fond_domain::parse_cook(&content, slug)
        .map_err(|e| anyhow::anyhow!("failed to parse recipe: {e}"))?;

    // Display
    println!("# {}", recipe.title);
    if let Some(ref source) = recipe.source {
        println!("Source: {source}");
    }
    if let Some(ref s) = recipe.servings {
        println!("Servings: {s}");
    }
    if let Some(ref t) = recipe.prep_time {
        print!("Prep: {t}  ");
    }
    if let Some(ref t) = recipe.cook_time {
        print!("Cook: {t}  ");
    }
    if recipe.prep_time.is_some() || recipe.cook_time.is_some() {
        println!();
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

    Ok(())
}

fn cmd_list(paths: &FondPaths) -> Result<()> {
    let db_path = paths.data_dir.join("fond.db");
    let db = FondDb::open(&db_path).context("failed to open database")?;
    let repo = RecipeRepository::new(&db);

    let recipes = repo.list_recipes().context("failed to list recipes")?;

    if recipes.is_empty() {
        println!("No recipes indexed. Add .cook files and run `fond reindex`.");
        return Ok(());
    }

    for r in &recipes {
        let tags = if r.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", r.tags.join(", "))
        };
        println!("  {} — {}{tags}", r.slug, r.title);
    }
    println!("\n{} recipe(s)", recipes.len());
    Ok(())
}

fn cmd_search(paths: &FondPaths, query: &str) -> Result<()> {
    let db_path = paths.data_dir.join("fond.db");
    let db = FondDb::open(&db_path).context("failed to open database")?;
    let repo = RecipeRepository::new(&db);

    let results = repo.search(query).context("search failed")?;

    if results.is_empty() {
        println!("No results for '{query}'.");
        return Ok(());
    }

    for r in &results {
        println!("  {} — {}", r.slug, r.title);
    }
    println!("\n{} result(s)", results.len());
    Ok(())
}
