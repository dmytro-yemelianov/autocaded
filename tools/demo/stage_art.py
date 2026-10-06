#!/usr/bin/env python3
"""Append the canonical artwork catalog to a staged demo Open menu."""
import hashlib
import json
from pathlib import Path
import shutil
import sys


def stage(manifest_path: Path, assets: Path) -> None:
    art = Path(__file__).resolve().parents[2] / "demo/art"
    catalog_bytes = (art / "catalog.json").read_bytes()
    catalog = json.loads(catalog_bytes)
    generated = json.loads((art / "generated/manifest.json").read_bytes())
    if generated["catalog_sha256"] != hashlib.sha256(catalog_bytes).hexdigest():
        raise ValueError("art catalog changed; regenerate art_drawings first")
    records = {entry["id"]: entry for entry in generated["entries"]}
    manifest = json.loads(manifest_path.read_text())
    names = {sample["file"].casefold() for sample in manifest["samples"]}
    staged = []
    for entry in catalog["entries"]:
        record = records[entry["id"]]
        source = (art / entry["outputs"]["dwg"]).resolve()
        if not source.is_relative_to(art.resolve()) or record["outputs"] != entry["outputs"]:
            raise ValueError("art output does not match its generated record")
        if hashlib.sha256(source.read_bytes()).hexdigest() != record["output_sha256"]["dwg"]:
            raise ValueError(f"{entry['id']}: drawing changed; regenerate first")
        if source.name.casefold() in names:
            raise ValueError(f"duplicate staged sample: {source.name}")
        names.add(source.name.casefold())
        title = record["labels"]["title"]
        if title["key"] != entry["title_key"]:
            raise ValueError("generated title does not match catalog")
        staged.append((source, {"file": source.name, "label": title["en"],
                                "label_key": entry["title_key"]}))
    for source, sample in staged:
        shutil.copyfile(source, assets / source.name)
        manifest["samples"].append(sample)
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    stage(Path(sys.argv[1]), Path(sys.argv[2]))
