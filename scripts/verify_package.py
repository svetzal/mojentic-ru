#!/usr/bin/env python3
"""Build a crate archive, then verify its public APIs from a fresh consumer.

Run from the repository root. Dependencies and toolchain follow Cargo.toml;
no registry publication or Git mutation occurs. The generated audit record is
execution evidence, deliberately excluded from the crate being measured.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def run(*args, cwd=ROOT):
    subprocess.run(args, cwd=cwd, check=True)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    manifest_text = (ROOT / "Cargo.toml").read_text()
    package = tomllib.loads(manifest_text)["package"]
    run("cargo", "package", "--allow-dirty")
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    archive = target / "package" / f'{package["name"]}-{package["version"]}.crate'
    consumer = Path(tempfile.mkdtemp(prefix="mojentic-package-consumer-"))
    with tarfile.open(archive) as crate:
        members = crate.getnames()
        forbidden = {".foundry", ".git", ".agents", ".codex", "target", "coverage"}
        assert not any(forbidden.intersection(Path(name).parts) for name in members), members
        crate.extractall(consumer / "artifact", filter="data")
    library = consumer / "artifact" / f'{package["name"]}-{package["version"]}'
    assert library != ROOT
    for base in ("src", "tests"):
        for packaged in (library / base).rglob("*"):
            if packaged.is_file():
                original = ROOT / packaged.relative_to(library)
                assert original.is_file() and digest(packaged) == digest(original), packaged
    dependencies = manifest_text.split("[dependencies]", 1)[1].split("[features]", 1)[0]
    (consumer / "Cargo.toml").write_text(
        '[package]\nname="mojentic-packaged-consumer"\nversion="0.0.0"\nedition="2021"\n'
        '[dependencies]\nmojentic={path=' + json.dumps(str(library)) + ',features=["full"]}\n'
        + dependencies
        + '[features]\ndefault=["ollama"]\nollama=[]\nopenai=[]\nanthropic=[]\nfull=[]\n'
    )
    (consumer / "src").mkdir()
    (consumer / "src/main.rs").write_text('fn main() {}\n')
    shutil.copytree(library / "tests", consumer / "tests")
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version=1"], cwd=consumer
    ))
    dependency = next(d for d in next(p for p in metadata["packages"] if p["name"] == "mojentic-packaged-consumer")["dependencies"] if d["name"] == "mojentic")
    assert Path(dependency["path"]).resolve() == library.resolve(), dependency
    resolved = next(p for p in metadata["packages"] if p["name"] == "mojentic")
    assert Path(resolved["manifest_path"]).resolve() == library / "Cargo.toml", resolved
    assert not str(library).startswith(str(ROOT)), "consumer must use extracted artifact"
    run("cargo", "test", "--verbose", "--all-features", cwd=consumer)
    timing_contract = json.loads((library / "tests/contracts/stream_timing_v1.json").read_text())
    interruption = timing_contract["transport_interruption"]
    record = {
        "kind": "packaged-public-consumer",
        "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "archive_sha256": digest(archive),
        "archive": str(archive),
        "consumer": str(consumer),
        "resolved_library": str(library),
        "forbidden_artifacts": "absent",
        "provider_consumer_sources": "byte-identical src and tests",
        "commands": ["cargo package --allow-dirty", "cargo metadata --format-version=1", "cargo test --verbose --all-features"],
        "exit_code": 0,
        "transport_interruption": {
            "contract_set": timing_contract["version"],
            "extension": interruption["version"],
            "entrypoints": ["gateway", "broker", "session"],
            "cases_per_entrypoint": sum(len(cases) for cases in interruption["providers"].values()),
            "terminal_error": interruption["terminal_error"],
            "verification": "cargo test --verbose --all-features (includes section2_transport_interruption_* tests)",
        },
        "contracts": {p.name: digest(p) for p in sorted((library / "tests/contracts").glob("*.json"))},
        "tested_sources": {str(p.relative_to(library)): digest(p) for base in ("src", "tests") for p in sorted((library / base).rglob("*.rs"))},
    }
    (ROOT / "audit/package-readiness-3.0.0.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({key: value for key, value in record.items() if key != "tested_sources"}, indent=2))


if __name__ == "__main__":
    main()
