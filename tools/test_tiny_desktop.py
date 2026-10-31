#!/usr/bin/env python3
"""Host regressions for the tiny desktop page, without introducing a demo crate."""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    cargo = os.environ.get("CARGO", "cargo")
    rustc = os.environ.get("RUSTC", "rustc")
    version = subprocess.check_output([rustc, "-vV"], text=True)
    host = next(line[6:] for line in version.splitlines() if line.startswith("host: "))
    libraries = {}
    search = set()
    for package in ("tiny-ui", "keyboard-host-tests"):
        manifest = ROOT / "3rd" / package / "Cargo.toml"
        result = subprocess.run(
            [cargo, "build", "--locked", "--manifest-path", str(manifest),
             "--target", host, "--message-format=json"],
            cwd=ROOT, text=True, stdout=subprocess.PIPE, check=True,
        )
        for line in result.stdout.splitlines():
            value = json.loads(line)
            if value.get("reason") == "compiler-message":
                print(value["message"].get("rendered", ""), end="", file=sys.stderr)
            if value.get("reason") == "compiler-artifact":
                paths = [Path(p) for p in value["filenames"] if p.endswith(".rlib")]
                if paths:
                    search.add(paths[0].parent)
                    name = value["target"]["name"]
                    if name in ("tiny_ui", "embedded_graphics", "heapless"):
                        libraries[name] = paths[0]
    filename = "tiny-desktop-tests.exe" if os.name == "nt" else "tiny-desktop-tests"
    output = ROOT / "target" / filename
    output.parent.mkdir(parents=True, exist_ok=True)
    args = [rustc, "--test", "--edition=2024", "--target", host,
            str(ROOT / "tests/tiny_desktop.rs"), "-o", str(output)]
    for path in sorted(search):
        args += ["-L", f"dependency={path}"]
    for name, path in libraries.items():
        args += ["--extern", f"{name}={path}"]
    subprocess.run(args, cwd=ROOT, check=True)
    subprocess.run([str(output), *sys.argv[1:]], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
