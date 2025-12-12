from codecs import latin_1_decode
import os
import math
from re import L
import shutil
import requests
import time
# 全局变量存储选中的项目（与 FabricCrawler 一致）
selected_item = None


def forge_crawler(current_dir):
    # 获取终端宽度
    terminal_width, _ = shutil.get_terminal_size()

    response = requests.get("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json")
    versions = response.json()

    # 1. 选择 Minecraft 版本
    game_versions = set()
    for key in versions["promos"].keys():
        mc_version = key.rsplit("-", 1)[0] 
        game_versions.add(mc_version)
    # 转换为列表,排序
    print(game_versions)
    game_versions = sorted(
        game_versions,
        key=lambda v: [int(x) for x in v.split(".")], reverse=True
    )

    select_version("Minecraft", game_versions, terminal_width)
    current_minecraft_version = selected_item

    # 2. 选择 Forge Mod Loader版本
    response = requests.get("https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json")
    FMLversions = response.json()
    response = requests.get("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json")
    promo = response.json()

    loader_versions = FMLversions.get(current_minecraft_version, [])
    loader_versions = list(reversed(loader_versions))

    # 标记推荐版本，用于💡————傻逼啊，Fabric一个api全部给明白的，Forge得request三次。
    promos = promo.get("promos", {})
    recommended_key = f"{current_minecraft_version}-recommended"
    latest_key = f"{current_minecraft_version}-latest"
    highlight_version = promos.get(recommended_key) or promos.get(latest_key)

    select_version("Forge Loader", loader_versions, terminal_width, highlight_version)
    current_forge_version = (
        selected_item.get("version") if isinstance(selected_item, dict) else selected_item
    )

    # 3. 下载Installer Jar
    if current_minecraft_version and current_forge_version:
        download_url = (
            f"https://maven.minecraftforge.net/net/minecraftforge/forge/"
            f"{current_forge_version}/forge-{current_forge_version}-installer.jar"
        )
        filename = f"forge-{current_forge_version}-installer.jar"
        filepath = os.path.join(current_dir, filename)

        try:
            with requests.get(download_url, stream=True) as resp:
                print(f"\n正在下载Forge Installer: {download_url}")
                resp.raise_for_status()
                with open(filepath, "wb") as file:
                    for chunk in resp.iter_content(chunk_size=8192):
                        if chunk:
                            file.write(chunk)
                print(f"\n下载完成! 文件已保存至: {filepath}")
            return filepath, current_minecraft_version, current_forge_version
        except Exception as e:
            print(f"下载失败: {e}")
            return None
    else:
        print("缺少版本信息，无法下载。")
        return None


def select_version(version_type, versions, terminal_width, highlight_version=None):
    global selected_item

    # 根据版本类型调整显示 - 同时支持字符串和字典格式的版本
    def get_name(v):
        if isinstance(v, str):
            # 检查是否需要高亮显示（仅在显示时添加💡，不修改原始字符串）
            if highlight_version and (v.endswith(f"-{highlight_version}") or v == highlight_version):
                return f"💡{v}"
            return v
        # 字典格式：为稳定版添加emoji标记
        return f"💡{v['version']}" if v.get("stable") else v["version"]

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
    # 在此处设定代理
    import os
    import requests

    # 可选: 若需自动全局设置代理，可使用如下方式（根据需要取消/设置代理地址）
    # os.environ['HTTP_PROXY'] = 'http://127.0.0.1:7897'
    # os.environ['HTTPS_PROXY'] = 'http://127.0.0.1:7897'

    # 可选: 若需requests单独设置代理，可仿如下格式传递proxies参数
    # proxies = {
    #     "http": "http://127.0.0.1:7897",
    #     "https": "http://127.0.0.1:7897"
    # }
    # 示例:
    # resp = requests.get("https://example.com", proxies=proxies)
    current_dir = os.getcwd()
    forge_crawler(r"/Users/fanxuancheng/Documents/GitHub/Jartender/Servers/test")
