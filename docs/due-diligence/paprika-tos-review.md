# Due Diligence: Paprika ToS / Personal-Use Stance for Import

**Date**: 2025-05-30
**Status**: Reviewed — No blockers identified
**Related Issue**: [#15](https://github.com/kafkade/fond/issues/15)

## Summary

fond's Paprika import and export features operate on the user's own recipe data
files (`.paprikarecipes` / `.paprikarecipe`). After reviewing Paprika's Terms of
Service, Privacy Policy, and publicly documented features, **no provision
prohibits a user from using their own exported data with third-party tools**.
fond's approach — parsing user-owned files on the user's local machine — is
consistent with Paprika's design philosophy and terms.

## Documents Reviewed

| Document | URL | Last Modified |
|----------|-----|---------------|
| Terms of Use and EULA | <https://paprikaapp.com/terms/> | Current as of 2025 |
| Privacy Policy | <https://paprikaapp.com/privacy/> | June 28, 2019 |
| App Store descriptions | Apple App Store, public listings | Current |

## Key Findings

### 1. User Data Ownership

Paprika's ToS (Section 9, "User Content") explicitly states:

> "You retain any and all of your rights to any User Content you submit, post
> or display on or through the Software and you are responsible for protecting
> those rights."

**Analysis**: Recipes entered by the user are "User Content" under Paprika's
terms. Users retain full rights to their own recipes. There is no assignment or
exclusive license granted to Hindsight Labs over user-entered recipe data.

### 2. Export as a Built-In Feature

Paprika provides built-in export functionality across all platforms (macOS, iOS,
Windows, Android):

- **Recipe export** to `.paprikarecipes` files (ZIP archives of gzip-compressed
  JSON)
- **Full database backup** to `.paprikabackup` files
- **Sharing** via email, AirDrop, and other system sharing mechanisms
- **Import from other apps** (MacGourmet, MasterCook, Living Cookbook, etc.)

**Analysis**: Paprika actively facilitates data portability. The export feature
is not gated behind any premium tier or special agreement. The company
intentionally gives users the ability to extract their data.

### 3. License Restrictions (Section 2)

The license restrictions prohibit:

- Copying the Software itself
- Reverse engineering the Software
- Redistributing the Software
- Creating derivative works of the Software

**Analysis**: These restrictions apply to the **Software** (the Paprika
application and its code), not to the **user's data**. fond does not copy,
reverse-engineer, or redistribute Paprika's software. fond reads data files that
the user has already exported from Paprika using Paprika's own built-in export
functionality.

### 4. File Format

The `.paprikarecipes` format is:

- A standard ZIP archive (identifiable via magic bytes)
- Containing gzip-compressed JSON files
- Using standard, well-known compression and serialization formats
- With no encryption, DRM, or copy-protection mechanisms

**Analysis**: The format uses only standard, open technologies. There is no
technological protection measure (TPM) to circumvent, so DMCA anti-circumvention
provisions (17 U.S.C. § 1201) do not apply. Parsing this format is analogous to
opening any ZIP or JSON file.

### 5. Prohibited Uses (Section 3)

Relevant prohibitions include not using the Software:

- "In any way that violates the personal or proprietary rights of any third
  party"
- "In any way that violates the contractual rights of another party"

**Analysis**: fond does not interact with Paprika's software or servers at all.
It processes files that already exist on the user's local filesystem. The user
created these files using Paprika's own export feature.

### 6. No API or Network Interaction

fond's Paprika import:

- Does **not** access Paprika's servers or cloud sync API
- Does **not** authenticate with any Paprika service
- Does **not** scrape or crawl any Paprika-owned website
- Operates entirely on local files the user provides as a CLI argument

**Analysis**: There is no terms-of-service relationship between fond and Paprika.
fond is a file-processing tool that the user runs on their own data.

## Risk Assessment

| Risk | Likelihood | Severity | Mitigation |
|------|-----------|----------|------------|
| ToS prohibits parsing exported files | Very Low | Medium | ToS covers the Software, not user data files; export is a built-in feature |
| Format treated as trade secret | Very Low | Low | Standard ZIP+gzip+JSON; no proprietary encoding |
| DMCA anti-circumvention claim | Negligible | High | No encryption or TPM to circumvent |
| User imports copyrighted recipes | Low | Low | User's responsibility (Paprika ToS §9 same); fond adds no content |

## Precedent

Parsing user-exported data from one application into another is a well-
established practice in the software ecosystem:

- **Password managers** routinely import/export between 1Password, LastPass,
  Bitwarden, etc.
- **Note-taking apps** import between Evernote, Notion, Obsidian, etc.
- **Recipe managers** — Paprika itself imports from MacGourmet, MasterCook,
  Living Cookbook, and YummySoup!
- **EU GDPR Art. 20** enshrines data portability as a right for EU users

## Conclusion

fond's Paprika import/export operates squarely within the rights retained by the
user under Paprika's own Terms of Service:

1. Users own their recipe data (ToS §9)
2. Users can export their data using Paprika's built-in features
3. fond processes those exported files locally — no Paprika software or services
   are accessed
4. The file format uses only standard, open technologies with no protection
   measures
5. Paprika itself imports from third-party recipe managers, demonstrating that
   cross-app recipe portability is an accepted industry practice

**No changes to fond's Paprika import or export implementation are needed.**

## Recommendations

1. **Documentation**: Note in user-facing docs that fond imports from
   user-exported Paprika files (already done in `docs/book/src/importing.md`)
2. **No reverse engineering**: Continue to avoid any interaction with Paprika's
   software, servers, or APIs — fond should only process files the user provides
3. **Attribution**: Continue crediting "Paprika Recipe Manager" by Hindsight Labs
   LLC when referring to the format in documentation
4. **Re-review cadence**: Re-check if Paprika updates its ToS to add data format
   restrictions (unlikely but prudent)
