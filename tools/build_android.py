"""Cross-compile the shared Rust player for the APK exporter."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
TARGETS = {"arm64-v8a": "aarch64-linux-android", "x86_64": "x86_64-linux-android"}
NDK_VERSION = "28.2.13676358"


def ndk_tools():
    ndk = os.environ.get("ANDROID_NDK_HOME")
    if not ndk:
        sdk = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
        if not sdk and os.name == "nt":
            sdk = Path(os.environ["LOCALAPPDATA"]) / "Android/Sdk"
        if not sdk:
            raise SystemExit("Set ANDROID_HOME or ANDROID_NDK_HOME")
        ndk = Path(sdk) / "ndk" / NDK_VERSION
    host = "windows-x86_64" if os.name == "nt" else "linux-x86_64"
    tools = Path(ndk).resolve() / "toolchains/llvm/prebuilt" / host / "bin"
    if not tools.is_dir():
        raise SystemExit(f"Android NDK {NDK_VERSION} is required. Missing {tools}")
    return tools


def build():
    tools = ndk_tools()
    wrapper = ".cmd" if os.name == "nt" else ""
    executable = ".exe" if os.name == "nt" else ""
    dest = ROOT / "target/android-runtime"
    for abi, triple in TARGETS.items():
        env = os.environ.copy()
        linker = tools / f"{triple}26-clang{wrapper}"
        key = triple.replace("-", "_")
        env[f"CARGO_TARGET_{key.upper()}_LINKER"] = str(linker)
        env[f"CC_{key}"] = str(tools / f"clang{executable}")
        env[f"CXX_{key}"] = str(tools / f"clang++{executable}")
        env[f"CFLAGS_{key}"] = f"--target={triple}26"
        env[f"CXXFLAGS_{key}"] = f"--target={triple}26"
        env[f"AR_{key}"] = str(tools / f"llvm-ar{executable}")
        env[f"CXXSTDLIB_{key}"] = "c++_static"
        subprocess.run(
            ["cargo", "rustc", "--locked", "--release", "--target", triple,
             "--lib", "--crate-type", "cdylib"],
            check=True, cwd=ROOT, env=env,
        )
        library = ROOT / f"target/{triple}/release/libopengalgame.so"
        dynamic = subprocess.check_output(
            [tools / f"llvm-readelf{executable}", "-d", "-s", library], text=True, encoding="utf-8",
        )
        if "ANativeActivity_onCreate" not in dynamic or "libc++_shared.so" in dynamic:
            raise SystemExit(f"{abi} is missing its entry point or requires an external C++ runtime")
        (dest / abi).mkdir(parents=True, exist_ok=True)
        shutil.copy2(library, dest / abi / library.name)
    make_notices(dest)
    (dest / "version.txt").write_text(f"{VERSION}\n", encoding="utf-8")
    print(dest)


def make_notices(dest):
    packages = set()
    for triple in TARGETS.values():
        tree = subprocess.check_output(
            ["cargo", "tree", "--locked", "--target", triple, "--prefix", "none", "--format", "{p}"],
            cwd=ROOT, text=True, encoding="utf-8",
        )
        packages.update(re.findall(r"^([^\s]+) v([^\s]+)", tree, re.M))
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    registry = cargo_home / "registry/src"
    with zipfile.ZipFile(dest / "notices.zip", "w", zipfile.ZIP_DEFLATED, strict_timestamps=False) as archive:
        index = []
        for name, version in sorted(packages):
            if name == "opengalgame":
                continue
            identifier = f"{name}-{version}"
            package = next(registry.glob(f"*/{identifier}"))
            info = tomllib.loads((package / "Cargo.toml").read_text(encoding="utf-8"))["package"]
            files = []
            for file in package.rglob("*"):
                if file.is_file() and file.stat().st_size <= 1024 * 1024 and any(
                    term in file.name.upper()
                    for term in ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "OFL")
                ):
                    relative = f"{identifier}/{file.relative_to(package).as_posix()}"
                    archive.write(file, relative)
                    files.append(relative)
            index.append({"name": name, "version": version, "license": info.get("license"),
                          "repository": info.get("repository"), "files": files})
        archive.writestr("index.json", json.dumps(index, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    build()
