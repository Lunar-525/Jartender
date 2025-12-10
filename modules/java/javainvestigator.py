import subprocess
import tempfile
from pathlib import Path
from typing import Dict, Optional

def check_java_with_code(java_path: str) -> dict:
    """用 Java 代码检测架构（更详细）"""
    
    # 创建临时 Java 文件
    java_code = """
public class ArchCheck {
    public static void main(String[] args) {
        System.out.println("os.arch=" + System.getProperty("os.arch"));
        System.out.println("os.name=" + System.getProperty("os.name"));
        System.out.println("java.version=" + System.getProperty("java.version"));
        System.out.println("java.vendor=" + System.getProperty("java.vendor"));
        System.out.println("java.home=" + System.getProperty("java.home"));
    }
}
"""
    
    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = Path(tmpdir)
        java_file = tmpdir / "ArchCheck.java"
        java_file.write_text(java_code)
        
        try:
            # 编译
            subprocess.run(
                [java_path.replace('/java', '/javac'), str(java_file)],
                check=True,
                capture_output=True,
                timeout=10
            )
            
            # 运行
            result = subprocess.run(
                [java_path, "-cp", str(tmpdir), "ArchCheck"],
                capture_output=True,
                text=True,
                timeout=10
            )
            
            # 解析输出
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
    Run `java -XshowSettings:properties -version` and parse key props.
    Returns version/vendor_version/arch along with raw path.
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