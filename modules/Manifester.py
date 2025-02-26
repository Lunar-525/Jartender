# Manifester.py
import subprocess
import time
import os
import re
from threading import Thread
from typing import Dict, Optional

"""
由于各个核心标准也是群魔乱舞，此处实现方法更加抽象。
1.scanner传递每个核心的路径
2.for所有路径，逐个创建子进程，运行15秒，通过管道捕获日志，解析日志文本来获得版本信息。
3.return log_data.解析,传递serverlistinitializer.py
"""

class ServerManifest:
    def __init__(self):
        self.process: Optional[subprocess.Popen] = None
        self.output_lines: list = []

    def launch_java_process(self, jar_path: str, timeout: int = 15) -> None:
        """启动Java进程并捕获输出"""
        if not os.path.exists(jar_path):
            raise FileNotFoundError(f"JAR文件不存在: {jar_path}")

        target_dir = os.path.dirname(jar_path)
        command = ["java", "-jar", os.path.basename(jar_path),"-nogui"]

        try:
            self.process = subprocess.Popen(
                command,
                cwd=target_dir,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                universal_newlines=True
            )
        except Exception as e:
            raise RuntimeError(f"启动进程失败: {str(e)}") from e

        # 启动输出捕获线程
        output_thread = Thread(target=self._capture_output, args=(100,))
        output_thread.daemon = True
        output_thread.start()

        # 等待超时或进程结束
        start_time = time.time()
        while time.time() - start_time < timeout:
            if self.process.poll() is not None:  # 进程已退出
                break
            time.sleep(0.1)

        self._terminate_process()
        output_thread.join(timeout=0.5)

    def _capture_output(self, max_lines: int) -> None:
        """异步捕获输出"""
        if self.process is None or self.process.stdout is None:
            return

        for line in iter(self.process.stdout.readline, ''):
            self.output_lines.append(line.strip())
            if len(self.output_lines) >= max_lines:
                break

    def _terminate_process(self) -> None:
        """终止Java进程"""
        if self.process is None or self.process.poll() is not None:
            return

        try:
            if os.name == 'nt':
                os.system(f"taskkill /F /PID {self.process.pid}")
            else:
                self.process.terminate()
        except Exception as e:
            raise RuntimeError(f"终止进程失败: {str(e)}") from e

    def analyze_logs(self) -> Dict[str, Optional[str]]:
        """自动检测服务器类型并分析日志"""
        log_data = '\n'.join(self.output_lines)

        if 'net.fabricmc.loader' in log_data:
            return self._analyze_fabric(log_data)
        elif 'MinecraftForge' in log_data:
            return self._analyze_forge(log_data)
        elif 'org.bukkit.craftbukkit.Main' in log_data:
            return self._analyze_bukkits(log_data)
        elif 'Mohist' or ' ███╗   ███╗ ' in log_data:
            return self._analyze_mohist(log_data)

        return {
            "minecraft_version": None,
            "server_type": "Unknown",
            "loader_version": None
        }

    @staticmethod
    def _analyze_fabric(log_str: str) -> Dict[str, Optional[str]]:
        pattern = r"Loading Minecraft (\d+\.\d+\.\d+) with Fabric Loader (\d+\.\d+\.\d+)"
        match = re.search(pattern, log_str)
        return {
            "minecraft_version": match.group(1) if match else None,
            "server_type": "Fabric",
            "loader_version": match.group(2) if match else None
        }

    @staticmethod
    def _analyze_forge(log_str: str) -> Dict[str, Optional[str]]:
        mc_version = None
        forge_version = None

        version_match = re.search(r"for MC (\d+\.\d+\.\d+)", log_str)
        forge_match = re.search(r"MinecraftForge v(\d+\.\d+\.\d+)", log_str)

        if version_match:
            mc_version = version_match.group(1)
        if forge_match:
            forge_version = forge_match.group(1)

        return {
            "minecraft_version": mc_version,
            "server_type": "Forge",
            "loader_version": forge_version
        }
    @staticmethod
    def _analyze_bukkits(log_str: str) -> Dict[str, Optional[str]]:
        """
        分析 Bukkit 系服务端日志，支持 Purpur/Paper/Spogit 等变种

        支持的日志格式：
        1. Purpur 格式：[bootstrap] Loading Purpur 1.21.4-2399-HEAD@62cbd47 (...) for Minecraft 1.21.4
        2. DeerFolia 格式：[bootstrap] Loading DeerFolia 1.21.4-DEV-HEAD@0561727 1.21.4-178-main@636ae0c
        3. ...

        返回结构：
        {
            "minecraft_version": "1.21.4",
            "server_type": "Purpur",
            "loader_version": "2399-HEAD@62cbd47"
        }
        """

        patterns = [

            re.compile(
                r"Loading\s+"
                r"(?P<server_type>Purpur|DeerFolia)\s+"
                r"(?P<full_version>\d+\.\d+\.\d+-(?P<build>[^\s]+))"
                r".*for Minecraft (?P<mc_version>\d+\.\d+\.\d+)",
                re.IGNORECASE
            ),

            re.compile(
                r"Loading\s+"
                r"(?P<server_type>DeerFolia)\s+"
                r"(?P<full_version>\d+\.\d+\.\d+-[^\s]+)\s+"
                r"(?P<alt_version>\d+\.\d+\.\d+-[^\s]+)",
                re.IGNORECASE
            ),

            re.compile(
                r"Loading\s+"
                r"(?P<server_type>\w+)\s+"
                r"(?P<full_version>\d+\.\d+\.\d+-[^\s]+)",
                re.IGNORECASE
            )
        ]

        for line in log_str.split('\n'):
            for pattern in patterns:
                match = pattern.search(line)
                if match:
                    server_type = match.group("server_type")
                    result = {
                        "minecraft_version": None,
                        "server_type": server_type,
                        "loader_version": None
                    }

                    if pattern == patterns[0]:  # Purpur 格式
                        result["minecraft_version"] = match.group("mc_version")
                        result["loader_version"] = match.group("build")
                    elif pattern == patterns[1]:  # DeerFolia 双版本格式
                        mc_ver = match.group("full_version").split('-')[0]
                        result["minecraft_version"] = mc_ver
                        result["loader_version"] = f"{match.group('full_version')}+{match.group('alt_version')}"
                    else:
                        full_ver = match.group("full_version")
                        ver_parts = full_ver.split('-', 1)
                        result["minecraft_version"] = ver_parts[0]
                        result["loader_version"] = ver_parts[1] if len(ver_parts) > 1 else None

                    return result

        return {
            "minecraft_version": None,
            "server_type": "Bukkit",
            "loader_version": None
        }

    @staticmethod
    def _analyze_mohist(log_str: str) -> Dict[str, Optional[str]]:
        """
        解析 Mohist 服务器日志版本信息
        支持的日志格式示例：
        "Thanks for using Mohist - 1.20.1-923, Java(65.0) 21.0.5 PID: 46224"

        返回结构：
        {
            "minecraft_version": "1.20.1",
            "server_type": "Mohist",
            "loader_version": "923"
        }
        """
        # Match Mohist version 的正则表达式模式
        pattern = r"Mohist - (\d+\.\d+\.\d+)-(\d+)"

        # scan log
        for line in log_str.split('\n'):
            match = re.search(pattern, line)
            if match:
                return {
                    "minecraft_version": match.group(1),
                    "server_type": "Mohist",  # 固定值
                    "loader_version": match.group(2)
                }

        return {
            "minecraft_version": None,
            "server_type": "Mohist",
            "loader_version": None
        }



def manifest(jar_path: str, timeout: int = 15) -> Dict[str, Optional[str]]:
    """
    对外暴露的主接口
    :param jar_path: JAR文件完整路径
    :param timeout: 最大等待时间（秒）
    :return: 包含服务器信息的字典
    """
    try:
        manifestor = ServerManifest()
        manifestor.launch_java_process(jar_path, timeout)
        return manifestor.analyze_logs()
    except Exception as e:
        return {
            "minecraft_version": None,
            "server_type": "Error",
            "loader_version": str(e)
        }

def ping():
    print("pong!")

# 模块自测试
if __name__ == "__main__":
    test_jar = r"C:\Users\tempusr\Documents\Jartender\Servers\1.21.4-leaf\leaf-1.21.4.jar"
    result = manifest(test_jar)
    print("Test result:", result)