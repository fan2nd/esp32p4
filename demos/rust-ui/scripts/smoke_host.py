"""Optional Python stdlib smoke test; run after cargo build --features host."""
import pathlib
import subprocess
import sys
import time
import urllib.request

root = pathlib.Path(__file__).resolve().parents[1]
binary = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else root / "target/debug/demo"
process = subprocess.Popen([str(binary)], stdout=subprocess.DEVNULL)
try:
    time.sleep(0.2)
    def get(path):
        return urllib.request.urlopen("http://127.0.0.1:7878" + path, timeout=3).read()
    initial = get("/frame")
    assert len(initial) == 320 * 240 * 4
    assert b"canvas" in get("/")
    get("/event?kind=down&x=40&y=100")
    changed = get("/event?kind=move&x=280&y=100")
    get("/event?kind=up&x=280&y=100")
    assert changed != initial
    get("/event?kind=keydown&key=next")
    get("/event?kind=keydown&key=activate")
    get("/event?kind=keyup&key=activate")
    assert get("/event?kind=keydown&key=next") == initial
    assert get("/event?kind=keydown&key=right") != initial
    print("PASS: localhost frame/event roundtrip, slider, focus, reset, arrow")
finally:
    process.terminate()
    process.wait()
