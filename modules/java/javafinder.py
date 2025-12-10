import os
import sys
import platform
from pathlib import Path
from typing import Literal, TypedDict, List

# 允许作为脚本直接运行
PROJECT_ROOT = Path(__file__).resolve().parents[2]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

from modules.java import javainvestigator

OSType = Literal["Windows", "Linux", "macOS", "Unknown"]


class OSInfo(TypedDict):
    os: OSType
    raw: str


def detect_os() -> OSInfo:
    """Detect operating system in a simple normalized form."""
    sysname = platform.system().lower()
    if "windows" in sysname:
        os_name: OSType = "Windows"
    elif "linux" in sysname:
        os_name = "Linux"
    elif "darwin" in sysname:
        os_name = "macOS"
    else:
        os_name = "Unknown"
    return {"os": os_name, "raw": platform.platform()}


def _unique_existing(paths: List[Path]) -> List[Path]:
    seen = set()
    result = []
    for p in paths:
        if not p:
            continue
        rp = p.resolve()
        if rp in seen:
            continue
        if rp.exists():
            seen.add(rp)
            result.append(rp)
    return result


def _macos_candidates() -> List[Path]:
    home = Path(os.environ.get("HOME", ""))
    candidates = [
        Path("/usr/bin/java"),
        Path("/Applications/Xcode.app/Contents/Applications/Application Loader.app/Contents/MacOS/itms/java/bin/java"),
        Path("/Library/Internet Plug-Ins/JavaAppletPlugin.plugin/Contents/Home/bin/java"),
        Path("/System/Library/Frameworks/JavaVM.framework/Versions/Current/Commands/java"),
    ]

    # /Library/Java/JavaVirtualMachines/*
    lib_jvm = Path("/Library/Java/JavaVirtualMachines")
    candidates += list(lib_jvm.glob("*/Contents/Home/bin/java"))
    candidates += list(lib_jvm.glob("*/Contents/Home/jre/bin/java"))

    # /System/Library/Java/JavaVirtualMachines/*
    sys_lib_jvm = Path("/System/Library/Java/JavaVirtualMachines")
    candidates += list(sys_lib_jvm.glob("*/Contents/Home/bin/java"))
    candidates += list(sys_lib_jvm.glob("*/Contents/Commands/java"))

    # sdkman
    sdkman_dir = Path(os.environ.get("SDKMAN_DIR", home / ".sdkman"))
    candidates += list((sdkman_dir / "candidates/java").glob("*/bin/java"))

    # asdf
    asdf_dir = Path(os.environ.get("ASDF_DATA_DIR", home / ".asdf"))
    candidates += list((asdf_dir / "installs/java").glob("*/bin/java"))

    # user Library JDKs (e.g., IntelliJ downloads)
    user_lib_jvm = home / "Library/Java/JavaVirtualMachines"
    candidates += list(user_lib_jvm.glob("*/Contents/Home/bin/java"))
    candidates += list(user_lib_jvm.glob("*/Contents/Commands/java"))

    # JAVA_HOME if set
    java_home = os.environ.get("JAVA_HOME")
    if java_home:
        candidates.append(Path(java_home) / "bin/java")

    return _unique_existing(candidates)


def find_java():
    """Entry used by javamgr."""
    info = detect_os()
    print(f"{info['os']} ({info['raw']})")
    if info["os"] == "macOS":
        paths = _macos_candidates()
        if not paths:
            print("未找到任何已存在的 Java 可执行文件。")
        else:
            print("发现：")
            rows = []
            for idx, p in enumerate(paths, 1):
                meta = javainvestigator.probe_show_settings(str(p))
                rows.append(
                    {
                        "idx": str(idx),
                        "version": meta.get("version") or "未知",
                        "vendor": meta.get("vendor_version") or "未知",
                        "arch": meta.get("arch") or "未知",
                        "path": str(p),
                    }
                )

            # 动态列宽对齐
            headers = {"idx": "#", "version": "版本", "vendor": "名称", "arch": "架构", "path": "路径"}
            col_widths = {
                key: max(len(headers[key]), *(len(row[key]) for row in rows))
                for key in headers
            }

            def fmt(row):
                return " | ".join(
                    [
                        row["idx"].rjust(col_widths["idx"]),
                        row["version"].ljust(col_widths["version"]),
                        row["vendor"].ljust(col_widths["vendor"]),
                        row["arch"].ljust(col_widths["arch"]),
                        row["path"],
                    ]
                )

            header_line = " | ".join(
                [
                    headers["idx"].rjust(col_widths["idx"]),
                    headers["version"].ljust(col_widths["version"]),
                    headers["vendor"].ljust(col_widths["vendor"]),
                    headers["arch"].ljust(col_widths["arch"]),
                    headers["path"],
                ]
            )
            print(header_line)
            print("-" * len(header_line))
            for row in rows:
                print(fmt(row))
    else:
        print("当前仅实现 macOS 路径扫描，其它平台待实现。")

if __name__ == "__main__":
    find_java()