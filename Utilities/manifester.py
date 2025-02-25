# manifester.py
import subprocess
import time
import os
import re
from threading import Thread
from typing import Dict, Optional


class ServerManifest:
    def __init__(self):
        self.process: Optional[subprocess.Popen] = None
        self.output_lines: list = []

    def launch_java_process(self, jar_path: str, timeout: int = 15) -> None:
        """启动Java进程并捕获输出"""
        if not os.path.exists(jar_path):
            raise FileNotFoundError(f"JAR文件不存在: {jar_path}")

        target_dir = os.path.dirname(jar_path)
        command = ["java", "-jar", os.path.basename(jar_path)]

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

        # 优先通过JAR文件名检测类型
        jar_name = self.process.args[2].lower() if self.process else ""
        if 'fabric' in jar_name:
            return self._analyze_fabric(log_data)
        elif 'forge' in jar_name:
            return self._analyze_forge(log_data)

        # 通过日志内容检测类型
        if 'Fabric Loader' in log_data:
            return self._analyze_fabric(log_data)
        elif 'MinecraftForge' in log_data:
            return self._analyze_forge(log_data)

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

        # 匹配两种可能的日志格式
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


# 模块自测试
if __name__ == "__main__":
    test_jar = r"C:\Users\tempusr\Documents\Jartender\Servers\1.21.4-Forge\forge-1.21.4-54.1.0-shim.jar"
    result = manifest(test_jar)
    print("测试结果:", result)