# Not Bad, my first time try to write a TUI app with Textual.

from textual.app import App, ComposeResult
from textual.containers import Horizontal, Vertical
from textual.widgets import (
    Button,
    Checkbox,
    Input,
    Label,
    ListItem,
    ListView,
    Static,
)
from textual.screen import Screen
from textual_sortable_datatable import SortableDataTable


# --- 颜色主题定义 ---
class Theme:
    """颜色主题类，集中管理所有颜色定义"""
    
    # 主题模式: "dark" 或 "light"
    MODE = "dark"
    
    # 品牌色（固定，不随主题变化）
    brand_mojang = "#F0313B"    # Mojang 红
    brand_adoptium = "#E3186A"  # Adoptium 粉
    brand_azul = "#418CA6"      # Azul 蓝
    
    # 强调色
    accent = "#70D6FF"          # 主强调色


    @classmethod
    def _get_colors(cls):
        """根据 MODE 返回对应的颜色配置"""
        if cls.MODE == "dark":
            return {
                "bg_status_ready" : "#253328",
                "bg_status" : "#220809",
                "bg_primary": "#1e1e1e",
                "bg_secondary": "#252526",
                "bg_hover": "#2a2d2e",
                "bg_button": "#3c3c3c",
                "bg_button_hover": "#505050",
                "text_primary": "#e0e0e0",
                "text_contrast": "#1e1e1e",
                "border_dark": "#111",
                "border_light": "#666",
            }
            
        else:  # light mode
            return {
                "bg_status_ready" : "#9bf0ae",
                "bg_status" : "#ff7d82",
                "bg_primary": "#ffffff",
                "bg_secondary": "#f5f5f5",
                "bg_hover": "#e8e8e8",
                "bg_button": "#e0e0e0",
                "bg_button_hover": "#d0d0d0",
                "text_primary": "#1e1e1e",
                "text_contrast": "#ffffff",
                "border_dark": "#cccccc",
                "border_light": "#aaaaaa",
            }
    @classmethod
    @property
    def bg_status_ready(cls): return cls._get_colors()["bg_status_ready"]

    @classmethod
    @property
    def bg_status(cls): return cls._get_colors()["bg_status"]

    @classmethod
    @property
    def bg_primary(cls): return cls._get_colors()["bg_primary"]
    
    @classmethod
    @property
    def bg_secondary(cls): return cls._get_colors()["bg_secondary"]
    
    @classmethod
    @property
    def bg_hover(cls): return cls._get_colors()["bg_hover"]
    
    @classmethod
    @property
    def bg_button(cls): return cls._get_colors()["bg_button"]
    
    @classmethod
    @property
    def bg_button_hover(cls): return cls._get_colors()["bg_button_hover"]
    
    @classmethod
    @property
    def text_primary(cls): return cls._get_colors()["text_primary"]
    
    @classmethod
    @property
    def text_contrast(cls): return cls._get_colors()["text_contrast"]
    
    @classmethod
    @property
    def border_dark(cls): return cls._get_colors()["border_dark"]
    
    @classmethod
    @property
    def border_light(cls): return cls._get_colors()["border_light"]


# --- CSS 样式定义 ---
TCSS = f"""
Screen {{
    background: {Theme.bg_primary};
    color: {Theme.text_primary};
}}

/* 侧边栏样式 */
#sidebar {{
    dock: left;
    width: 18%;
    background: {Theme.bg_secondary};
    height: 100%;
    border-right: solid {Theme.border_dark};
}}

SidebarItem {{
    height: 3;
    background: {Theme.bg_secondary};
    padding: 1;
    content-align: left middle;
    border-left: solid transparent;
}}

SidebarItem:hover {{
    background: {Theme.bg_hover};
}}

.brand-mojang {{ color: {Theme.brand_mojang}; }}
.brand-adoptium {{ color: {Theme.brand_adoptium}; }}
.brand-azul {{ color: {Theme.brand_azul}; }}

/* 选中状态高亮 */
SidebarItem.active {{
    background: {Theme.accent};
    color: {Theme.text_contrast}; 
    text-style: bold;
}}

/* 主内容区域 */
#main-content {{
    height: 100%;
    padding: 1 2;
}}

#header-title {{
    text-style: bold;
    color: {Theme.accent};
    margin-bottom: 1;
}}

/* 分栏布局 */
#lists-container {{
    height: 1fr;
}}

/* 左侧 Major Version */
#major-version-col {{
    width: 30%;
    height: 100%;
    margin-right: 1;
}}

/* 右侧 Specific Version */
#version-details-col {{
    width: 70%;
    height: 100%;
}}

/* 表格和列表样式调整 */
SortableDataTable {{
    background: {Theme.bg_primary};
    border: solid {Theme.border_light};
    height: 1fr;
}}

/* DataTable 选中行高亮 */
SortableDataTable > .datatable--cursor {{
    background: {Theme.accent};
    color: {Theme.text_contrast};
    text-style: bold;
}}

ListView {{
    background: {Theme.bg_primary};
    border: solid {Theme.border_light};
    height: 1fr;
}}

ListItem {{
    padding: 0 1;
}}

/* ListView 选中项高亮 */
ListView > ListItem.--highlight {{
    background: {Theme.accent};
    color: {Theme.text_contrast};
    text-style: bold;
}}

ListView:focus > ListItem.--highlight {{
    background: {Theme.accent};
    color: {Theme.text_contrast};
    text-style: bold;
}}

ListItem:hover {{
    background: {Theme.bg_hover};
}}

ListItem.-active {{
    background: {Theme.accent};
    color: {Theme.text_contrast};
    text-style: bold;
}}

/* 底部搜索框 */
.search-input {{
    dock: bottom;
    height: 3;
    border: solid {Theme.border_light};
    background: {Theme.bg_primary};
    margin-top: 0;
}}

/* 底部按钮栏 */
#footer-bar {{
    dock: bottom;
    height: 4;
    background: {Theme.bg_primary};
    padding-top: 1;
}}

/* 重新调整底部布局结构 */
#footer-left {{
    width: 50%;
    height: 100%;
    align: left middle;
}}

#footer-right {{
    width: 50%;
    height: 100%;
    align: right middle;
}}

Button {{
    min-width: 12;
    margin-right: 1;
    background: {Theme.bg_button};
    border: none;
    height: 3;
}}

Button:hover {{
    background: {Theme.bg_button_hover};
}}

.btn-primary {{
    background: {Theme.bg_button};
}}

/* RECOMMENDED */
Checkbox {{
    background: {Theme.bg_button};
    margin-left: 0;
    height: 3;
    content-align: center middle;
}}

/* 底部状态栏 */
#status-bar {{
    dock: bottom;
    height: 1;
    background: {Theme.bg_status};
}}

#status-bar-left {{
    width: 1fr;
    height: 1;
    padding: 0 1;
    content-align: left middle;
}}

#status-bar-right {{
    width: auto;
    height: 1;
    padding: 0 1;
    background: {Theme.bg_status_ready};
    content-align: right middle;
}}
"""

class SidebarItem(Static):
    """自定义侧边栏项目"""
    def __init__(self, label: str, icon: str, brand_class: str, is_active: bool = False):
        # 构建 CSS 类字符串
        css_classes = brand_class
        if is_active:
            css_classes += " active"
        
        # 使用 classes 参数 (复数) 而不是 add_classes 方法
        super().__init__(f"{icon}  {label}", classes=css_classes)

class InstallJavaScreen(Screen):
    def compose(self) -> ComposeResult:
        yield Horizontal(
            Static("Install Java - Jartender", id="status-bar-left"),
            Static("READY", id="status-bar-right"),
            id="status-bar"
        )
        yield Horizontal(
            Vertical(
                SidebarItem("Mojang", "M", "brand-mojang"),
                SidebarItem("Adoptium", "A", "brand-adoptium"),
                SidebarItem("Azul Zulu", "A", "brand-azul", is_active=True),
                id="sidebar"
            ),
            Vertical(
                Label("Azul Zulu", id="header-title"),
                Horizontal(
                    Vertical(
                        Label("Major Version"),
                        ListView(
                            *[ListItem(Label(f"Java {i}")) for i in range(25, 7, -1)]
                        ),
                        # 修复点：将 class_="search-input" 改为 classes="search-input"
                        Input(placeholder="Search", classes="search-input"),
                        id="major-version-col"
                    ),
                    Vertical(
                        Label("Version"),
                        SortableDataTable(zebra_stripes=True, cursor_type="row"),
                        # 修复点：将 class_="search-input" 改为 classes="search-input"
                        Input(placeholder="Search", classes="search-input"),
                        id="version-details-col"
                    ),
                    id="lists-container"
                ),
                Horizontal(
                    Horizontal(
                        Button("Refresh"),
                        Checkbox("Recommended", value=False),
                        id="footer-left"
                    ),
                    Horizontal(
                        Button("Download", variant="primary"),
                        Button("Cancel"),
                        id="footer-right"
                    ),
                    id="footer-bar"
                ),
                id="main-content"
            )
        )

    def on_mount(self) -> None:
        table = self.query_one(SortableDataTable)
        table.add_columns("Version", "Java Name", "Released", "Type")
        table.add_row("17.0.17", "azul_zulu_jre17.0.17", "10/4/25", "jre")
        table.add_row("17.0.2", "azul_zulu_jre17.0.2", "1/8/22", "jre")

    def on_list_view_highlighted(self, event: ListView.Highlighted) -> None:
        """当 ListView 高亮项变化时触发"""
        # 移除所有 ListItem 的 -active 类
        for item in self.query("ListItem"):
            item.remove_class("-active")
        # 给当前高亮项添加 -active 类
        if event.item:
            event.item.add_class("-active")

class JartenderAPP(App):
    CSS = TCSS
    TITLE = "Install Java - Jartender"

    def on_mount(self) -> None:
        self.push_screen(InstallJavaScreen())

if __name__ == "__main__":
    app = JartenderAPP()
    app.run()