#!/usr/bin/env python3
"""Fetch the extended benchmark corpus from its registry fragments.

The extended corpus (~76 documents, ~165 MB) is registry-pinned rather than
committed: each entry records the direct URL, SHA-256, size, license note, and
page count. This script downloads every entry, verifies the hash, and reports
drift or dead links without failing the whole fetch. Documents marked
"redistributable": false are still fetchable for local testing; they are never
committed.

Usage:
  python3 tools/fetch_extended_corpus.py [--registry-dir test/extended/registry]
"""

import argparse
import hashlib
import json
import os
import sys
import urllib.request

USER_AGENT = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/124.0 Safari/537.36"
)


def fetch(url, target):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=180) as response:
        data = response.read()
    os.makedirs(os.path.dirname(target), exist_ok=True)
    with open(target, "wb") as handle:
        handle.write(data)
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry-dir", default="test/extended/registry")
    args = parser.parse_args()

    fragments = [
        os.path.join(args.registry_dir, name)
        for name in sorted(os.listdir(args.registry_dir))
        if name.startswith("registry-fragment") and name.endswith(".json")
    ]
    if not fragments:
        print(f"no registry fragments under {args.registry_dir}", file=sys.stderr)
        return 1

    ok = drifted = failed = skipped = 0
    for fragment in fragments:
        registry = json.load(open(fragment))
        for document in registry["documents"]:
            target = os.path.join(args.registry_dir, document["path"])
            if os.path.exists(target):
                digest = hashlib.sha256(open(target, "rb").read()).hexdigest()
                if digest == document["sha256"]:
                    ok += 1
                    continue
            try:
                data = fetch(document["direct_url"], target)
            except Exception as error:
                print(f"FAILED  {document['path']}: {error}")
                failed += 1
                continue
            digest = hashlib.sha256(data).hexdigest()
            if digest != document["sha256"]:
                print(f"DRIFTED {document['path']}: upstream bytes changed")
                drifted += 1
            else:
                ok += 1

    print(f"ok={ok} drifted={drifted} failed={failed} skipped={skipped}")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
