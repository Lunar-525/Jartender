import os
import sys
import json
from pathlib import Path

class BColors:
    HEADER = '\033[95m'
    OKBLUE = '\033[94m'
    OKCYAN = '\033[96m'
    OKGREEN = '\033[38;2;120;200;120m'
    LOGOYELLOW = '\033[38;2;200;180;100m'
    WARNING = '\033[93m'
    FAIL = '\033[91m'
    ENDC = '\033[0m'
    BOLD = '\033[1m'
    UNDERLINE = '\033[4m'

from modules import (
    Manifester,
    Contractor,
    Serverlistinitializer,
    Lister,
    ServerLauncher,
    AboutJartender,
    Settings,
    ServerInstaller,
)
from modules.java import javamgr
# 读取服务器 EULA 初始化列表 列出 服务器启动 关于 设置 安装

if __name__ == "__main__":
    # 基本变量初始化
    current_dir = Path(__file__).parent
    modules_dir = current_dir / "modules"
    sys.path.insert(0, str(modules_dir))
    current_server = "None"
    def initialize():
        config_exists = Settings.CONFIG_PATH.exists()
        list_exists = Settings.LIST_PATH.exists()

        if not list_exists:
            # 首次启动，要求用户设置服务器存储目录（默认当前目录下 Servers）
            default_path = Settings.DEFAULT_SERVERS_DIR
            user_input = input(
                f"请输入服务器存储目录，回车采用默认路径 [{default_path}]: "
            ).strip()
            chosen_path = Path(user_input).expanduser() if user_input else default_path
            chosen_path.mkdir(parents=True, exist_ok=True)

            Settings.save_config({"serverpath": str(chosen_path.resolve())})
            Settings.LIST_PATH.write_text("[]", encoding="utf-8")

            print(BColors.OKGREEN + f"✅ 已将服务器目录设置为: {chosen_path.resolve()}")
            print("如需更改目录，可修改 config.json 中的 serverpath。")
        else:
            # list.json 已存在但 config.json 可能缺失时，回填默认配置
            if not config_exists:
                Settings.save_config({"serverpath": str(Settings.DEFAULT_SERVERS_DIR)})

    initialize()


    def gradient_yellow_rgb(text, offset):
        start_color = (112, 214, 255)
        end_color = (255, 112, 166)
        length = len(text)
        colored_text = ""

        offset = offset * 2

        for i, char in enumerate(text):
            factor = (i + offset) / (length + offset)
            r = int(start_color[0] + (end_color[0] - start_color[0]) * factor)
            g = int(start_color[1] + (end_color[1] - start_color[1]) * factor)
            b = int(start_color[2] + (end_color[2] - start_color[2]) * factor)

            colored_text += f"\033[38;2;{r};{g};{b}m{char}"

        return colored_text + "\033[0m"


    print(BColors.OKGREEN    +r"===========================================================================")
    print(gradient_yellow_rgb(r"     ██╗ █████╗ ██████╗ ████████╗███████╗███╗   ██╗██████╗ ███████╗██████╗ ",0))
    print(gradient_yellow_rgb(r"     ██║██╔══██╗██╔══██╗╚══██╔══╝██╔════╝████╗  ██║██╔══██╗██╔════╝██╔══██╗",1))
    print(gradient_yellow_rgb(r"     ██║███████║██████╔╝   ██║   █████╗  ██╔██╗ ██║██║  ██║█████╗  ██████╔╝",2))
    print(gradient_yellow_rgb(r"██   ██║██╔══██║██╔══██╗   ██║   ██╔══╝  ██║╚██╗██║██║  ██║██╔══╝  ██╔══██╗",3))
    print(gradient_yellow_rgb(r"╚█████╔╝██║  ██║██║  ██║   ██║   ███████╗██║ ╚████║██████╔╝███████╗██║  ██║",4))
    print(gradient_yellow_rgb(r" ╚════╝ ╚═╝  ╚═╝╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═══╝╚═════╝ ╚══════╝╚═╝  ╚═╝",5))

    if os.path.getsize(Settings.LIST_PATH) == 0:
        print(BColors.OKGREEN + r"===========================================================================")
        print(BColors.WARNING + "⚠️服务器列表为空。您需要初始化服务器列表。")
        ifinitserver = input("初始化服务器列表？(Y/n)")
        if ifinitserver == "Y":
            Serverlistinitializer.initialize()
        elif ifinitserver == "n":
            print("您跳过了服务器列表初始化。您可以稍后手动进行初始化。")
        else:
            print("你想干嘛？")

def main_menu(current_server):
    while True:
        print(BColors.OKGREEN + "============== Jartender - A Simple Minecraft Server Manager ==============")
        print("1. 启动服务器")
        print("2. 管理服务器")
        print("3. Jartender 设置")
        print("0. 退出")

        choice = input("请选择操作 (输入对应数字): ").strip()

        if choice == "1":
            new_current_server = start_server_menu(current_server)
            current_server = new_current_server
        elif choice == "2":
            manage_server_menu(current_server)
        elif choice == "3":
            settings_menu()
        elif choice == "0":
            print("退出 Jartender..." + BColors.ENDC)
            break
        else:
            print("无效输入，请输入 0-3 之间的数字。")


def start_server_menu(current_server):
    """启动服务器的子菜单"""
    print("\n=== 启动服务器 ===")
    print(BColors.WARNING + "当前服务器:" + current_server + BColors.OKGREEN)
    print("1. 选择服务器核心")
    print("2. 直接启动服务器")
    print("3. 以 GUI 启动服务器")
    print("0. 返回主菜单")

    choice = input("请选择操作: ").strip()

    if choice == "1":
        server_list = Lister.load_server_list()
        current_server = Lister.display_servers(server_list)
        return current_server
    elif choice == "2":
        print("正在启动服务器...")
        config = Settings.load_config()
        ServerLauncher.launch(current_server, Settings.server_root(config), False)
    elif choice == "3":
        print("正在以 GUI 模式启动服务器...")
        config = Settings.load_config()
        ServerLauncher.launch(current_server, Settings.server_root(config), True)
    elif choice == "0":
        return
    else:
        print("无效输入，请重新选择。")


def manage_server_menu(current_server):
    """管理服务器的子菜单"""
    print("\n=== 管理服务器 ===")
    print(BColors.WARNING + "当前服务器:" + current_server + BColors.OKGREEN)
    print("1. 版本管理")
    print("2. 安装新的服务端")
    print("3. Plugins/Mods 管理")
    print("4. Worlds 管理")
    print("5. 服务器设置")
    print("6. 扫描并更新服务器列表")
    print("0. 返回主菜单")

    choice = input("请选择操作: ").strip()

    if choice == "1":
        print("进入版本管理...")
    elif choice == "2":
        print("安装新的服务端...")
        config = Settings.load_config()
        server_root = Settings.server_root(config)
        print(f"当前服务器目录{server_root}")
        ServerInstaller.run(server_root)
    elif choice == "3":
        print("进入 Plugins 管理...")
    elif choice == "4":
        print("进入 Worlds 管理...")
    elif choice == "5":
        print("进入服务器设置...")
    elif choice == "6":
        if "y" == input("确认扫描(y)"):
            print("开始扫描...")
            Serverlistinitializer.initialize()
        else:
            print("用户取消了扫描。")
    elif choice == "0":
        return
    else:
        print("无效输入，请重新选择。")


def settings_menu():
    """Jartender 设置菜单"""
    print("\n=== Jartender 设置 ===")
    print("1. 存放服务器路径")
    print("2. 网络设置")
    print("3. 关于 Jartender")
    print("4. Java虚拟机 管理")
    print("0. 返回主菜单")

    choice = input("请选择操作: ").strip()

    if choice == "1":
        print("进入存放服务器路径设置...WIP,目前不可用")
    elif choice == "2":
        print("进入全局设置...WIP,目前不可用")
    elif choice == "3":
        AboutJartender.about()
    elif choice == "4":
        javamgr.main_menu()
    elif choice == "0":
        return
    else:
        print("无效输入，请重新选择。")


if __name__ == "__main__":
    main_menu(current_server)
    print(BColors.ENDC)
