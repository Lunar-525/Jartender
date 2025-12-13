# Jartender

![SET](assets/set.png "Windows, Linux, macOS (由上往下)")

[English](https://github.com/tucaoba2333/Jartender/blob/main/README_EN.md) here.

Jartender 是一款~~简洁易用~~的 CLI 工具，用于管理和操作 Minecraft 服务器核心。它提供了服务器扫描、安装、启动和管理等功能，支持 Windows、macOS 和 Linux 三大主流操作系统。

## 主要特性

- **服务器扫描** - 自动扫描并识别现有的 Minecraft 服务器核心
- **快速启动** - 一键启动服务器，支持 GUI 和无 GUI 模式
- **自动安装** - 支持 Fabric 和 Forge 服务器的自动下载和配置
- **EULA 管理** - 自动处理服务器 EULA 协议
- **Java 管理** - 自动检测和管理 Java 运行时环境
- **美观界面** - 彩色终端界面，提供良好的用户体验

## 🚀 快速开始

### 系统要求

- Python 3.6 或更高版本
- Java 运行时环境（JRE）或 Java 开发工具包（JDK）

### 使用方法

1. 克隆：
```bash
git clone https://github.com/tucaoba2333/Jartender.git
cd Jartender
```

2. 运行：
```bash
python3 jartender.py
```

首次运行时会提示您设置服务器存储目录（默认为当前目录下的 `Servers` 文件夹）。

## 📋 功能状态

|          计划中的功能           | 是否实现？ | 是否处于开发？ |
|:-------------------------:|:-----:|:-------:|
|  检索现有的服务端，给出信息   |   ？   |    ✅    |
|          修改 EULA          |   ？   |    ✅    |
|           启动服务端           |   ✅   |    ✅    |
|          基本菜单结构           |   ✅   |    ✅    |
|           管理服务器           |   ❌   |    ❌    |
|        下载并自动配置服务器         |   ✅(仅Fabric)   |    ✅    |
|          管理 Java          |   ❌   |    ✅    |
|           多语言支持           |   ❌   |    ❌    |
|          我还在想（ ）          |   ❌   |    ❌    |

## ⚠️ 注意 ⚠️

<font color=red>**此工具仍处于早期开发阶段，大部分功能都不稳定或尚未完成。**</font>

## 使用方法

运行程序后，您将看到主菜单：

```
============== Jartender - A Simple Minecraft Server Manager ==============
1. 启动服务器
2. 管理服务器
3. Jartender 设置
0. 退出
```

### 主要功能说明

- **启动服务器** - 选择并启动已配置的 Minecraft 服务器
- **管理服务器** - 管理服务器列表、安装新服务器等
- **Jartender 设置** - 配置服务器存储路径、网络设置、Java 管理等

## 许可证

本项目采用 [GNU General Public License v3.0](LICENSE) 许可证。

## 贡献

欢迎提交 Issue 和 Pull Request！

__我正在寻找一位具有TUI开发经验的人，如果你感兴趣请联系我。__

---

**注意**: 本项目部分代码参考了 [Prism Launcher](https://github.com/PrismLauncher/PrismLauncher) 和 [MultiMC](https://github.com/MultiMC/MultiMC5) 的实现，在此表示感谢。
