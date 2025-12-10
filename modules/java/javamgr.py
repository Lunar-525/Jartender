import json
from pathlib import Path
from modules import Settings, java
from modules.java import javafinder

JAVA_LIST_PATH = Path(__file__).resolve().parent.parent.parent / "java_list.json"


def ensure_java_list():
    """Ensure java_list.json exists with an empty array."""
    if not JAVA_LIST_PATH.exists():
        JAVA_LIST_PATH.write_text("[]", encoding="utf-8")


def load_java_list():
    ensure_java_list()
    try:
        return json.loads(JAVA_LIST_PATH.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return []


def save_java_list(data):
    JAVA_LIST_PATH.write_text(json.dumps(data, indent=4, ensure_ascii=False), encoding="utf-8")


def main_menu():
    while True:
        print("\n=== Java 管理 ===")
        print("1. 管理 Java")
        print("2. 下载 Java")
        print("3. 自动检测 Java")
        print("0. 返回")
        choice = input("请选择操作: ").strip()

        if choice == "1":
            manage_java()
        elif choice == "2":
            download_java()
        elif choice == "3":
            detect_java()
        elif choice == "0":
            return
        else:
            print("无效输入，请重新选择。")


def manage_java():
    """Placeholder: list installed/registered Java entries from json."""
    java_list = load_java_list()
    if not java_list:
        print("尚未记录任何 Java 版本。")
        return
    print("当前已记录的 Java 版本：")
    for idx, entry in enumerate(java_list, 1):
        print(f"{idx}. {entry.get('name','unknown')} @ {entry.get('path','')}")


def download_java():
    """Placeholder for future download flow."""
    print("下载 Java（占位）：")
    print("1) 选择版本 -> 2) 选择发行版(Adoptium/Azul/OpenJDK/GraalVM...) -> TODO")


def detect_java():
    """Placeholder: auto-detect Java and list."""
    javafinder.find_java()
    manage_java()
