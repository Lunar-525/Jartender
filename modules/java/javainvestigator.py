import subprocess
import tempfile
from pathlib import Path
from typing import Dict, Optional

def check_java_with_code(java_path: str) -> dict:

    # 弃用。反正我觉得暂时用不到。
    
    java_file_path = Path(__file__).parent / "ArchCheck.java"
    java_code = java_file_path.read_text(encoding="utf-8")
    
    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = Path(tmpdir)
        java_file = tmpdir / "ArchCheck.java"
        java_file.write_text(java_code, encoding="utf-8")
        
        try:
            subprocess.run(
                [java_path.replace('/java', '/javac'), str(java_file)],
                check=True,
                capture_output=True,
                timeout=10
            )
            
            result = subprocess.run(
                [java_path, "-cp", str(tmpdir), "ArchCheck"],
                capture_output=True,
                text=True,
                timeout=10
            )
            
            data = {}
            for line in result.stdout.split('\n'):
                if '=' in line:
                    key, value = line.split('=', 1)
                    data[key] = value
            
            return {
                "path": java_path,
                "arch": data.get("os.arch"),
                "os_name": data.get("os.name"),
                "version": data.get("java.version"),
                "vendor": data.get("java.vendor"),
                "java_home": data.get("java.home"),
                "is_64bit": data.get("os.arch") in ['x86_64', 'amd64', 'aarch64', 'arm64', 'riscv64'],
                "valid": True
            }
            
        except Exception as e:
            return {"path": java_path, "valid": False, "error": str(e)}


def probe_show_settings(java_path: str) -> Dict[str, Optional[str]]:
    """
    java -XshowSettings:properties -version
    Returns version/vendor_version/arch along with raw path.
    这不比上面那个简单多了吗, 也不用区分是Java8还是Java17+。
    """
    try:
        result = subprocess.run(
            [java_path, "-XshowSettings:properties", "-version"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=10,
        )
    except Exception as e:
        return {"path": java_path, "valid": False, "error": str(e)}

    combined = "\n".join([result.stdout or "", result.stderr or ""])
    version = _extract_prop(combined, "java.version")
    vendor_version = _extract_prop(combined, "java.vendor.version")
    if not vendor_version:
        vendor_version = _extract_prop(combined, "java.vendor")
    arch = _extract_prop(combined, "os.arch")

    return {
        "path": java_path,
        "version": version,
        "vendor_version": vendor_version,
        "arch": arch,
        "valid": bool(version or vendor_version or arch),
    }


def _extract_prop(text: str, key: str) -> Optional[str]:
    for line in text.splitlines():
        if f"{key} =" in line:
            return line.split("=", 1)[1].strip()
    return None

if __name__ == "__main__":
    print(probe_show_settings("/usr/bin/java"))