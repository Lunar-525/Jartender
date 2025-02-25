import os


def accept_eula(server_dir):

    eula_path = os.path.join(server_dir, "eula.txt")
    print(r"输入“TRUE”即代表您接受并同意Mojang AB 和微软的 Minecraft 最终用户许可协议(https://aka.ms/MinecraftEULA).")
    eula_input = input(r"By typing 'TRUE' you are indicating your agreement to Mojang AB and Microsoft's EULA (https://aka.ms/MinecraftEULA).")

    if eula_input == "TRUE":
        try:
            os.path.exists(eula_path)
            os.remove(eula_path)
            print(f"✅ 已删除 {eula_path}")
            with open(eula_path, "w", encoding="utf-8") as file:
                    file.write("eula=true\n")
            print(f"⚠️ 文件 {eula_path} 不存在，无需删除")

        except Exception as e:
            print(f"❌ 删除 eula.txt 时出错: {e}，将会创建新的eula.txt")

            with open(eula_path, "w", encoding="utf-8") as file:
                file.write("eula=true\n")

            print(f"✅ 已成功设置 {eula_path} 为 eula=true")
    else:
        print("您拒绝接受Mojang AB 和微软的 Minecraft 最终用户许可协议。")

if __name__ == "__main__":
    server_directory = r"C:\Users\tempusr\Documents\Jartender\Servers\1.21.4"  # 你的服务器路径
    accept_eula(server_directory)
