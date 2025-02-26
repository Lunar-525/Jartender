import json,os

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

from modules import Scanner,Manifester

def read_server_path(config_path: str = "config.json") -> str:
    with open('./config.json', 'r') as config_file:
        config_data = json.load(config_file)
        return config_data
"""
1.从config文件获取serverpath
2.使用scanner.scan获取服务器核心列表。
3.for server_core_path,使用manifester.manifest获取服务器核心的信息。
4.构建字典传入json。
"""
def initialize():
    server_path = read_server_path()['serverpath']
    server_core_path = Scanner.scan_core(server_path)
    result_list = []
    eta = 15 * len(server_core_path)
    print(BColors.WARNING + f"\n这将花费一些时间，取决于您的服务端数量。过程可能弹出服务器gui。估计大约需要花费{eta}秒。\n" + BColors.ENDC)
    for folder_name, jar_file in server_core_path.items():
        # 构建服务器核心路径
        jar_path = os.path.join(server_path, folder_name, jar_file)
        try:
            #一开始我想叫它manifest，现在我是真懒得改了
            manifest_data = Manifester.manifest(jar_path)

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
            print(BColors.ENDC + f"{server_info}")
            print(BColors.OKGREEN + "✅ 成功处理: {folder_name}")

        except Exception as e:
            print(BColors.FAIL + f"❌ 处理 {folder_name} 时发生错误: {str(e)}" + BColors.OKGREEN)
            continue

    # 写入JSON文件
    try:
        with open('./list.json', 'w', encoding='utf-8') as f:
            json.dump(result_list, f, indent=4, ensure_ascii=False)
        print(BColors.OKGREEN + "✅数据已成功写入./list.json")
    except Exception as e:
        print(BColors.FAIL + f"❌写入文件失败: {str(e)}" + BColors.OKGREEN)



if __name__ == "__main__":
    server_path = read_server_path()['serverpath']
    print(server_path)
    #print(scanner.scan_core(server_path))
