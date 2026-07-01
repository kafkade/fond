# Syncing Your Recipes

fond is local-first: your recipes live on your own disk and work fully offline.
When you want the *same* collection on more than one machine — a laptop and a
desktop, or two people in a household — you sync the **files you own** with a
tool you control. fond does not run a sync server, and there is no fond account.

This guide covers **Tier 1** sync (recipe content): the `.cook` files and photos.
It is a supported, tested path today. Personal-overlay sync (notes, ratings,
cook logs) is a separate, later problem — see
[ADR-012](https://github.com/kafkade/fond/blob/main/docs/adr/012-sync-multi-device.md).

## The one rule: never sync `fond.db`

Your fond data directory looks like this:

```text
~/fond/
  recipes/        ← .cook files   (SOURCE OF TRUTH — sync these)
  photos/         ← image blobs   (content-addressed — sync these)
  fond.db         ← SQLite index  (DERIVED — do NOT sync)
  fond.db-wal     ← SQLite WAL    (DERIVED — do NOT sync)
  fond.db-shm     ← SQLite shared-mem (DERIVED — do NOT sync)
  config.toml     ← configuration
```

**Sync `recipes/` and `photos/`. Never sync `fond.db`.**

`fond.db` is a *rebuildable* index derived from your `.cook` files. It is
device-specific: its internal row IDs mean different things on different
machines. If you sync the database between devices, two copies will overwrite
and corrupt each other.

Instead, each device rebuilds its own index locally:

```bash
fond reindex
```

Run `fond reindex` on a device after new or changed files arrive from sync. The
`.cook` files are sacred; the database is disposable.

> Not sure whether your database is sitting in a synced folder? Run
> [`fond doctor`](#checking-your-setup-with-fond-doctor) — it will warn you.

### Two layouts that make this easy

Because you must exclude `fond.db` from sync, pick one of these:

1. **Exclude the database with an ignore rule** (keep the default layout). Every
   sync tool supports ignore patterns — add `fond.db`, `fond.db-wal`, and
   `fond.db-shm`. Per-tool patterns are in the sections below.
2. **Put the database outside the synced folder.** Keep only `recipes/` and
   `photos/` in the synced directory, and point fond's data directory
   elsewhere on each device with `--data-dir` / `FOND_DATA_DIR`, symlinking or
   copying the synced `recipes/` and `photos/` into it. Layout (1) is simpler
   for most people.

## Option 1 — Syncthing (recommended)

[Syncthing](https://syncthing.net/) is free, open-source, peer-to-peer file
sync. Your files go **directly** between your own devices — no third party ever
holds a copy, and there is no subscription. This is the best fit for fond's
local-first, you-own-your-data promise.

### Setup

1. Install Syncthing on both machines (see the
   [Syncthing downloads](https://syncthing.net/downloads/)).
2. On machine A, add `~/fond` as a shared folder. Note its **Folder ID**.
3. On machine B, add the same Folder ID and point it at `~/fond`.
4. Pair the two devices (each shows a Device ID / QR code) and accept the share.

### Exclude the database

In the shared folder, create a `.stignore` file so Syncthing never carries the
derived index:

```text
// ~/fond/.stignore
fond.db
fond.db-wal
fond.db-shm
```

### Day-to-day

- Edit recipes on either machine. Syncthing propagates changed `.cook` files and
  new photos when both devices are online.
- After changes land, run `fond reindex` on the receiving device.
- **Conflicts:** if you edit the *same* `.cook` file on both machines before
  they sync, Syncthing keeps both and names one
  `recipe.sync-conflict-<date>-<device>.cook`. Because `.cook` is line-oriented
  plain text, you can diff and merge it with any editor, then `fond reindex`.

## Option 2 — Cloud folders (Dropbox / iCloud Drive / Google Drive / OneDrive)

If you already use a cloud folder, put `~/fond` inside it. This is the
zero-setup option and works well on always-on machines and mobile.

**Trade-off:** a third party stores a copy of your files, usually behind a
subscription. That is the vendor custody fond exists to let you escape — so use
the cloud as *transport only*. Your recipes stay portable `.cook` files you can
walk away with at any time.

### Exclude the database

Cloud tools use different ignore mechanisms:

- **Dropbox:** mark the database files as ignored, e.g.

  ```bash
  dropbox exclude add ~/fond/fond.db ~/fond/fond.db-wal ~/fond/fond.db-shm
  ```

  (Or use Dropbox's "Ignored" file attribute from the desktop app.)
- **iCloud Drive / Google Drive / OneDrive:** these have limited per-file ignore
  support. The reliable approach is layout (2) above — keep only `recipes/` and
  `photos/` in the cloud folder and place `fond.db` outside it via `--data-dir`.

Whatever the tool, the rule is unchanged: the database must not travel, and each
device runs `fond reindex` after files arrive.

## Option 3 — Git (power users)

Recipes are plain text, so a git repository gives you full history, real diffs,
and explicit conflict resolution.

```bash
cd ~/fond
git init
cat > .gitignore <<'EOF'
fond.db
fond.db-wal
fond.db-shm
EOF
git add recipes photos .gitignore
git commit -m "My recipes"
git remote add origin <your-remote>
git push -u origin main
```

On another machine, `git clone` the repo and run `fond reindex`.

- **Conflicts** surface as ordinary git merge conflicts in the `.cook` file —
  resolve them in your editor, commit, then `fond reindex`.
- **Photos** are binary and can bloat history. Either keep them out of git or
  use [Git LFS](https://git-lfs.com/).
- Pushes and pulls are manual (or scripted); unlike Syncthing/cloud, git does
  not sync in the background.

## Checking your setup with `fond doctor`

`fond doctor` inspects your data directory and warns if it looks like your
`fond.db` is inside a folder managed by a sync tool:

```bash
fond doctor
```

A clean setup reports:

```text
fond doctor
  data dir: /home/you/fond
  database: /home/you/fond/fond.db (present)

[ok] No file-sync tool detected around your data directory.
     Recipes and photos are safe to sync; just keep fond.db out of the synced set.
```

If a synced folder is detected, it prints the tool it found and a reminder to
exclude the database and `fond reindex` per device. The check is advisory — it
never fails a command — and machine-readable output is available with
`fond doctor --format json`.

## Validation checklist (two-machine round-trip)

Use this to confirm an end-to-end Tier 1 setup. "A" and "B" are two machines
sharing `~/fond` via one of the options above.

1. **Baseline on A.** `fond add` a couple of recipes (one with a photo). Run
   `fond reindex`, then `fond list` — note the collection.
2. **Exclude the database.** Confirm your ignore rule covers `fond.db`,
   `fond.db-wal`, `fond.db-shm`. Run `fond doctor` — for Syncthing/Dropbox/git it
   should still `[warning]` that the folder is synced (that is expected; the
   point is that the *database* is excluded).
3. **Sync to B.** Wait for `recipes/` and `photos/` to arrive on B. Verify
   `fond.db` did **not** come across (it should be absent or untouched on B).
4. **Reindex on B.** Run `fond reindex` on B, then `fond list`. B should show the
   **same** recipes as A, and `fond view <slug>` should render identically,
   including the photo.
5. **Reverse edit.** On B, `fond tag <slug> --add weeknight` (edits the `.cook`
   file) and let it sync back. On A, run `fond reindex` and confirm the tag
   appears.
6. **Conflict drill (optional).** Edit the *same* recipe on A and B while
   offline, then reconnect. Confirm your tool surfaces both versions
   (`*.sync-conflict-*.cook` or a git conflict), merge by hand, and `fond
   reindex`. No data is silently lost.

If every step passes, Tier 1 sync is working: the files are the source of truth,
the database is rebuilt per device, and nothing is lost.

## What is *not* synced by Tier 1

Tier 1 covers recipe **content** only. Personal-overlay data that lives solely in
`fond.db` — notes, ratings, cook logs, pantry, meal plans, dietary profiles —
does **not** sync this way, by design. Multi-device sync of that authored data is
deliberately deferred (see
[ADR-012](https://github.com/kafkade/fond/blob/main/docs/adr/012-sync-multi-device.md)).
For now, treat those as per-device, or keep a single primary machine for them.
