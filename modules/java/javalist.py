import json
from pathlib import Path
from typing import Any, Dict, List

# Path to java_list.json at project root
JAVA_LIST_PATH = Path(__file__).resolve().parent.parent.parent / "java_list.json"


def ensure_java_list() -> None:
    """Create java_list.json with an empty list when missing."""
    if not JAVA_LIST_PATH.exists():
        JAVA_LIST_PATH.write_text("[]", encoding="utf-8")


def load_java_list() -> List[Dict[str, Any]]:
    """Load java_list.json safely, falling back to an empty list."""
    ensure_java_list()
    try:
        data = json.loads(JAVA_LIST_PATH.read_text(encoding="utf-8"))
        if isinstance(data, list):
            return data
    except json.JSONDecodeError:
        pass
    return []


def save_java_list(data: List[Dict[str, Any]]) -> None:
    """Persist the given list into java_list.json."""
    JAVA_LIST_PATH.write_text(
        json.dumps(data, indent=4, ensure_ascii=False), encoding="utf-8"
    )


def _normalize_entry(entry: Dict[str, Any]) -> Dict[str, Any]:
    """Normalize a single Java entry before storing."""
    normalized = {
        "path": entry.get("path"),
        "version": entry.get("version") or "Unknown",
        "vendor": entry.get("vendor") or entry.get("vendor_version") or "Unknown",
        "arch": entry.get("arch") or "Unknown",
    }
    normalized["name"] = (
        entry.get("name")
        or normalized["vendor"]
        or normalized["version"]
        or "Unknown"
    )
    return normalized


def upsert_entries(entries: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    """
    Merge detected entries into java_list.json by java path.

    Entries with the same path will be updated; new paths will be appended.
    Returns the merged list.
    """
    existing = load_java_list()
    index_by_path = {
        item.get("path"): idx for idx, item in enumerate(existing) if item.get("path")
    }
    updated = list(existing)

    for entry in entries:
        normalized = _normalize_entry(entry)
        path = normalized.get("path")
        if not path:
            continue
        if path in index_by_path:
            updated[index_by_path[path]].update(normalized)
        else:
            updated.append(normalized)
            index_by_path[path] = len(updated) - 1

    save_java_list(updated)
    return updated
