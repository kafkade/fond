# Scheduling Backups

fond is local-first, and that includes how it backs up. `fond backup create`
writes a single self-contained archive of everything you own — your `.cook`
recipes, photos, and authored-overlay sidecars — that a later `fond backup
restore` can rebuild a whole install from. (The derived `fond.db` is *not* stored;
it is rebuilt by `fond reindex` after a restore.)

This guide is about running that command **on a schedule**, unattended.

**fond ships no backup daemon.** There is no resident background service, no cron
built into the app, and nothing phoning home. Instead you hand the *when* to the
scheduler your operating system already has — cron, launchd, or Windows Task
Scheduler. That keeps fond small, keeps you in control, and means backups run with
exactly the permissions and environment you grant them.

For the command itself and its flags, see [`fond backup`](./cli-reference.md#fond-backup).

## The command your scheduler will run

The whole job is one line:

```bash
fond backup create
```

With no `--dest`, fond writes a timestamped archive to
`<data-dir>/backups/fond-backup-<timestamp>.fondbkp`. Because every run gets a
fresh timestamp, scheduled backups never overwrite each other — you accumulate a
history, and [rotation](#rotation-and-retention) is a separate, deliberate step.

Two things matter for *unattended* runs:

- **Use an absolute path to `fond`.** Schedulers start with a minimal `PATH`, so
  spell out the binary — e.g. `/usr/local/bin/fond` (find yours with
  `command -v fond`).
- **Point fond at your data directory explicitly** if it isn't the platform
  default, since a scheduled job may not resolve the same default as your login
  shell. Set `FOND_DATA_DIR=/path/to/fond` (or pass `--data-dir /path/to/fond`).

Backups are **plaintext by default** — unencrypted but integrity-checked — and
need no key. If you back up to untrusted media, see
[Encrypted scheduled backups](#encrypted-scheduled-backups) first.

## cron (Linux and macOS)

Run `crontab -e` and add a daily job. The `FOND_DATA_DIR` line sets the
environment for the jobs below it:

```cron
# fond: back up every day at 02:30
FOND_DATA_DIR=/home/you/fond
30 2 * * * /usr/local/bin/fond backup create >> /home/you/fond/backups/backup.log 2>&1
```

Notes:

- cron has no idea where `fond` lives — the absolute path is required.
- Redirecting to a log file (`>> … 2>&1`) captures success and failure output;
  check it after the first scheduled run.
- If you prefer to write straight to external media and name files yourself,
  remember cron treats `%` specially — escape it as `\%`:

  ```cron
  30 2 * * * /usr/local/bin/fond backup create --dest "/mnt/backup/fond-backup-$(date +\%F-\%H\%M\%S).fondbkp" >> /home/you/fond/backups/backup.log 2>&1
  ```

## launchd (macOS)

On macOS, launchd is more reliable than cron (it catches up on missed runs after
sleep). Save this as `~/Library/LaunchAgents/dev.fond.backup.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>dev.fond.backup</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/fond</string>
    <string>backup</string>
    <string>create</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>FOND_DATA_DIR</key>
    <string>/Users/you/fond</string>
  </dict>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Hour</key><integer>2</integer>
    <key>Minute</key><integer>30</integer>
  </dict>
  <key>StandardOutPath</key>
  <string>/Users/you/fond/backups/backup.log</string>
  <key>StandardErrorPath</key>
  <string>/Users/you/fond/backups/backup.log</string>
</dict>
</plist>
```

Then load it (once):

```bash
launchctl load ~/Library/LaunchAgents/dev.fond.backup.plist
```

On recent macOS you can use the modern form instead:
`launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/dev.fond.backup.plist`.
Unload with `launchctl unload …` (or `bootout`).

## Windows Task Scheduler

fond runs on Windows too. Register a daily task from an elevated PowerShell —
adjust the path to `fond.exe`:

```powershell
schtasks /Create /SC DAILY /ST 02:30 /TN "fond backup" `
  /TR "'C:\Program Files\fond\fond.exe' backup create"
```

To set the data directory or write to another drive, wrap the call in a tiny
script (e.g. `fond-backup.ps1`) and schedule *that*:

```powershell
# fond-backup.ps1
$env:FOND_DATA_DIR = "C:\Users\you\fond"
& 'C:\Program Files\fond\fond.exe' backup create --dest ("D:\fond-backups\fond-backup-{0}.fondbkp" -f (Get-Date -Format yyyyMMdd-HHmmss))
```

```powershell
schtasks /Create /SC DAILY /ST 02:30 /TN "fond backup" `
  /TR "powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\you\fond-backup.ps1"
```

## Rotation and retention

Because each run writes a new timestamped file, backups **accumulate** — fond will
not delete old ones for you. Prune on a schedule so the folder doesn't grow
forever. Keep enough history to survive a mistake you don't notice immediately
(a corrupted recipe, an accidental `fond rm`), not just the most recent copy.

Delete archives older than 30 days:

```bash
# Linux / macOS
find /home/you/fond/backups -name 'fond-backup-*.fondbkp' -mtime +30 -delete
```

```powershell
# Windows (PowerShell)
Get-ChildItem D:\fond-backups\fond-backup-*.fondbkp |
  Where-Object LastWriteTime -lt (Get-Date).AddDays(-30) |
  Remove-Item
```

Run rotation as its own scheduled job (a second cron line / launchd agent / task),
or append it to your backup wrapper script.

## Keep an off-device copy (3-2-1)

**A backup that lives on the same disk as your data is not a backup.** If that
drive dies, or the machine is lost or stolen, both the originals and the archives
go with it. Follow the classic **3-2-1** rule:

- **3** copies of your data,
- on **2** different kinds of media,
- with **1** copy **off-device** (a different machine, an external drive you
  unplug, or object storage you control).

Practical patterns:

- Point `--dest` at a mounted external drive or a synced folder so archives land
  off the primary disk directly.
- Or keep the default local `backups/` folder and add a second scheduled step that
  copies the newest archive elsewhere (`rsync`, `rclone`, `robocopy`, or your file
  sync tool). Note this is different from [syncing your recipes](./syncing.md):
  there you replicate the live `.cook` files; here you ship sealed, point-in-time
  archives.

## Encrypted scheduled backups

`fond backup create --encrypt` seals the archive with XChaCha20-Poly1305 using the
same household key as `fond overlay --encrypt` — good when backups sit on untrusted
media. But a scheduler runs **non-interactively**, so the key has to arrive without
a prompt. Choose honestly:

- **Passphrase mode** (`--encrypt --passphrase`, Argon2id) reads the passphrase
  from `FOND_OVERLAY_PASSPHRASE`. Provide it in the job's environment — never on
  the command line, where it would show up in process listings:

  ```cron
  FOND_DATA_DIR=/home/you/fond
  FOND_OVERLAY_PASSPHRASE=correct-horse-battery-staple
  30 2 * * * /usr/local/bin/fond backup create --encrypt --passphrase >> /home/you/fond/backups/backup.log 2>&1
  ```

  Protect that value — a readable crontab or plist with the passphrase in it
  defeats the encryption. Prefer a root-only file, your scheduler's secret store,
  or an environment injected at runtime.

- **Keychain mode** (`--encrypt`, the default key source) needs no passphrase — but
  only works when the OS keychain is **unlocked**. In headless, cron, or system-
  service contexts the login keychain is often **locked**, and the job will fail
  closed rather than back up silently unprotected. If you schedule keychain-mode
  encryption, run it as a logged-in user with an unlocked keychain and verify the
  first scheduled run actually produced an archive.

- **Plaintext** (the default, no `--encrypt`) needs no key at all. If your backup
  destination is already protected — full-disk-encrypted external media you
  control — plaintext scheduled backups are the simplest reliable choice.

See [What fond protects](./security.md) for the full encryption model and key
handling.

## Prove your restores

An untested backup is a hope, not a backup. fond gives you a drill that reads an
archive and confirms it would restore — checking authentication and every
per-file hash — **without writing anything**:

```bash
fond backup verify /path/to/fond-backup-<timestamp>.fondbkp
```

Add `--against-source` to also diff the archive against your live data directory
and see what has changed since it was taken:

```bash
fond backup verify /path/to/fond-backup-<timestamp>.fondbkp --against-source
```

Run `verify` periodically against your newest archive — schedule it as its own job
if you like — so a bad drive or a mis-configured encrypted job is caught *before*
you actually need to restore. When the day comes, restore is one command (it
verifies first and fails closed on tamper, then reindexes):

```bash
fond backup restore /path/to/fond-backup-<timestamp>.fondbkp
```

See [`fond backup`](./cli-reference.md#fond-backup) for the full create / restore /
verify reference.
