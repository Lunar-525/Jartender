# Jartender

![SET](assets/set.png "Windows, Linux, macOS (top to bottom)")

[中文](https://github.com/tucaoba2333/Jartender/blob/main/README.md) 在这里.

Jartender is a CLI tool for managing and operating Minecraft server cores. It provides features such as server scanning, installation, launching, and management, supporting Windows, macOS, and Linux operating systems.

## Key Features

- **Server Scanning** - Automatically scan and identify existing Minecraft server cores
- **Quick Launch** - One-click server launch with GUI and headless mode support
- **Auto Installation** - Automatic download and configuration for Fabric and Forge servers
- **EULA Management** - Automatically handle server EULA agreements
- **Java Management** - Automatically detect and manage Java runtime environments
- **Beautiful Interface** - Colorful terminal interface providing a great user experience

## Quick Start

### System Requirements

- Python 3.6 or higher
- Java Runtime Environment (JRE) or Java Development Kit (JDK)

### Usage

1. Clone:
```bash
git clone https://github.com/tucaoba2333/Jartender.git
cd Jartender
```

2. Run:
```bash
python3 jartender.py
```

On first run, you will be prompted to set the server storage directory (default is the `Servers` folder in the current directory).

## Feature Status

|                Planned Feature                | Implemented? | In Development? |
|:---------------------------------------------:|:------------:|:---------------:|
|     Retrieve existing server information      |      ?       |       ✅        |
|                  Modify EULA                  |      ?       |       ✅        |
|                 Start server                  |      ✅      |       ✅        |
|             Basic menu structure              |      ✅      |       ✅        |
|                 Manage server                 |      ❌      |       ❌        |
|      Download and auto-configure server       |✅(Fabric only)|       ✅        |
|                  Manage Java                  |      ❌      |       ✅        |
|            Multi-language support             |      ❌      |       ❌        |
|              Still thinking ( )               |      ❌      |       ❌        |

## ⚠️ Development Status ⚠️

<font color=red>**This tool is still in early development. Some features may be unstable or incomplete.**</font>

## Usage Guide

After running the program, you will see the main menu:

```
============== Jartender - A Simple Minecraft Server Manager ==============
1. Start Server
2. Manage Server
3. Jartender Settings
0. Exit
```

### Main Features

- **Start Server** - Select and launch configured Minecraft servers
- **Manage Server** - Manage server list, install new servers, etc.
- **Jartender Settings** - Configure server storage path, network settings, Java management, etc.

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).

## Contributing

Issues and Pull Requests are welcome!

__I'm looking for someone with TUI development experience. If you're interested, please contact me.__

---

**Note**: This project references code from [Prism Launcher](https://github.com/PrismLauncher/PrismLauncher) and [MultiMC](https://github.com/MultiMC/MultiMC5). Thanks to them.
