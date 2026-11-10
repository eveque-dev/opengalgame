"""Build portable OpenGalgame packages from a verified Windows release binary."""
from pathlib import Path
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
RELEASE = ROOT / "release"
RELEASE.mkdir(exist_ok=True)
BINARY = ROOT / "target/release/opengalgame.exe"
assert BINARY.is_file(), "Run cargo build --release --locked first"
ANDROID = ROOT / "target/android-runtime"
assert (ANDROID / "version.txt").read_text(encoding="utf-8").strip() == VERSION, "Run python tools/build_android.py first"


def fresh_directory(name):
    path = RELEASE / name
    if path.exists() and any(path.iterdir()):
        path = RELEASE / f"{name}-{time.time_ns()}"
    path.mkdir(parents=True, exist_ok=True)
    return path


def archive_files(name, files):
    path = RELEASE / f"{name}.zip"
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for relative, file in sorted(files.items()):
            info = zipfile.ZipInfo(f"{name}/{relative}", (2026, 11, 5, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, file.read_bytes(), compresslevel=9)
    with zipfile.ZipFile(path) as archive:
        assert archive.testzip() is None
        for relative, file in files.items():
            assert archive.read(f"{name}/{relative}") == file.read_bytes()
    return {"file": path.name, "bytes": path.stat().st_size, "files": len(files),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


editor_name = f"opengalgame-v{VERSION}-windows-x64"
editor = fresh_directory(editor_name)
shutil.copy2(BINARY, editor / "opengalgame.exe")
for name in ["README.md", "DESIGN.md", "LICENSE", "THIRD_PARTY.md", "一键使用说明.md"]:
    shutil.copy2(ROOT / name, editor / name)
for folder in ["examples", "docs"]:
    shutil.copytree(ROOT / folder, editor / folder, dirs_exist_ok=True)
shutil.copytree(ANDROID, editor / "android-runtime")

tree = subprocess.check_output([
    "cargo", "tree", "--locked", "--target", "x86_64-pc-windows-msvc",
    "--prefix", "none", "--format", "{p}"], cwd=ROOT, text=True, encoding="utf-8")
packages = set(re.findall(r"^([^\s]+) v([^\s]+)", tree, re.M))
registry = Path.home() / ".cargo/registry/src"
licenses = editor / "licenses"
licenses.mkdir(exist_ok=True)
index = []
for name, version in sorted(packages):
    if name == "opengalgame":
        continue
    candidates = sorted(registry.glob(f"*/{name}-{version}"))
    assert candidates, (name, version)
    source = candidates[0]
    metadata = tomllib.loads((source / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    saved = []
    for file in source.rglob("*"):
        if not file.is_file() or file.stat().st_size > 1024 * 1024:
            continue
        if not any(term in file.name.upper() for term in ["LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "OFL"]):
            continue
        target = licenses / f"{name}-{version}" / file.relative_to(source)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(file, target)
        saved.append(target.relative_to(licenses).as_posix())
    index.append({"name": name, "version": version, "license": metadata.get("license", "see package"),
                  "repository": metadata.get("repository", ""), "files": saved})
(licenses / "index.json").write_text(json.dumps(index, ensure_ascii=False, indent=2), encoding="utf-8")
for example in ["first-light", "long-form", "harbor-signals", "tidal-stage"]:
    subprocess.run([str(editor / "opengalgame.exe"), "--validate", str(editor / f"examples/{example}/project.json")], check=True)
subprocess.run([str(editor / "opengalgame.exe"), "--replay",
                str(editor / "examples/harbor-signals/project.json"),
                str(editor / "examples/harbor-signals/route-promise.json")], check=True)
subprocess.run([str(editor / "opengalgame.exe"), "--replay",
                str(editor / "examples/tidal-stage/project.json"),
                str(editor / "examples/tidal-stage/route-demo.json")], check=True)
subprocess.run([str(editor / "opengalgame.exe"), "--test",
                str(editor / "examples/tidal-stage/project.json"),
                str(RELEASE / f"test-report-v{VERSION}.json")], check=True)

games = []
for prefix, example in [("three-letters", "long-form"), ("harbor-signals", "harbor-signals"), ("tidal-stage", "tidal-stage")]:
    name = f"{prefix}-v{VERSION}-windows-x64"
    folder = fresh_directory(name)
    subprocess.run([str(editor / "opengalgame.exe"), "--export", str(editor / f"examples/{example}/project.json"), str(folder)], check=True)
    subprocess.run([str(folder / "Game.exe"), "--validate", str(folder / "game.json")], check=True)
    if example == "tidal-stage":
        subprocess.run([str(folder / "Game.exe"), "--test", str(folder / "game.json"), str(folder / "test-report.json")], check=True)
    games.append((name, folder))
game = games[0][1]
artifacts = []
for name, folder in [(editor_name, editor)] + games:
    files = {p.relative_to(folder).as_posix(): p for p in folder.rglob("*") if p.is_file()}
    artifacts.append(archive_files(name, files))

with tempfile.TemporaryDirectory(dir=RELEASE) as temp:
    demo = editor / "examples/tidal-stage/project.json"
    exe = Path(temp) / f"tidal-stage-v{VERSION}-windows-x64.exe"
    apk = Path(temp) / f"tidal-stage-v{VERSION}-android.apk"
    subprocess.run([str(editor / "opengalgame.exe"), "--export-exe", str(demo), str(exe)], check=True)
    subprocess.run([str(exe), "--verify-bundle"], check=True)
    env = os.environ.copy()
    env.setdefault("ANDROID_HOME", str(Path(env["LOCALAPPDATA"]) / "Android/Sdk"))
    env["LOCALAPPDATA"] = str(Path(temp) / "demo-signing")
    subprocess.run([str(editor / "opengalgame.exe"), "--export-apk", str(demo), str(apk)], check=True, env=env)
    with zipfile.ZipFile(apk) as archive:
        assert archive.testzip() is None
        assert "AndroidManifest.xml" in archive.namelist()
        for abi, machine in [("arm64-v8a", 183), ("x86_64", 62)]:
            with archive.open(f"lib/{abi}/libopengalgame.so") as lib:
                header = lib.read(20)
                assert header[:6] == b"\x7fELF\x02\x01" and int.from_bytes(header[18:20], "little") == machine
        with archive.open("assets/game.zip") as inner:
            with zipfile.ZipFile(inner) as data:
                assert data.testzip() is None
                assert json.load(data.open("game.json"))["title"]
                assert any(name.startswith("assets/") for name in data.namelist())
    for package in (exe, apk):
        target = RELEASE / package.name
        shutil.copy2(package, target)
        artifacts.append({"file": target.name, "bytes": target.stat().st_size,
                          "sha256": hashlib.sha256(target.read_bytes()).hexdigest()})

source_files = {}
for folder in ["src", "tests", "examples", ".cargo", "docs", ".github"]:
    for file in (ROOT / folder).rglob("*"):
        if file.is_file() and not file.name.endswith((".bak", ".tmp")):
            source_files[file.relative_to(ROOT).as_posix()] = file
for name in ["Cargo.toml", "Cargo.lock", "README.md", "DESIGN.md", "LICENSE", "THIRD_PARTY.md", "CONTRIBUTING.md",
             ".gitignore", ".gitattributes", "一键使用说明.md", "打开编辑器.cmd", "从源码编译运行.cmd",
             "tools/package_windows.py", "tools/build_android.py", "tools/make_long_demo.py", "tools/make_v4_demo.py", "tools/make_production_demo.py", "tools/make_demo_voices.ps1"]:
    source_files[name] = ROOT / name
artifacts.append(archive_files(f"opengalgame-v{VERSION}-source", source_files))
standalone = RELEASE / "opengalgame.exe"
shutil.copy2(BINARY, standalone)
artifacts.append({"file": standalone.name, "bytes": standalone.stat().st_size,
                  "sha256": hashlib.sha256(standalone.read_bytes()).hexdigest()})
(RELEASE / "SHA256SUMS.txt").write_text("".join(f"{a['sha256']}  {a['file']}\n" for a in artifacts), encoding="utf-8")
report = {"version": VERSION, "platform": "x86_64-pc-windows-msvc", "editor_directory": str(editor),
          "sample_directory": str(game), "logic_sample_directory": str(games[1][1]), "production_sample_directory": str(games[2][1]), "dependencies_with_license_metadata": len(index), "artifacts": artifacts}
(ROOT / "reports").mkdir(exist_ok=True)
(ROOT / "reports/package-opengalgame.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps(report, ensure_ascii=True, indent=2))
