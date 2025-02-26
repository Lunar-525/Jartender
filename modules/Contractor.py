import os


def check_eula(server_dir):
    """检查 eula.txt 是否存在并解析其内容"""
    eula_path = os.path.join(server_dir, "eula.txt")
    try:
        print(f"📄 检查文件: {eula_path}")

        with open(eula_path, "r", encoding="utf-8") as file:
            content = file.read().strip().lower()  # 读取内容并转换为小写，避免大小写问题

        print(f"🔍 读取到的内容:\n{content}")

        if "eula=false" in content:
            if input("⚠️ 您未接受 EULA 协议，是否修改 eula.txt？(y/n) ").strip().lower() == "y":
                accept_eula(server_dir)
            else:
                print("⛔ 用户取消了操作。")
        elif "eula=true" in content:
            print("✅ 您已经同意 EULA 协议。")
        else:
            print("⚠️ eula.txt 内容异常，建议重新创建！")
            accept_eula(server_dir)

    except FileNotFoundError:
        print(f"❌ 未找到 eula.txt，将会创建新的 eula.txt")
        with open(eula_path, "w", encoding="utf-8") as file:
            file.write("eula=false\n")
        if input("⚠️ 您未接受 EULA 协议，是否修改 eula.txt？(y/n) ").strip().lower() == "y":
            accept_eula(server_dir)
        else:
            print("⛔ 用户取消了操作。")

    except Exception as e:
        print(f"❌ 读取 eula.txt 时出错: {e}")


def accept_eula(server_dir):
    """User accepted EULA"""
    eula_path = os.path.join(server_dir, "eula.txt")
    print(
        "\n💡 输入 'TRUE' 即代表您接受并同意 Mojang AB 和微软的 Minecraft 最终用户许可协议 (https://aka.ms/MinecraftEULA).")

    eula_input = input(
        "By typing 'TRUE' you are indicating your agreement to Mojang AB and Microsoft's EULA (https://aka.ms/MinecraftEULA): ").strip().upper()

    if eula_input == "TRUE":
        try:
            with open(eula_path, "w", encoding="utf-8") as file:
                file.write("eula=true\n")
            print(f"✅ 已成功设置 {eula_path} 为 eula=true")
        except Exception as e:
            print(f"❌ 创建 {eula_path} 时出错: {e}")
    else:
        print("⛔ 您拒绝接受 Mojang AB 和微软的 Minecraft 最终用户许可协议。")


if __name__ == "__main__":
    server_directory = r"C:\Users\tempusr\Documents\Jartender\Servers\1.21.4-mohist"
    check_eula(server_directory)
