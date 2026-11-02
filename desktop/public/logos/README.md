# 服务端标识（corner logo）

`desktop/public/logos/` 存放实例列表与当前实例卡片的角标，每个 Minecraft 服务端核心一张，用于在界面上快速认出实例类型。

角标按设计稿拼贴处理：**透明底、放大（实例列表约 96 px、当前实例框约 104 px）、按类型旋转固定角度、从左下角出血并被卡片裁切、略降不透明度和卡片融为一体，同时不接触卡片顶边**。位图统一整理为 128×128 的 PNG：等比缩放居中，并把原图的深色或白色方块底抠成透明；矢量图保留为单文件 SVG，任意尺寸都清晰。旋转角度由 [`InstanceLogo.tsx`](../../src/InstanceLogo.tsx) 中的 `instanceTilt()` 按类型名哈希得到，同一类型每次渲染角度一致。

`instanceLogo()` 负责把后端 `server_type` 归一化（忽略大小写和符号，`bungee` → `bungeecord`、`neoforged` → `neoforge`、`quilt` → `fabric`）后映射到文件；CraftBukkit / Spigot 属于同一家族，`bukkit` 与 `spigot` 共用 `spigot.png`。无法识别的类型回退到 `generic.svg`。

| 文件 | 服务端核心 | 来源 | 处理说明 |
| --- | --- | --- | --- |
| [`vanilla.png`](vanilla.png) | Vanilla（原版） | 自绘 | 像素画草方块，未使用 Mojang 素材 |
| [`fabric.png`](fabric.png) | Fabric | [logo.png](https://fabricmc.net/assets/logo.png) | 取官方织机图形，最近邻放大保持像素格 |
| [`forge.png`](forge.png) | Forge | [favicon-32x32.png](https://files.minecraftforge.net/static/images/favicon-32x32.png) | 按亮度取 alpha 抠底，铁砧着 Forge 铜色 |
| [`neoforge.png`](neoforge.png) | NeoForge | [favicon.ico](https://neoforged.net/favicon.ico) | 先裁到狐狸头部，再把灰绿色底纹抠掉 |
| [`paper.png`](paper.png) | Paper | [256x.png](https://papermc.io/assets/logo/256x.png) | 等比缩放，保留透明底 |
| [`purpur.svg`](purpur.svg) | Purpur | [purpur.svg](https://purpurmc.org/images/purpur.svg) | 唯一改动：`stroke-width` 由 1.8 加宽到 4 |
| [`folia.png`](folia.png) | Folia | [folia.png](https://raw.githubusercontent.com/PaperMC/Folia/HEAD/folia.png) | 裁到羽毛图形，去掉字标 |
| [`velocity.svg`](velocity.svg) | Velocity | [velocitypowered.com](https://velocitypowered.com) | 取出内联 SVG sprite 的 `brand/velocity`，填充改为浅灰 `#e8eef2` 适配深色底 |
| [`bungeecord.svg`](bungeecord.svg) | BungeeCord | 自绘 | 紫色链条标识 |
| [`spigot.png`](spigot.png) | Bukkit / Spigot | [favicon.ico](https://static.spigotmc.org/favicon.ico) | 取 16×16 像素龙头图形（原字标版太小不可读），最近邻放大 |
| [`geyser.png`](geyser.png) | Geyser | [github.com/GeyserMC](https://github.com/GeyserMC) | 组织头像，等比缩放 |
| [`leaves.png`](leaves.png) | Leaves | [favicon.ico](https://leavesmc.org/favicon.ico) | 深色方块底抠成透明，只留叶子 |
| [`mohist.png`](mohist.png) | Mohist | [favicon.ico](https://mohistmc.com/favicon.ico) | 齿轮图标，等比缩放 |
| [`arclight.png`](arclight.png) | Arclight | [arclightlogo.jpg](https://raw.githubusercontent.com/IzzelAliz/Arclight/HEAD/.github/arclightlogo.jpg) | JPG 只有渐变天空底，用区域生长法从边缘抠底后再裁掉地面光晕 |
| [`banner.png`](banner.png) | Banner | 自绘 | 像素画旗杆 + 旗帜 |
| [`catserver.png`](catserver.png) | CatServer | 自绘 | 像素画猫脸 |
| [`installing.png`](installing.png) | 「安装中」状态 | 自绘 | 琥珀色沙漏，与安装中的状态色一致 |
| [`generic.svg`](generic.svg) | 未识别核心 | 自绘（JarTender 图标） | 兜底标识，1024×1024 viewBox |

## 自绘资源

[`vanilla.png`](vanilla.png)、[`bungeecord.svg`](bungeecord.svg)、[`banner.png`](banner.png)、[`catserver.png`](catserver.png)、[`installing.png`](installing.png) 和 [`generic.svg`](generic.svg) 由本项目绘制，不含上游素材，随仓库许可分发。它们刻意不复刻对应项目的官方标识：原版用草方块、BungeeCord 用链条、Banner 用旗帜、CatServer 用猫脸，`generic.svg` 直接复用 JarTender 应用图标。

## 授权与商标

表中的第三方标识仅用于标识对应的服务端软件，便于用户区分实例类型；它们不代表该项目的认可或背书，也不在仓库的 GPL-3.0 许可范围内，各标识的版权和商标权仍归各自项目所有。超出「标识对应软件」的使用（例如官方宣传物料）请先查阅该项目的品牌政策。

若某个项目要求移除其标识，删除对应文件并从 [`InstanceLogo.tsx`](../../src/InstanceLogo.tsx) 的 `LOGOS` 表中删掉该条目即可，角标会自动回退到 `generic.svg`，无需改动其他代码。

上述来源链接于 2026-11-02 用 `curl -sSI` 逐一校验，均返回 200；`velocitypowered.com` 现为 302 跳转到 PaperMC 的 Velocity 页面，内联 sprite 中的 `brand/velocity` 图形仍可取得。
