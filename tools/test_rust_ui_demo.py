#!/usr/bin/env python3
"""Test the firmware bin's page on the host, without another demo crate."""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    cargo = os.environ.get("CARGO", "cargo")
    rustc = os.environ.get("RUSTC", "rustc")
    info = subprocess.check_output([rustc, "-vV"], text=True)
    host = next(line.split(": ", 1)[1] for line in info.splitlines() if line.startswith("host: "))
    built = subprocess.run(
        [cargo, "build", "--locked", "--manifest-path", str(ROOT / "3rd/tiny-ui/Cargo.toml"),
         "--target", host, "--message-format=json"],
        cwd=ROOT, text=True, stdout=subprocess.PIPE, check=True,
    )
    libraries = {}
    for line in built.stdout.splitlines():
        record = json.loads(line)
        if record.get("reason") == "compiler-message":
            print(record["message"].get("rendered", ""), file=sys.stderr, end="")
        if record.get("reason") == "compiler-artifact":
            name = record["target"]["name"]
            if name in ("tiny_ui", "embedded_graphics"):
                libraries[name] = next(Path(p) for p in record["filenames"] if p.endswith(".rlib"))
    deps = libraries["embedded_graphics"].parent
    output = deps.parent / ("rust_ui_demo_tests.exe" if os.name == "nt" else "rust_ui_demo_tests")
    command = [rustc, "--test", "--edition=2021", "--target", host,
               str(ROOT / "tests/rust_ui_demo.rs"), "-L", f"dependency={deps}", "-o", str(output)]
    for name, path in libraries.items():
        command += ["--extern", f"{name}={path}"]
    subprocess.run(command, cwd=ROOT, check=True)
    subprocess.run([str(output), *sys.argv[1:]], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
