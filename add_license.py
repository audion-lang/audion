#!/usr/bin/env python3
#
# add_license.py
# Copyright (C) 2026 YOUR_NAME_HERE
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

# run this script every year :)

import re
import urllib.request
from pathlib import Path
from datetime import datetime
today = datetime.today()

# --- TEMPLATE: edit these ---
COPYRIGHT_HOLDER = "Aleksandr Bogdanov"
COPYRIGHT_YEARS = "2025-" + str(today.year)
# ----------------------------

PROJECT_ROOT = Path(__file__).parent

STANDARD_HEADER = f"""\
// Copyright (C) {COPYRIGHT_YEARS} {COPYRIGHT_HOLDER}
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
//
"""

EXSTROM_FILES = {"melodies.rs", "sequences.rs"}

def fetch_license():
    print("Fetching GPLv3 license text...")
    url = "https://www.gnu.org/licenses/gpl-3.0.txt"
    text = urllib.request.urlopen(url).read().decode("utf-8")
    license_path = PROJECT_ROOT / "LICENSE"
    license_path.write_text(text)
    print(f"  Saved {license_path}")

def add_header(path: Path, header: str):
    content = path.read_text()

    if "GNU General Public License" in content:
        print(f"  SKIP (already licensed): {path.name}")
        return

    # Strip existing block comment at top of file
    stripped = re.sub(r"^/\*.*?\*/\s*", "", content, count=1, flags=re.DOTALL)

    path.write_text(header + "\n" + stripped)
    print(f"  ADD  header: {path.name}")


def main():
    fetch_license()
    for rs_file in sorted((PROJECT_ROOT / "src").glob("*.rs")):
        print("\nAdding license headers to source file " + rs_file.name + " ...")
        if rs_file.name in EXSTROM_FILES:
            continue
        add_header(rs_file, STANDARD_HEADER)

    print("\nDone! Review changes with: git diff")


if __name__ == "__main__":
    main()
