import json
from pathlib import Path
from typing import Tuple

# 统一的路径定义，避免各模块随意切换工作目录
PROJECT_ROOT = Path(__file__).resolve().parent.parent
CONFIG_PATH = PROJECT_ROOT / "config.json"
LIST_PATH = PROJECT_ROOT / "list.json"
DEFAULT_SERVERS_DIR = PROJECT_ROOT / "Servers"


def ensure_bootstrap_files() -> Tuple[bool, bool]:
    """
    确保 config.json、list.json、Servers 目录存在。
    :return: (config_created, list_created)
    """
    config_created = False
    list_created = False

    if not CONFIG_PATH.exists():
        DEFAULT_SERVERS_DIR.mkdir(parents=True, exist_ok=True)
        config = {"serverpath": str(DEFAULT_SERVERS_DIR.resolve())}
        CONFIG_PATH.write_text(json.dumps(config, indent=4), encoding="utf-8")
        config_created = True

    if not LIST_PATH.exists():
        LIST_PATH.write_text("[]", encoding="utf-8")
        list_created = True

    return config_created, list_created


def load_config() -> dict:
    with open(CONFIG_PATH, "r", encoding="utf-8") as f:
        return json.load(f)


def save_config(config: dict) -> None:
    CONFIG_PATH.write_text(json.dumps(config, indent=4), encoding="utf-8")


def server_root(config: dict | None = None) -> Path:
    cfg = config if config is not None else load_config()
    return Path(cfg["serverpath"]).expanduser()
