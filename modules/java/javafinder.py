import os
import sys
import platform
from pathlib import Path
from typing import Literal, TypedDict, List

# Windows 注册表访问
if platform.system() == "Windows":
    try:
        import winreg
    except ImportError:
        winreg = None
else:
    winreg = None

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


def _parse_version(version_str: str) -> tuple:
    """
    解析版本号字符串为可比较的元组
    例如: "1.8.0_442" -> (1, 8, 0, 442)
          "11.0.1" -> (11, 0, 1)
          "17.0.1" -> (17, 0, 1)
    对于 "Unknown" 或无效版本，return 0, 以确保排在最后。
    """
    if not version_str or version_str == "Unknown":
        return (0,)
    
    try:
        # 移除可能的非数字前缀
        version_str = version_str.strip()
        
        # 处理下划线和连字符分隔的版本号
        parts = version_str.replace("_", ".").replace("-", ".").split(".")
        
        # 解析每个部分为整数
        version_parts = []
        for part in parts:
            # 只取数字部分
            num_str = ""
            for char in part:
                if char.isdigit():
                    num_str += char
                else:
                    break
            if num_str:
                version_parts.append(int(num_str))
        
        if version_parts:
            return tuple(version_parts)
        else:
            return (999999,)
    except (ValueError, AttributeError):
        return (999999,)


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


def _windows_candidates() -> List[Path]:
    """查找 Windows 上的 Java 安装路径"""
    if not winreg:
        return []
    
    candidates = []
    
    # 注册表查找函数
    def find_java_from_registry(
        key_type: int,
        key_name: str,
        value_name: str,
        subkey_suffix: str = ""
    ) -> List[Path]:
        """从注册表查找 Java 安装路径"""
        results = []
        for base_key in [winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE]:
            try:
                with winreg.OpenKey(
                    base_key,
                    key_name,
                    0,
                    winreg.KEY_READ | key_type | winreg.KEY_ENUMERATE_SUB_KEYS
                ) as jre_key:
                    # 枚举子键
                    i = 0
                    while True:
                        try:
                            subkey_name = winreg.EnumKey(jre_key, i)
                            new_key_name = f"{key_name}\\{subkey_name}{subkey_suffix}"
                            
                            try:
                                with winreg.OpenKey(
                                    base_key,
                                    new_key_name,
                                    0,
                                    winreg.KEY_READ | key_type
                                ) as new_key:
                                    try:
                                        java_home, _ = winreg.QueryValueEx(new_key, value_name)
                                        java_home_path = Path(java_home)
                                        
                                        # 尝试不同的路径组合
                                        # 1. 如果 Path 指向 bin 目录
                                        if (java_home_path / "java.exe").exists():
                                            results.append(java_home_path / "java.exe")
                                        # 2. 如果 Path 指向 Java 安装根目录
                                        elif (java_home_path / "bin" / "java.exe").exists():
                                            results.append(java_home_path / "bin" / "java.exe")
                                    except FileNotFoundError:
                                        pass
                            except (FileNotFoundError, OSError):
                                pass
                            
                            i += 1
                        except OSError:
                            break
            except (FileNotFoundError, OSError):
                pass
        
        return results
    
    # (key_name, value_name, subkey_suffix, arch_flags)
    # arch_flags: 0=64, 1=32, 2=both
    registry_configs = [
        # Oracle Java
        (r"SOFTWARE\JavaSoft\Java Runtime Environment", "JavaHome", "", 2),
        (r"SOFTWARE\JavaSoft\Java Development Kit", "JavaHome", "", 2),
        # JavaSoft (Java 9+、Oracle、Amazon Corretto)
        (r"SOFTWARE\JavaSoft\JRE", "JavaHome", "", 2),
        (r"SOFTWARE\JavaSoft\JDK", "JavaHome", "", 2),
        # AdoptOpenJDK
        (r"SOFTWARE\AdoptOpenJDK\JRE", "Path", r"\hotspot\MSI", 2),
        (r"SOFTWARE\AdoptOpenJDK\JDK", "Path", r"\hotspot\MSI", 2),
        # Eclipse Foundation
        (r"SOFTWARE\Eclipse Foundation\JDK", "Path", r"\hotspot\MSI", 2),
        # Eclipse Adoptium
        (r"SOFTWARE\Eclipse Adoptium\JRE", "Path", r"\hotspot\MSI", 2),
        (r"SOFTWARE\Eclipse Adoptium\JDK", "Path", r"\hotspot\MSI", 2),
        # IBM Semeru
        (r"SOFTWARE\Semeru\JRE", "Path", r"\openj9\MSI", 2),
        (r"SOFTWARE\Semeru\JDK", "Path", r"\openj9\MSI", 2),
        # Microsoft
        (r"SOFTWARE\Microsoft\JDK", "Path", r"\hotspot\MSI", 0),  #64
        # Azul Zulu
        (r"SOFTWARE\Azul Systems\Zulu", "InstallationPath", "", 2),
        # BellSoft Liberica
        (r"SOFTWARE\BellSoft\Liberica", "InstallationPath", "", 2),
        # BellSoft LibericaNIK
        (r"SOFTWARE\BellSoft\LibericaNIK", "InstallationPath", "", 2),
        # SapMachine
        (r"SOFTWARE\SapMachine\JDK", "JavaHome", "", 2),
    ]
    
    # 循环处理所有配置
    for key_name, value_name, subkey_suffix, arch_flags in registry_configs:
        if arch_flags == 0 or arch_flags == 2:  # 64位
            candidates.extend(find_java_from_registry(
                winreg.KEY_WOW64_64KEY,
                key_name,
                value_name,
                subkey_suffix
            ))
        if arch_flags == 1 or arch_flags == 2:  # 32位
            candidates.extend(find_java_from_registry(
                winreg.KEY_WOW64_32KEY,
                key_name,
                value_name,
                subkey_suffix
            ))    

    
    # 扫描 Program Files\Java 和 Program Files (x86)\Java 下的所有子目录
    java_dirs = [
        Path(r"C:\Program Files\Java"),
        Path(r"C:\Program Files (x86)\Java"),
    ]
    
    for java_dir in java_dirs:
        if java_dir.exists() and java_dir.is_dir():
            for subdir in java_dir.iterdir():
                if subdir.is_dir():
                    # 检查子目录是否包含 bin、include、jre、lib 这些文件夹
                    required_dirs = {"bin", "include", "jre", "lib"}
                    subdir_dirs = {d.name.lower() for d in subdir.iterdir() if d.is_dir()}
                    
                    # 检查是否包含所有必需的目录（至少包含 bin）
                    if "bin" in subdir_dirs and required_dirs.intersection(subdir_dirs):
                        java_exe = subdir / "bin" / "java.exe"
                        if java_exe.exists():
                            candidates.append(java_exe)
    
    # 默认路径（64位优先）- 保留作为后备
    default_paths_64 = [
        Path(r"C:\Program Files\Java\jre8\bin\java.exe"),
        Path(r"C:\Program Files\Java\jre7\bin\java.exe"),
        Path(r"C:\Program Files\Java\jre6\bin\java.exe"),
    ]
    candidates.extend(default_paths_64)
    
    # 默认路径（32位）- 保留作为后备
    default_paths_32 = [
        Path(r"C:\Program Files (x86)\Java\jre8\bin\java.exe"),
        Path(r"C:\Program Files (x86)\Java\jre7\bin\java.exe"),
        Path(r"C:\Program Files (x86)\Java\jre6\bin\java.exe"),
    ]
    candidates.extend(default_paths_32)
    
    # PrismLauncher Java 目录
    appdata = os.environ.get("APPDATA")
    if appdata:
        prism_java_dir = Path(appdata) / "PrismLauncher" / "java"
        if prism_java_dir.exists():
            # 扫描所有子目录中的 bin\java.exe
            for java_subdir in prism_java_dir.iterdir():
                if java_subdir.is_dir():
                    java_exe = java_subdir / "bin" / "java.exe"
                    if java_exe.exists():
                        candidates.append(java_exe)
    
    # JAVA_HOME 环境变量
    java_home = os.environ.get("JAVA_HOME")
    if java_home:
        candidates.append(Path(java_home) / "bin" / "java.exe")
    
    # PATH 环境变量中的 java.exe
    path_env = os.environ.get("PATH", "")
    for path_str in path_env.split(os.pathsep):
        if path_str:
            java_path = Path(path_str) / "java.exe"
            if java_path.exists():
                candidates.append(java_path)
    
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
                        "version": meta.get("version") or "Unknown",
                        "vendor": meta.get("vendor_version") or "Unknown",
                        "arch": meta.get("arch") or "Unknown",
                        "path": str(p),
                    }
                )

            # 按版本号升序排序
            rows.sort(key=lambda row: _parse_version(row["version"]))

            # 更新索引
            for idx, row in enumerate(rows, 1):
                row["idx"] = str(idx)

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
                
    elif info["os"] == "Windows":
        paths = _windows_candidates()
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
                        "version": meta.get("version") or "Unknown",
                        "vendor": meta.get("vendor_version") or "Unknown",
                        "arch": meta.get("arch") or "Unknown",
                        "path": str(p),
                    }
                )

            # 按版本号升序排序
            rows.sort(key=lambda row: _parse_version(row["version"]))

            # 更新索引
            for idx, row in enumerate(rows, 1):
                row["idx"] = str(idx)

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

if __name__ == "__main__":
    find_java()