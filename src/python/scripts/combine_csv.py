# /// script
# requires-python = ">=3.13"
# dependencies = ["polars>=1.27.0"]
# ///

from __future__ import annotations

import argparse
from pathlib import Path

import polars as pl


REQUIRED_COLUMNS = {"Digikey PN", "Qty"}


def load_and_normalize(path: Path) -> pl.DataFrame:
    df = pl.read_csv(path)

    missing = REQUIRED_COLUMNS - set(df.columns)
    if missing:
        raise ValueError(f"{path} is missing required columns: {sorted(missing)}")

    return (
        df.with_columns(
            pl.col("Digikey PN").cast(pl.Utf8).str.strip_chars().alias("Digikey PN"),
            pl.col("Qty").cast(pl.Int64).alias("Qty"),
        )
        .filter(pl.col("Digikey PN").is_not_null() & (pl.col("Digikey PN") != ""))
    )


def merge_boms(path_a: Path, path_b: Path) -> pl.DataFrame:
    df_a = load_and_normalize(path_a)
    df_b = load_and_normalize(path_b)

    merged = pl.concat([df_a, df_b], how="diagonal_relaxed")

    keep_if_present = ["Value", "Footprint", "Datasheet"]
    aggs: list[pl.Expr] = [pl.col("Qty").sum().alias("Qty")]

    for col in keep_if_present:
        if col in merged.columns:
            aggs.append(pl.col(col).drop_nulls().first().alias(col))

    result = merged.group_by("Digikey PN").agg(aggs)

    output_columns = ["Qty"]
    for col in keep_if_present:
        if col in result.columns:
            output_columns.append(col)
    output_columns.append("Digikey PN")

    return result.select(output_columns).sort("Digikey PN")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Merge two BOM CSVs by DigiKey part number and sum quantities."
    )
    parser.add_argument("csv_a", type=Path, help="Path to the first CSV file")
    parser.add_argument("csv_b", type=Path, help="Path to the second CSV file")
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=Path("combined.csv"),
        help="Path to the output CSV file (default: combined.csv)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()

    result = merge_boms(args.csv_a, args.csv_b)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    result.write_csv(args.output)


if __name__ == "__main__":
    main()