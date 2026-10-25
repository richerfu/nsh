#!/usr/bin/env python3
"""Check a built NSH library's direct OH_* imports against the SDK headers."""
import argparse
import json
import re
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("library", type=Path)
parser.add_argument("--sdk", type=Path, required=True, help="OHOS_NDK_HOME")
parser.add_argument("--api", type=int, default=20)
parser.add_argument("--output", type=Path)
args = parser.parse_args()
native = args.sdk / "native"
nm = native / "llvm/bin/llvm-nm"
output = subprocess.check_output([nm, "-D", "--undefined-only", args.library], text=True)
imports = sorted(set(re.findall(r"\b(OH_\w+)\b", output)))
annotations = {}
headers = native / "sysroot/usr/include"
for header in sorted(headers.rglob("*.h")):
    source = header.read_text(errors="replace")
    for match in re.finditer(r"\b(OH_\w+)\s*(?:\(|__attribute__|;)", source):
        symbol = match[1]
        if symbol not in imports:
            continue
        start = source.rfind("/**", 0, match.start())
        end = source.rfind("*/", 0, match.start())
        if start < 0 or end < start:
            continue
        since = re.search(r"@since\s+(\d+)", source[start:end])
        if since:
            annotations.setdefault(symbol, []).append({
                "since": int(since[1]), "header": str(header.relative_to(headers))
            })
report = {
    "count": len(imports),
    "newer": {symbol: annotations[symbol] for symbol in imports
              if symbol in annotations and min(a["since"] for a in annotations[symbol]) > args.api},
    "unknown": [symbol for symbol in imports if symbol not in annotations],
    "symbols": {symbol: annotations.get(symbol, []) for symbol in imports},
}
if args.output:
    args.output.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({key: report[key] for key in ("count", "newer", "unknown")}, indent=2))
raise SystemExit(bool(report["newer"] or report["unknown"]))
