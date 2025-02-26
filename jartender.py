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

if __name__ == "__main__":
    # add
    current_dir = Path(__file__).parent
    modules_dir = current_dir / "modules"
    sys.path.insert(0, str(modules_dir))

    from modules import Manifester, Contractor, Serverlistinitializer, Lister, ServerLauncher, AboutJartender

    current_directory = os.getcwd()
    config_path = os.path.join(current_directory, 'config.json')

    current_server = "None"

    if not os.path.isfile(config_path):
        print(BColors.FAIL + "config.json 与 list.json 文件不存在。将进行初始化。")
        try:
            f = open("config.json", "x")
            fh = open("list.json", "x")

        except:
            print("Error: 没有找到文件或读取文件失败")

        else:
            if not os.path.exists("./Servers"):
                print("将使用Servers作为默认服务器目录。您可以在设置中更改默认服务器目录。")
                os.mkdir("./Servers")
            current_dir = os.getcwd()

            config = {
                "serverpath": os.path.join(current_dir, "Servers")  # 指定服务器绝对路径
            }
            with open("config.json", "w", encoding="utf-8") as f:
                json.dump(config, f, indent=4)

            print(f"config.json 已初始化，serverpath 设置为 {config['serverpath']},应用将会关闭来完成初始化，请您手动重新启动。")

            exit()


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

    if os.path.getsize("list.json") == 0:
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
        ServerLauncher.launch(current_server,current_dir,False)
    elif choice == "3":
        print("正在以 GUI 模式启动服务器...")
        ServerLauncher.launch(current_server, current_dir, True)
    elif choice == "0":
        return
    else:
        print("无效输入，请重新选择。")


def manage_server_menu(current_server):
    """管理服务器的子菜单"""
    print("\n=== 管理服务器 ===")
    print(BColors.WARNING + "当前服务器:" + current_server + BColors.OKGREEN)
    print("1. 版本管理")
    print("2. Mods 管理")
    print("3. Plugins 管理")
    print("4. Worlds 管理")
    print("5. 服务器设置")
    print("6. 扫描并更新服务器列表")
    print("0. 返回主菜单")

    choice = input("请选择操作: ").strip()

    if choice == "1":
        print("进入版本管理...")
    elif choice == "2":
        print("进入 Mods 管理...")
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
    print("0. 返回主菜单")

    choice = input("请选择操作: ").strip()

    if choice == "1":
        print("进入存放服务器路径设置...")
    elif choice == "2":
        print("进入网络设置...")
    elif choice == "3":
        AboutJartender.about()

    elif choice == "0":
        return
    else:
        print("无效输入，请重新选择。")


if __name__ == "__main__":
    main_menu(current_server)
    print(BColors.ENDC)
