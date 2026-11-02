# 随包字体清单

`desktop/public/fonts/` 下的字体全部随应用离线分发，逐个记录来源与许可，便于审计。

| 文件 | 字体 | 用途 | 许可 | 嵌入 / 再分发 | 上游 |
| --- | --- | --- | --- | --- | --- |
| `Marcellus-Regular.ttf` | Marcellus | 文字按钮、英文说明、弹窗标题、`.about` | SIL OFL 1.1（见 `Marcellus-OFL.txt`） | 允许，含商业使用与随应用嵌入（`fsType = 0`） | [Google Fonts](https://fonts.google.com/specimen/Marcellus) · [源码](https://github.com/google/fonts/tree/main/ofl/marcellus) |
| `BoutiqueBitmap9x9_3D.ttf` | BoutiqueBitmap9x9 3D | 像素标题、版本号 | 免费软件，允许商用及再分发；衍生品须同许可（见 `OFL.txt`） | 允许 | [BoutiqueBitmap9x9](https://github.com/scott0107000/BoutiqueBitmap9x9) |

## 注意事项

- **Marcellus 含 Reserved Font Name "Marcellus"**。OFL 允许打包、嵌入和再分发，但若修改字形后发布衍生字体，必须改名，不能继续使用 "Marcellus"。本仓库只按原样使用，未做修改。分发时需保留 `Marcellus-OFL.txt`。
- `OFL.txt` 只覆盖像素字体（BoutiqueBitmap9x9、M+ BITMAP FONTS、BestTen-DOT 等组成部分），不含 Marcellus，后者许可见 `Marcellus-OFL.txt`。
- **Branch 已移除**。该字体为 Sans & Sons 的商业零售字体，官方仅 Business License（$1,499）起允许 "Embed font files in App/Game/Server"，且任何档位都禁止将字体文件本身再分发，无法随本 GPL-3.0 项目公开分发。原 `Branch.otf` 不再位于构建路径内。
- 新增字体时请同时提供许可文本，并在上表登记：许可名称、是否允许嵌入、上游地址。
