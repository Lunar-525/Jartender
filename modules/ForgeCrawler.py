import os
import math
import shutil
import requests
import time
# 全局变量存储选中的项目（与 FabricCrawler 一致）
selected_item = None


def forge_crawler(current_dir):
    terminal_width, _ = shutil.get_terminal_size()

    """
    Forge 安装流程占位：
    1. 获取版本信息（TODO: 调用官方 API）
    2. 选择 Minecraft 版本
    3. 选择 Forge Loader/Installer 版本
    4. 下载服务器 Jar
    5. 保存文件并返回路径
    """

    # TODO: 拉取 Forge 版本信息
    response = requests.get("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json")
    versions = response.json()

    # 1. 选择 Minecraft 版本
    game_versions = set()
    for key in versions["promos"].keys():
        mc_version = key.rsplit("-", 1)[0] 
        game_versions.add(mc_version)
    # 转换为列表并按版本号排序
    game_versions = sorted(
        game_versions,
        key=lambda v: [int(x) for x in v.split(".")],
        reverse=True  # 新版本在前
    )

    print(game_versions)
    #select_version("Minecraft", game_versions, terminal_width)
    #current_minecraft_version = selected_item["version"]
    time.sleep(100)

    
    # 2. 选择 Forge 版本（占位）
    if versions["loader"]:
        select_version("Forge", versions["loader"], terminal_width)
        current_forge_version = selected_item["version"]
    else:
        current_forge_version = None
        print("TODO: 未实现 Forge 版本获取")

    # 3. 下载服务器 Jar（占位）
    download_url = None  # TODO: 构造下载链接
    if current_minecraft_version and current_forge_version:
        print(f"TODO: 下载 Forge Server，MC {current_minecraft_version}, Forge {current_forge_version}")
    else:
        print("占位：缺少版本信息，无法下载。")
        return None

    # TODO: 下载并保存文件，与 FabricCrawler 逻辑类似
    # filepath = os.path.join(current_dir, filename)
    # return filepath, current_minecraft_version, current_forge_version


def select_version(version_type, versions, terminal_width):
    global selected_item

    # 根据版本类型调整显示
    if version_type == "Minecraft":
        item_prefix = ""
        get_name = lambda v: v["version"]
    else:
        # 为稳定版添加emoji标记
        get_name = lambda v: f"💡{v['version']}" if v.get("stable") else v["version"]

    # 计算适合的列数和每项的宽度
    max_item_length = max([len(get_name(v)) + 5 for v in versions], default=10)  # 加5是为了包含序号和间距
    cols = max(1, terminal_width // max_item_length)
    item_width = terminal_width // cols

    # 初始化分页
    page_size = 20  # 每页显示的项数
    current_page = 0
    total_pages = max(1, math.ceil(len(versions) / page_size))

    while True:
        os.system('cls' if os.name == 'nt' else 'clear')  # 清屏

        # 显示标题
        print(f"{version_type} 版本列表 (第 {current_page + 1}/{total_pages} 页):")

        # 计算当前页的项
        start_idx = current_page * page_size
        end_idx = min(start_idx + page_size, len(versions))
        page_items = versions[start_idx:end_idx]

        # 计算行数
        rows = math.ceil(len(page_items) / cols) if page_items else 0

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
    current_dir = os.getcwd()
    forge_crawler(r"/Users/Luna/Documents/GitHub/Jartender/Servers/test")