def main_menu():
    while True:
        #if
        print("\n=== Jartender - A Simple Minecraft Server Manager ===")
        print("1. 启动服务器")
        print("2. 管理服务器")
        print("3. Jartender 设置")
        print("0. 退出")

        choice = input("请选择操作 (输入对应数字): ").strip()

        if choice == "1":
            start_server_menu()
        elif choice == "2":
            manage_server_menu()
        elif choice == "3":
            settings_menu()
        elif choice == "0":
            print("退出 Jartender...")
            break
        else:
            print("无效输入，请输入 0-3 之间的数字。")


def start_server_menu():
    """启动服务器的子菜单"""
    print("\n=== 启动服务器 ===")
    print("1. 选择服务器核心")
    print("2. 直接启动服务器")
    print("3. 以 GUI 启动服务器")
    print("0. 返回主菜单")

    choice = input("请选择操作: ").strip()

    if choice == "1":
        print("（这里可以添加服务器核心选择逻辑）")
    elif choice == "2":
        print("正在启动服务器...")
    elif choice == "3":
        print("正在以 GUI 模式启动服务器...")
    elif choice == "0":
        return
    else:
        print("无效输入，请重新选择。")


def manage_server_menu():
    """管理服务器的子菜单"""
    print("\n=== 管理服务器 ===")
    print("1. 版本管理")
    print("2. Mods 管理")
    print("3. Plugins 管理")
    print("4. Worlds 管理")
    print("5. 服务器设置")
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
        print("关于Jartender...")
    elif choice == "0":
        return
    else:
        print("无效输入，请重新选择。")


if __name__ == "__main__":
    main_menu()
