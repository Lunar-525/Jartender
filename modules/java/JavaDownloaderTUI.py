"""
JavaDownloaderTUI - JVM 下载器 TUI 界面模块
使用 prompt_toolkit 构建的交互式终端界面
"""

from prompt_toolkit import Application
from prompt_toolkit.layout import Layout, HSplit, VSplit, Window, FormattedTextControl
from prompt_toolkit.key_binding import KeyBindings
from prompt_toolkit.styles import Style
from prompt_toolkit.widgets import Frame, Box
from prompt_toolkit.layout.dimension import D


class JavaDownloaderTUI:
    """JVM 下载器 TUI 主类"""
    
    def __init__(self):
        """初始化 TUI 界面"""
        self.setup_style()
        self.setup_keybindings()
        self.setup_layout()
        self.app = None
    
    def setup_style(self):
        """设置样式主题"""
        self.style = Style.from_dict({
            # 背景色：亮白色
            'bg': 'bg:#FFFFFF',
            'main': 'bg:#FFFFFF fg:#000000',
            
            # 辅助色：#CF3A2B (红色调)
            'accent': 'bg:#CF3A2B fg:#FFFFFF',
            'accent-text': 'fg:#CF3A2B',
            'accent-border': 'fg:#CF3A2B',
            
            # 标题样式
            'title': 'bg:#CF3A2B fg:#FFFFFF bold',
            'subtitle': 'fg:#CF3A2B bold',
            
            # 边框和分隔线
            'border': 'fg:#CF3A2B',
            'frame': 'bg:#FFFFFF',
            
            # 文本样式
            'text': 'fg:#000000',
            'text-bold': 'fg:#000000 bold',
            'text-dim': 'fg:#666666',
            
            # 按钮和交互元素
            'button': 'bg:#CF3A2B fg:#FFFFFF',
            'button.focused': 'bg:#E04A3A fg:#FFFFFF bold',
            
            # 状态栏
            'status-bar': 'bg:#CF3A2B fg:#FFFFFF',
            'status-key': 'bg:#CF3A2B fg:#FFFFFF bold',
        })
    
    def setup_keybindings(self):
        """设置键盘绑定"""
        self.kb = KeyBindings()
        
        @self.kb.add('c-c')
        @self.kb.add('c-q')
        def _(event):
            """退出应用 (Ctrl+C 或 Ctrl+Q)"""
            event.app.exit()
        
        @self.kb.add('q')
        def _(event):
            """退出应用 (按 q 键)"""
            event.app.exit()
    
    def setup_layout(self):
        """设置布局结构"""
        # 标题栏
        title_window = Window(
            content=FormattedTextControl(
                text='   ☕ JVM 下载器 - Java Downloader   ',
            ),
            height=D.exact(1),
            style='title',
            align='center'
        )
        
        # 主内容区域（占位，后续添加内容）
        content_window = Window(
            content=FormattedTextControl(
                text=self.get_welcome_text(),
            ),
            style='main',
        )
        
        # 状态栏
        status_bar = Window(
            content=FormattedTextControl(
                text='  [q] 退出  [↑↓] 选择  [Enter] 确认  ',
            ),
            height=D.exact(1),
            style='status-bar',
        )
        
        # 主布局容器
        self.root_container = HSplit([
            title_window,
            Window(height=D.exact(1), char='─', style='accent-border'),  # 分隔线
            content_window,
            Window(height=D.exact(1), char='─', style='accent-border'),  # 分隔线
            status_bar,
        ])
        
        self.layout = Layout(self.root_container)
    
    def get_welcome_text(self):
        """获取欢迎文本"""
        return '\n'.join([
            '',
            '  欢迎使用 JVM 下载器！',
            '',
            '  此工具可以帮助您下载和管理 Java 虚拟机 (JVM)。',
            '',
            '  背景设置完成，组件开发中...',
            '',
            '  按 q 或 Ctrl+C 退出',
            '',
        ])
    
    def run(self):
        """运行 TUI 应用"""
        self.app = Application(
            layout=self.layout,
            key_bindings=self.kb,
            style=self.style,
            full_screen=True,
            mouse_support=True,
        )
        
        self.app.run()


def main():
    """主入口函数"""
    tui = JavaDownloaderTUI()
    tui.run()


if __name__ == '__main__':
    main()

