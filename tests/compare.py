# tests/compare_trades.py
import pandas as pd

py = pd.read_csv("py_trades.csv")
rs = pd.read_csv("rs_trades.csv")

py = py.sort_values(["EntryBar", "ExitBar"]).reset_index(drop=True)
rs = rs.sort_values(["EntryBar", "ExitBar"]).reset_index(drop=True)

merged = py.merge(rs, on="EntryBar", how="outer", suffixes=("_py", "_rs"), indicator=True)

# 1. Trades whose EntryBar exists on only one side (real signal divergence)
only_one_side = merged[merged["_merge"] != "both"]
print(f"=== {len(only_one_side)} trades with EntryBar only on one side ===")
print(only_one_side.head(20).to_string())

# 2. Trades present on both sides but with mismatched fields beyond float tolerance
both = merged[merged["_merge"] == "both"].copy()


def mismatch(row, col, tol=1e-6):
    a, b = row[f"{col}_py"], row[f"{col}_rs"]
    return abs(a - b) > tol if pd.notna(a) and pd.notna(b) else a != b


for col in ["Size", "ExitBar", "EntryPrice", "ExitPrice"]:
    diffs = both[both.apply(lambda r: mismatch(r, col), axis=1)]
    if not diffs.empty:
        print(f"\n=== {len(diffs)} trades differ in {col} (first 10) ===")
        print(diffs[["EntryBar", f"{col}_py", f"{col}_rs"]].head(10).to_string())
