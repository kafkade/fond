# Getting Started

## Installation

Build from source (requires Rust 1.85+):

```bash
git clone https://github.com/kafkade/fond.git
cd fond
cargo install --path crates/fond
```

## Initialize

Create your recipe collection directory:

```bash
fond init
```

This creates the fond data directory with:

```
~/fond/
  recipes/        ← your .cook files (source of truth)
  photos/         ← content-addressed images
  fond.db         ← SQLite index (derived, rebuildable)
  config.toml     ← configuration
```

You can set a custom location with `--data-dir` or the `FOND_DATA_DIR` environment variable.

## Your First Recipe

Create a `.cook` file in the `recipes/` directory. Cooklang is a simple plain-text format:

```cooklang
---
title: Scrambled Eggs
servings: 2
tags:
  - breakfast
  - quick
---

Crack @eggs{3} into a bowl and whisk with @salt{a pinch} and @pepper{a pinch}.

Heat @butter{1 tbsp} in a #non-stick pan{} over medium-low heat.

Pour in eggs and stir gently with a #spatula{} for ~{3 minutes}.

Serve immediately.
```

Then index it:

```bash
fond reindex
```

## View Your Recipes

```bash
# List all recipes
fond list

# View a specific recipe
fond view scrambled-eggs

# Search by keyword
fond search "eggs"

# Filter by tag
fond list --tag breakfast
```

## Shell Completions

Generate completions for your shell:

```bash
# Bash
fond completions bash > ~/.bash_completion.d/fond

# Zsh
fond completions zsh > ~/.zfunc/_fond

# Fish
fond completions fish > ~/.config/fish/completions/fond.fish

# PowerShell
fond completions powershell > _fond.ps1
```
