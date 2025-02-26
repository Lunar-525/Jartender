import os
import json
import subprocess

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

def launch(current_server,current_dir,gui):
    """启动指定的 Minecraft 服务器"""
    servers = load_server_list()
    if not servers:
        return

    # 查找匹配的服务器
    selected_server = next((s for s in servers if s["server_name"] == current_server), None)
    if not selected_server:
        print(f"❌ 找不到名为 '{current_server}' 的服务器！")
        return

    fullpath = selected_server["jar_path"]

    if not os.path.exists(fullpath):
        print(f"❌ JAR 文件未找到: {fullpath}")
        return

    print(f"🚀 正在启动服务器: {current_server} ...")
    print(f"📂 服务器核心路径: {fullpath}")

    try:
        os.chdir(os.path.join(current_dir, "Servers", current_server))
        if not gui:
            subprocess.run(["java", "-jar", fullpath,"-nogui"], check=True)
        elif gui:
            subprocess.run(["java", "-jar", fullpath], check=True)
        else:
            print(f"❌ 未传递gui参数或参数出现问题")
    except subprocess.CalledProcessError as e:
        print(f"❌ 服务器启动失败: {e}")
    except FileNotFoundError:
        print("❌ 未找到 Java，请确保 Java 已正确安装并添加到环境变量！")

if __name__ == "__main__":
    launch("1.21.4-Fabric")  # 测试启动
