import os
import json
import shutil
from tabulate import tabulate

CONFIG_FILE = "./list.json"

def load_server_list():
    """加载 list.json 并解析 JSON"""
    if not os.path.exists(CONFIG_FILE):
        print(f"❌ 配置文件 {CONFIG_FILE} 不存在！")
        return []

    try:
        with open(CONFIG_FILE, "r", encoding="utf-8") as file:
            return json.load(file)
    except json.JSONDecodeError as e:
        print(f"❌ 解析 JSON 出错: {e}")
        return []

def get_terminal_width():
    """获取终端窗口的宽度"""
    return shutil.get_terminal_size((80, 20)).columns

def display_servers(servers):
    """格式化并展示服务器列表，并返回用户选择的服务器"""
    if not servers:
        print("⚠️ 没有找到服务器数据！")
        return None

    headers = ["序号", "服务器 (server_name)", "核心类型 (server_type)", "Minecraft 版本 (minecraft_version)", "核心版本 (loader_version)"]
    table_data = [[i + 1, s["server_name"], s["server_type"], s["minecraft_version"], s["loader_version"]] for i, s in enumerate(servers)]

    terminal_width = get_terminal_width()
    table = tabulate(table_data, headers=headers, tablefmt="grid")

    # 控制台自适应表格宽度
    if len(table.split("\n")[0]) > terminal_width:
        table = tabulate(table_data, headers=headers, tablefmt="pipe")

    print(table)

    # 等待用户输入序号
    while True:
        try:
            choice = int(input("🔢 请输入要选择的服务器序号: "))
            if 1 <= choice <= len(servers):
                return servers[choice - 1]["server_name"]
            else:
                print("❌ 输入超出范围，请重新输入！")
        except ValueError:
            print("❌ 请输入有效的数字！")

if __name__ == "__main__":
    server_list = load_server_list()
    selected_server = display_servers(server_list)

    if selected_server:
        print(f"✅ 你选择了服务器: {selected_server}")
