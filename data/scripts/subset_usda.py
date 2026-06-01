#!/usr/bin/env python3
"""
subset_usda.py — Download & subset USDA FoodData Central for fond.

Downloads Foundation Foods and SR Legacy CSV archives, extracts common
cooking ingredients, and produces a compact per-100g nutrition subset.

Output: data/usda/usda_nutrition_subset.csv

Usage:
    python data/scripts/subset_usda.py

The raw ZIP files are cached in data/usda/raw/ (gitignored).
"""

import csv
import gzip
import hashlib
import io
import urllib.request
import zipfile
from collections import defaultdict
from pathlib import Path

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
RAW_DIR = REPO_ROOT / "data" / "usda" / "raw"
OUTPUT_DIR = REPO_ROOT / "data" / "usda"
OUTPUT_CSV = OUTPUT_DIR / "usda_nutrition_subset.csv"

DOWNLOADS = {
    "foundation": {
        "url": "https://fdc.nal.usda.gov/fdc-datasets/FoodData_Central_foundation_food_csv_2024-10-31.zip",
        "label": "Foundation Foods (Oct 2024)",
    },
    "sr_legacy": {
        "url": "https://fdc.nal.usda.gov/fdc-datasets/FoodData_Central_sr_legacy_food_csv_2018-04.zip",
        "label": "SR Legacy (Apr 2018, final release)",
    },
}

# Nutrient IDs (USDA FoodData Central)
NUTRIENT_IDS = {
    1008: "kcal",
    1003: "protein_g",
    1004: "fat_g",
    1005: "carb_g",
    1079: "fiber_g",
    2000: "sugar_g",       # Total Sugars including NLEA
    1093: "sodium_mg",
}
# Fallback: if nutrient 2000 is missing, try 1063 (Sugars, Total)
SUGAR_FALLBACK_ID = 1063
# Fallback energy IDs: Foundation Foods often use Atwater factors
# instead of nutrient 1008. Try 2047 (General) then 2048 (Specific).
ENERGY_FALLBACK_IDS = [2047, 2048]

# USDA food categories to EXCLUDE (not cooking ingredients)
EXCLUDED_CATEGORIES = {
    "Baby Foods",
    "Fast Foods",
    "Restaurant Foods",
    "Meals, Entrees, and Side Dishes",
    "American Indian/Alaska Native Foods",
    "Branded Food Products Database",
    "Quality Control Materials",
}

# For Foundation Foods, only keep aggregated items (not raw samples)
FOUNDATION_KEEP_TYPES = {"foundation_food"}

# Description keywords that signal non-ingredient items (case-insensitive)
EXCLUDED_DESCRIPTION_KEYWORDS = [
    "babyfood",
    "baby food",
    "infant formula",
    "school lunch",
    "hospital",
    "supplement",
    "nutrition bar",
    "protein bar",
    "energy bar",
    "meal replacement",
    "military",
    "mre ",
    "usda commodity",
    "not further specified",
]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def download_file(url: str, dest: Path) -> None:
    """Download a file with progress reporting."""
    if dest.exists():
        print(f"  [cached] {dest.name}")
        return
    print(f"  Downloading {url} ...")
    req = urllib.request.Request(url, headers={"User-Agent": "fond/0.3 (recipe-app)"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        total = int(resp.headers.get("Content-Length", 0))
        downloaded = 0
        chunk_size = 1 << 16  # 64 KB
        with open(dest, "wb") as f:
            while True:
                chunk = resp.read(chunk_size)
                if not chunk:
                    break
                f.write(chunk)
                downloaded += len(chunk)
                if total:
                    pct = downloaded * 100 // total
                    print(f"\r  {downloaded:,} / {total:,} bytes ({pct}%)", end="", flush=True)
        print()
    size_mb = dest.stat().st_size / (1 << 20)
    print(f"  Saved {dest.name} ({size_mb:.1f} MB)")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def read_csv_from_zip(zip_path: Path, csv_name: str) -> list[dict]:
    """Read a CSV file from inside a ZIP archive (exact filename match)."""
    with zipfile.ZipFile(zip_path) as zf:
        # Match exact filename, not suffix (avoid input_food.csv matching food.csv)
        matches = [n for n in zf.namelist() if n.split("/")[-1] == csv_name]
        if not matches:
            raise FileNotFoundError(
                f"{csv_name} not found in {zip_path.name}. "
                f"Contents: {zf.namelist()[:20]}"
            )
        target = matches[0]
        with zf.open(target) as f:
            text = io.TextIOWrapper(f, encoding="utf-8", errors="replace")
            reader = csv.DictReader(text)
            return list(reader)


def normalize_description(desc: str) -> str:
    """Normalize a USDA description for display."""
    return desc.strip()


def is_excluded_by_keywords(desc: str) -> bool:
    """Check if a food description matches exclusion keywords."""
    lower = desc.lower()
    return any(kw in lower for kw in EXCLUDED_DESCRIPTION_KEYWORDS)


# ---------------------------------------------------------------------------
# Main pipeline
# ---------------------------------------------------------------------------

def process_dataset(zip_path: Path, data_type: str) -> list[dict]:
    """Process a single USDA dataset ZIP into nutrition rows."""
    print(f"\nProcessing {data_type} from {zip_path.name}...")

    # Read CSVs
    foods = read_csv_from_zip(zip_path, "food.csv")
    nutrients_raw = read_csv_from_zip(zip_path, "food_nutrient.csv")
    nutrient_defs = read_csv_from_zip(zip_path, "nutrient.csv")
    categories = read_csv_from_zip(zip_path, "food_category.csv")

    print(f"  Raw foods: {len(foods)}")
    print(f"  Raw nutrient rows: {len(nutrients_raw)}")
    print(f"  Nutrient definitions: {len(nutrient_defs)}")
    print(f"  Categories: {len(categories)}")

    # Build category lookup
    cat_map = {}
    for c in categories:
        cat_id = c.get("id", "")
        cat_desc = c.get("description", "")
        if cat_id and cat_desc:
            cat_map[cat_id] = cat_desc

    # Filter foods by category and description
    valid_foods = {}
    excluded_cat_count = 0
    excluded_kw_count = 0
    excluded_dt_count = 0

    for f in foods:
        fdc_id = f.get("fdc_id", "")
        desc = f.get("description", "")
        cat_id = f.get("food_category_id", "")
        food_data_type = f.get("data_type", "")

        if not fdc_id or not desc:
            continue

        # For Foundation Foods, only keep aggregated foundation_food entries
        if data_type == "foundation" and food_data_type not in FOUNDATION_KEEP_TYPES:
            excluded_dt_count += 1
            continue

        category = cat_map.get(cat_id, "Unknown")

        # Exclude by category
        if category in EXCLUDED_CATEGORIES:
            excluded_cat_count += 1
            continue

        # Exclude by description keywords
        if is_excluded_by_keywords(desc):
            excluded_kw_count += 1
            continue

        valid_foods[fdc_id] = {
            "fdc_id": int(fdc_id),
            "description": normalize_description(desc),
            "category": category,
            "data_type": data_type,
        }

    print(f"  Excluded by data_type: {excluded_dt_count}")
    print(f"  Excluded by category: {excluded_cat_count}")
    print(f"  Excluded by keywords: {excluded_kw_count}")
    print(f"  Valid foods: {len(valid_foods)}")

    # Build nutrient lookup: fdc_id -> {nutrient_id: amount}
    nutrient_lookup: dict[str, dict[int, float]] = defaultdict(dict)
    wanted_ids = set(NUTRIENT_IDS.keys()) | {SUGAR_FALLBACK_ID} | set(ENERGY_FALLBACK_IDS)

    for nr in nutrients_raw:
        fdc_id = nr.get("fdc_id", "")
        nutrient_id_str = nr.get("nutrient_id", "")
        amount_str = nr.get("amount", "")

        if not fdc_id or not nutrient_id_str:
            continue
        if fdc_id not in valid_foods:
            continue

        try:
            nutrient_id = int(nutrient_id_str)
        except ValueError:
            continue

        if nutrient_id not in wanted_ids:
            continue

        try:
            amount = float(amount_str) if amount_str else None
        except ValueError:
            amount = None

        if amount is not None:
            # If duplicate nutrient per food, keep the first one
            if nutrient_id not in nutrient_lookup[fdc_id]:
                nutrient_lookup[fdc_id][nutrient_id] = amount

    # Build output rows
    results = []
    for fdc_id, food in valid_foods.items():
        nutrients = nutrient_lookup.get(fdc_id, {})

        row = {
            "fdc_id": food["fdc_id"],
            "description": food["description"],
            "category": food["category"],
            "data_type": food["data_type"],
        }

        for nid, col_name in NUTRIENT_IDS.items():
            val = nutrients.get(nid)
            # Energy fallback: try Atwater General (2047), then Specific (2048)
            if val is None and nid == 1008:
                for fallback_id in ENERGY_FALLBACK_IDS:
                    val = nutrients.get(fallback_id)
                    if val is not None:
                        break
            # Sugar fallback: try 1063 (Sugars, Total)
            if val is None and nid == 2000:
                val = nutrients.get(SUGAR_FALLBACK_ID)
            row[col_name] = round(val, 2) if val is not None else ""
            
        # Only include if at least kcal is present
        if row["kcal"] != "":
            results.append(row)

    print(f"  Foods with nutrition data: {len(results)}")
    return results


def main():
    RAW_DIR.mkdir(parents=True, exist_ok=True)
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    # Step 1: Download
    print("=" * 60)
    print("Step 1: Download USDA FoodData Central datasets")
    print("=" * 60)

    zip_paths = {}
    for key, info in DOWNLOADS.items():
        filename = info["url"].rsplit("/", 1)[-1]
        dest = RAW_DIR / filename
        print(f"\n{info['label']}:")
        download_file(info["url"], dest)
        zip_paths[key] = dest

    # Record checksums
    print("\nChecksums (SHA-256):")
    for key, path in zip_paths.items():
        checksum = sha256_file(path)
        size_mb = path.stat().st_size / (1 << 20)
        print(f"  {path.name}: {checksum[:16]}... ({size_mb:.1f} MB)")

    # Step 2: Process
    print("\n" + "=" * 60)
    print("Step 2: Extract & subset")
    print("=" * 60)

    all_rows = []
    for key, path in zip_paths.items():
        rows = process_dataset(path, key)
        all_rows.extend(rows)

    # Step 3: Sort and write
    print("\n" + "=" * 60)
    print("Step 3: Write output CSV")
    print("=" * 60)

    # Sort by category then description
    all_rows.sort(key=lambda r: (r["category"], r["description"]))

    columns = [
        "fdc_id", "description", "category", "data_type",
        "kcal", "protein_g", "fat_g", "carb_g",
        "fiber_g", "sugar_g", "sodium_mg",
    ]

    with open(OUTPUT_CSV, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=columns)
        writer.writeheader()
        writer.writerows(all_rows)

    # Step 4: Statistics
    print("\n" + "=" * 60)
    print("Step 4: Summary statistics")
    print("=" * 60)

    raw_size = OUTPUT_CSV.stat().st_size
    print(f"\nTotal foods in subset: {len(all_rows)}")
    print(f"Raw CSV size: {raw_size:,} bytes ({raw_size / 1024:.1f} KB)")

    # Gzip size
    with open(OUTPUT_CSV, "rb") as f_in:
        compressed = gzip.compress(f_in.read(), compresslevel=9)
    gz_size = len(compressed)
    print(f"Gzipped size: {gz_size:,} bytes ({gz_size / 1024:.1f} KB)")
    print(f"Compression ratio: {gz_size / raw_size:.1%}")

    # Category breakdown
    cat_counts: dict[str, int] = defaultdict(int)
    dt_counts: dict[str, int] = defaultdict(int)
    for r in all_rows:
        cat_counts[r["category"]] += 1
        dt_counts[r["data_type"]] += 1

    print(f"\nBy data source:")
    for dt, count in sorted(dt_counts.items()):
        print(f"  {dt}: {count}")

    print(f"\nBy category (top 20):")
    for cat, count in sorted(cat_counts.items(), key=lambda x: -x[1])[:20]:
        print(f"  {cat}: {count}")

    # Nutrient coverage
    print(f"\nNutrient coverage (% of foods with data):")
    for col in ["kcal", "protein_g", "fat_g", "carb_g", "fiber_g", "sugar_g", "sodium_mg"]:
        has_data = sum(1 for r in all_rows if r[col] != "")
        pct = has_data * 100 / len(all_rows) if all_rows else 0
        print(f"  {col}: {has_data}/{len(all_rows)} ({pct:.1f}%)")

    # Size assessment
    print(f"\n{'=' * 60}")
    print("Size assessment for binary embedding:")
    print(f"{'=' * 60}")
    if gz_size < 500 * 1024:
        print(f"  ✅ Compressed size ({gz_size / 1024:.0f} KB) is well under 500 KB.")
        print(f"  Suitable for embedding via include_bytes! + runtime decompression,")
        print(f"  or loading into SQLite at fond init time.")
    elif gz_size < 2 * 1024 * 1024:
        print(f"  ⚠️  Compressed size ({gz_size / 1024:.0f} KB) is moderate.")
        print(f"  Consider trimming further or using lazy loading.")
    else:
        print(f"  ❌ Compressed size ({gz_size / 1024:.0f} KB) is too large for embedding.")
        print(f"  Needs further subsetting or a different approach.")

    print(f"\nOutput written to: {OUTPUT_CSV}")
    print("Done.")


if __name__ == "__main__":
    main()
