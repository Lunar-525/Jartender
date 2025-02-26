import os

from jartender import BColors


def scan_core(path: str) -> dict:
    print(f"hi, doing at {path}")
    """
    扫描指定路径下的服务器文件夹，识别并选择核心 JAR 文件
    实际上实现方法非常抽象。
    :param path: 服务器根目录路径
    :return: 包含服务器名称和对应核心 JAR 文件的字典
    """
    """
    1.listdir>current_server
    2.移除current_server列表内所有不endwith .jar的项目
    3.移除包含install的项目
    4.若只剩一个项目，就完成核心的识别。否则用户手动选择。
    """
    if not os.path.exists(path):
        raise FileNotFoundError(f"路径不存在: {path}")

    server_dict = {}
    scanned_folders = os.listdir(path)
    print(scanned_folders)

    if scanned_folders == []:
        print(f"❌没有于{path}找到服务器!")

    for folder in scanned_folders:
        folder_path = os.path.join(path, folder)
        if not os.path.isdir(folder_path):
            continue  # 跳过非目录项

        print(f"\n{BColors.OKBLUE}正在扫描文件夹: {folder}{BColors.ENDC}")
        current_server = os.listdir(folder_path)

        # 检查是否为有效的服务器目录
        is_mohist = 'libraries' in current_server and any(
            'mohist' in item.lower() and item.endswith('.jar')
            for item in current_server
        )

        if not is_mohist and not all(item in current_server for item in ['eula.txt', 'server.properties', 'libraries']):
            print(f"{BColors.WARNING}⚠️ 文件夹 {folder} 不是有效的服务器目录，跳过{BColors.ENDC}")
            continue

        # 过滤 JAR 文件
        jar_files = [
            item for item in current_server
            if item.endswith('.jar') and 'installer' not in item.lower()
        ]

        if not jar_files:
            print(f"{BColors.FAIL}❌ 文件夹 {folder} 中未找到有效的 JAR 文件{BColors.ENDC}")
            continue

        # 处理 Mohist 核心的特殊条件
        if is_mohist:
            mohist_jars = [jar for jar in jar_files if 'mohist' in jar.lower()]
            if mohist_jars:
                jar_files = mohist_jars  # 优先选择 Mohist 核心

        # 处理多个 JAR 文件的情况
        if len(jar_files) > 1:
            print(f"{BColors.FAIL}❌ 文件夹 {folder} 中存在多个 JAR 文件，请手动选择：{BColors.ENDC}")
            for idx, jar in enumerate(jar_files, start=1):
                print(f"{BColors.OKGREEN}{idx}. {jar}{BColors.ENDC}")

            while True:
                try:
                    choice = int(input(f"{BColors.WARNING}⚠️ 请输入序号选择核心 JAR 文件: {BColors.ENDC}"))
                    if 1 <= choice <= len(jar_files):
                        selected_jar = jar_files[choice - 1]
                        break
                    print(f"{BColors.WARNING}⚠️ 请输入 1 到 {len(jar_files)} 之间的数字{BColors.ENDC}")
                except ValueError:
                    print(f"{BColors.WARNING}⚠️ 请输入有效的数字{BColors.ENDC}")
        else:
            selected_jar = jar_files[0]

        print(f"{BColors.OKGREEN}✅ 已选择 {selected_jar} 为 {folder} 的核心 JAR 文件{BColors.ENDC}")
        server_dict[folder] = selected_jar

    return server_dict



if __name__ == "__main__":
    path = r"C:\Users\tempusr\Documents\Jartender\Servers"
    try:
        result = scan_core(path)
        print(result)
        print(f"\n{BColors.HEADER}扫描结果：{BColors.ENDC}")
        for server, jar in result.items():
            print(f"{BColors.OKBLUE}{server}: {jar}{BColors.ENDC}")
    except Exception as e:
        print(f"{BColors.FAIL}❌ 发生错误: {e}{BColors.ENDC}")