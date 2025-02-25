import json,os

from jartender import BColors

from modules import scanner,manifester

def read_server_path(config_path: str = "config.json") -> str:
    with open('config.json', 'r') as config_file:
        config_data = json.load(config_file)
        return config_data

def initialize():
    server_path = read_server_path()['serverpath']
    server_core_path = scanner.scan_core(server_path)
    result_list = []
    eta = 15 * len(server_core_path)
    print(f"\n这将花费一些时间，取决于您的服务端数量。过程可能弹出服务器gui。估计大约需要花费{eta}秒。")
    for folder_name, jar_file in server_core_path.items():
        # 构建JAR路径
        jar_path = os.path.join(server_path, folder_name, jar_file)
        try:
            # 获取manifest信息
            manifest_data = manifester.manifest(jar_path)

            # 构建结果字典
            server_info = {
                "server_name": folder_name,
                "jar_name": jar_file,
                "jar_path": jar_path,
                "minecraft_version": manifest_data.get("minecraft_version"),
                "server_type": manifest_data.get("server_type"),
                "loader_version": manifest_data.get("loader_version")
            }

            result_list.append(server_info)
            print(server_info)
            print(f"✅ 成功处理: {folder_name}")

        except Exception as e:
            print(f"❌ 处理 {folder_name} 时发生错误: {str(e)}")
            continue

    # 写入JSON文件
    try:
        with open('./list.json', 'w', encoding='utf-8') as f:
            json.dump(result_list, f, indent=4, ensure_ascii=False)
        print(f"\n{BColors.OKGREEN}数据已成功写入./list.json{BColors.ENDC}")
    except Exception as e:
        print(f"{BColors.FAIL}写入文件失败: {str(e)}{BColors.ENDC}")



if __name__ == "__main__":
    server_path = read_server_path()['serverpath']
    print(server_path)
    #print(scanner.scan_core(server_path))
