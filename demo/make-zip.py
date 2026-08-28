"""Zip a staged folder, preserving the executable bit.

Python's zipfile rather than the `zip` command: `zip` is not installed on a
plain Ubuntu and needs root to add, while python3 is already a hard dependency
of the seed script. One fewer thing between someone and a working package.

The executable bit matters. macOS will not let a `.command` file be
double-clicked without it, and that is the entire user experience on a Mac.
"""

import os
import stat
import sys
import zipfile

if len(sys.argv) != 4:
    sys.exit("usage: make-zip.py <stage-dir> <top-folder> <output.zip>")

stage, top, out = sys.argv[1], sys.argv[2], sys.argv[3]
root = os.path.join(stage, top)

if os.path.exists(out):
    os.remove(out)

count = 0
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for name in sorted(filenames):
            full = os.path.join(dirpath, name)
            arc = os.path.join(top, os.path.relpath(full, root))

            info = zipfile.ZipInfo.from_file(full, arc)
            mode = os.stat(full).st_mode
            # Carry the real permission bits into the archive's external attrs,
            # which is where unzip and Finder look for them.
            info.external_attr = (stat.S_IMODE(mode) & 0o7777) << 16
            info.compress_type = zipfile.ZIP_DEFLATED

            with open(full, "rb") as fh:
                z.writestr(info, fh.read())
            count += 1

print(f"{count} files")
