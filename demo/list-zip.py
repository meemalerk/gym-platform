"""Inspect a built package without needing `unzip` installed.

    python3 demo/list-zip.py <file.zip> top     what the recipient sees at the root
    python3 demo/list-zip.py <file.zip> leaks   anything that must never ship
"""

import sys
import zipfile

if len(sys.argv) != 3:
    sys.exit("usage: list-zip.py <file.zip> top|leaks")

path, mode = sys.argv[1], sys.argv[2]
names = zipfile.ZipFile(path).namelist()

if mode == "top":
    # One level below the archive's single top folder — exactly the window a
    # person sees after double-clicking the zip.
    top = names[0].split("/")[0]
    seen = []
    for n in names:
        parts = n[len(top) + 1 :].split("/")
        if not parts or not parts[0]:
            continue
        entry = parts[0] + ("/" if len(parts) > 1 else "")
        if entry not in seen:
            seen.append(entry)
    for entry in sorted(seen, key=lambda s: (s.endswith("/"), s.lower())):
        print(f"    {entry}")

elif mode == "leaks":
    bad = [
        n
        for n in names
        if n.endswith("/.env")
        or n.endswith(".env")
        or "/target/" in n
        or "/node_modules/" in n
    ]
    for n in bad[:5]:
        print(n)
