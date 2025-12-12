import time
import requests
import os
import math
import json
import shutil


def fabric_crawler(current_dir):
    # 获取终端宽度
    terminal_width, _ = shutil.get_terminal_size()

    # 获取版本信息
    response = requests.get("https://meta.fabricmc.net/v2/versions")
    versions = response.json()

    # 1. 选择Minecraft版本
    game_versions = [v for v in versions["game"] if v["stable"]]
    select_version("Minecraft", game_versions, terminal_width)
    current_minecraft_version = selected_item["version"]

    # 2. 选择Fabric Loader版本
    loader_versions = versions["loader"]
    select_version("Fabric Loader", loader_versions, terminal_width)
    current_fabric_loader_version = selected_item["version"]

    # 3. 选择Installer版本
    installer_versions = versions["installer"]
    select_version("Installer", installer_versions, terminal_width)
    current_installer_version = selected_item["version"]

    # 清清爽爽，干干净净💀
    
    # 下载服务器Jar文件
    if current_minecraft_version and current_fabric_loader_version and current_installer_version:
        download_url = (
        f"https://meta.fabricmc.net/v2/versions/loader/"
        f"{current_minecraft_version}/{current_fabric_loader_version}/"
        f"{current_installer_version}/server/jar"
        )

        try:
            with requests.get(download_url, stream=True) as resp:
                print(f"\n正在下载Fabric Server: {download_url}")
                resp.raise_for_status()

                if "Content-Disposition" in resp.headers:
                    content_disposition = resp.headers["Content-Disposition"]
                    filename = content_disposition.split("filename=")[1].strip('"')
                else:
                    filename = (
                        f"fabric-server-mc.{current_minecraft_version}-"
                        f"loader.{current_fabric_loader_version}-"
                        f"launcher.{current_installer_version}.jar"
                    )

                filepath = os.path.join(current_dir, filename)
                with open(filepath, "wb") as file:
                    for chunk in resp.iter_content(chunk_size=8192):
                        if chunk:
                            file.write(chunk)

            print(f"\n下载完成! 文件已保存至: {filepath}")
            return filepath, current_minecraft_version, current_fabric_loader_version
        except Exception as e:
            print(f"下载失败: {e}")
            return None
    else:
        print("缺少版本信息，无法下载。")
        return None

# 全局变量存储选中的项目
selected_item = None


def select_version(version_type, versions, terminal_width):
    global selected_item

    # 根据版本类型调整显示
    if version_type == "Minecraft":
        item_prefix = ""
        get_name = lambda v: v["version"]
    else:
        # 为稳定版添加emoji标记
        get_name = lambda v: f"💡{v['version']}" if v["stable"] else v["version"]

    # 计算适合的列数和每项的宽度
    max_item_length = max([len(get_name(v)) + 5 for v in versions])  # 加5是为了包含序号和间距
    cols = max(1, terminal_width // max_item_length)
    item_width = terminal_width // cols

    # 初始化分页
    page_size = 20  # 每页显示的项数
    current_page = 0
    total_pages = math.ceil(len(versions) / page_size)

    while True:
        os.system('cls' if os.name == 'nt' else 'clear')  # 清屏

        # 显示标题
        print(f"{version_type} 版本列表 (第 {current_page + 1}/{total_pages} 页):")

        # 计算当前页的项
        start_idx = current_page * page_size
        end_idx = min(start_idx + page_size, len(versions))
        page_items = versions[start_idx:end_idx]

        # 计算行数
        rows = math.ceil(len(page_items) / cols)

        # 按列显示版本
        for i in range(rows):
            line = ""
            for j in range(cols):
                idx = i + j * rows
                if idx < len(page_items):
                    # 计算全局索引
                    global_idx = start_idx + idx
                    item_text = f"{global_idx + 1}. {get_name(page_items[idx])}"
                    line += item_text.ljust(item_width)
            print(line)

        # 显示导航选项
        nav_options = []
        if current_page > 0:
            nav_options.append("P-上一页")
        if current_page < total_pages - 1:
            nav_options.append("N-下一页")

        if nav_options:
            print("\n导航: " + ", ".join(nav_options))

        # 用户选择
        choice = input(f"\n请选择{version_type}版本 (输入序号，或导航命令): ").strip().upper()

        # 处理导航命令
        if choice == 'P' and current_page > 0:
            current_page -= 1
            continue
        elif choice == 'N' and current_page < total_pages - 1:
            current_page += 1
            continue

        # 处理版本选择
        try:
            idx = int(choice) - 1
            if 0 <= idx < len(versions):
                selected_item = versions[idx]
                print(f"已选择: {get_name(selected_item)}")
                break
            else:
                input(f"请输入1到{len(versions)}之间的数字，按Enter继续...")
        except ValueError:
            input("请输入有效的数字或导航命令，按Enter继续...")


if __name__ == "__main__":
    # 测试函数
    current_dir = os.getcwd()
    fabric_crawler(r"/Users/fanxuancheng/Documents/GitHub/Jartender/Servers/testfabric/")