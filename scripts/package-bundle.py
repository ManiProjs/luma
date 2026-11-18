#!/usr/bin/env python3
"""Assemble the platform-independent layout consumed by the Luma installer."""

from __future__ import annotations

import argparse
import json
import os
import tarfile
import tempfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--core", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    core = args.core.resolve()
    cli = args.cli.resolve()
    for path in (core, cli, Path("LICENSE").resolve()):
        if not path.is_file():
            raise SystemExit(f"required release input is missing: {path}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="luma-bundle-") as temporary:
        root = Path(temporary) / "luma"
        (root / "bin").mkdir(parents=True)
        (root / "lib").mkdir(parents=True)
        (root / "skills").mkdir(parents=True)
        binary_name = "luma-core.exe" if args.target.endswith("windows-msvc") else "luma-core"
        (root / "bin" / binary_name).write_bytes(core.read_bytes())
        (root / "lib" / "luma-cli.js").write_bytes(cli.read_bytes())
        (root / "LICENSE").write_bytes(Path("LICENSE").read_bytes())
        import shutil

        for skill in Path("skills").iterdir():
            if skill.is_dir():
                shutil.copytree(skill, root / "skills" / skill.name)
        (root / "bundle.json").write_text(
            json.dumps(
                {
                    "name": "Luma",
                    "version": args.version,
                    "target": args.target,
                    "components": ["core", "ink-cli", "embedded-dashboard"],
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        if not args.target.endswith("windows-msvc"):
            os.chmod(root / "bin" / binary_name, 0o755)
        with tarfile.open(args.output, "w:gz") as archive:
            archive.add(root, arcname="luma")


if __name__ == "__main__":
    main()
