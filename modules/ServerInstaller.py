import os,json
import subprocess
import sys
import time
from typing import Optional

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

from modules import FabricCrawler, Contractor


def read_server_path(config_path: str = "config.json") -> str:
    with open('config.json', 'r') as config_file:
        config_data = json.load(config_file)
        return config_data

ServersPath = read_server_path()['serverpath']
print(ServersPath)

def run(current_dir):
    print("1. 🧶安装Fabric Server")
    print("2. 🔨安装Forge Server")
    print("更多选项仍在开发...")
    choice = input("请选择操作:")
    if choice == "1":
        install_fabric(current_dir)
    elif choice == "2":
        install_forge(current_dir)
    elif choice == "3":
        print("敬请期待")

def nametag(current_dir):
    name = input("名称:")
    new_dir = os.path.join(current_dir, name)
    print(new_dir)
    if os.path.exists(new_dir):
        print(f"路径 '{new_dir}' 已经存在!")
        nametag(current_dir)
    else:
        os.makedirs(new_dir)
        print(f"创建{name}成功。")
        return(new_dir,name)



def install_fabric(current_dir: str) -> None:
    """
    安装并初始化Fabric服务器

    :param current_dir: 当前工作目录路径
    """
    new_dir = nametag(current_dir)
    name = new_dir[1]
    server_jar = FabricCrawler.fabric_crawler(new_dir[0])[0]
    print(server_jar)
    os.chdir(new_dir[0])
    print("开始进行Fabric服务器初始化...")

    # 阶段1: 首次运行以生成配置文件
    try:
        print("正在执行首次初始化运行...")
        process = subprocess.Popen(
            ["java", "-jar", server_jar],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
            universal_newlines=True
        )

        # 实时监控输出流
        eula_detected = False
        while True:
            line = process.stdout.readline()
            if not line:  # 进程已退出
                break

            print(line.strip())  # 实时显示日志

            # 检测 EULA 提示
            if "Failed to load eula.txt" in line:
                print("检测到 EULA 协议提示，终止进程...")
                eula_detected = True
                _terminate_process(process)
                break

        # 检查退出状态
        return_code = process.poll()
        if return_code is None:  # 进程未退出则强制终止
            _terminate_process(process)
        elif "returned non-zero exit status 128" in return_code:
            print("服务器已经停止运行。")
        elif return_code != 0 and not eula_detected:
            print(f"首次运行异常退出，返回码: {return_code}")
            sys.exit(1)

    except Exception as e:
        print(BColors.WARNING + f"执行首次运行时可能发生错误: {str(e)}")
        if_stop = input("是否停止(y)？大部分情况下您可以忽略。" + BColors.OKGREEN)
        if if_stop == "y":
            sys.exit(1)

    # 阶段2: 同意EULA协议
    try:
        print("正在同意EULA协议...")
        Contractor.accept_eula(new_dir[0])
    except Exception as e:
        print(f"同意EULA时发生错误: {str(e)}")
        sys.exit(1)

    # 阶段3: 再次运行并监控日志
    try:
        print("正在启动服务器进行世界生成...")
        process = subprocess.Popen(
            ["java", "-jar", server_jar, "--nogui"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
            universal_newlines=True
        )

        # 实时监控日志输出
        world_created = False
        while True:
            line = process.stdout.readline()
            if not line:
                break

            print(line.strip())  # 实时显示日志

            # 检测到世界生成后终止进程
            if "[main/INFO]: No existing world data, creating new world" in line:
                print("检测到Fabric服务器已初始化完成，终止进程...")
                world_created = True
                _terminate_process(process)
                break

        # 确认进程已终止
        process.wait(timeout=10)

        if not world_created:
            print("未检测到世界生成，可能初始化未完成！")
            sys.exit(1)

    except Exception as e:
        print(f"世界生成阶段发生错误: {str(e)}")
        sys.exit(1)

    print("✅ Fabric服务器初始化完成！🎉🎈🎊")


def _terminate_process(process: subprocess.Popen) -> None:
    """跨平台终止进程"""
    try:
        if os.name == 'nt':
            # Windows系统
            subprocess.run(["taskkill", "/F", "/T", "/PID", str(process.pid)], check=True)
        else:
            # Unix系统
            process.terminate()

        # 等待进程结束
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
    except Exception as e:
        print(f"终止进程失败: {str(e)}")
        raise


def install_forge(current_dir):
    1

if __name__ == "__main__":
    nametag("")