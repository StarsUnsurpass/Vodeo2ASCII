# Video2ASCII (视频转 ASCII 播放器)

一个富有创意的 Rust 命令行工具，能将视频文件实时转换为彩色的 ASCII 字符画动画并在终端中播放。

[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)](https://github.com/StarsUnsurpass/Vodeo2ASCII)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

## ✨ 核心功能

*   **双模式渲染**：
    *   **像素模式 (Pixel Art)**：使用“半块字符” (Half-Block, `▀`) 技术，实现双倍垂直分辨率，画面细腻，还原度极高（接近低分辨率 LED 屏效果）。
    *   **字符模式 (ASCII Art)**：经典的字符画风格，使用 `.:-=+*#%@` 等字符根据亮度进行渲染，充满复古极客感。
*   **全彩显示**：支持“TrueColor” (24-bit) 色彩，完美还原视频原色。
*   **自动适配**：智能检测终端窗口大小，自动缩放视频以保持正确的长宽比。
*   **交互式体验**：
    *   自动扫描当前目录下的视频文件。
    *   支持键盘上下键选择视频。
    *   支持手动输入路径或直接**拖拽文件**进终端播放。
*   **高性能**：使用 Rust 编写，针对终端渲染进行了深度优化（差异化渲染、缓冲区复用），播放流畅。

## 🛠️ 环境要求

在运行本程序之前，请确保您的电脑上安装了 **FFmpeg**。

### Windows
1.  下载 FFmpeg：[gyan.dev/ffmpeg/builds](https://www.gyan.dev/ffmpeg/builds/) (推荐下载 release-essentials.zip)。
2.  解压后，将 `bin` 文件夹里的 `ffmpeg.exe` 和 `ffprobe.exe` 复制到**本程序的根目录下**（和 `Cargo.toml` 在一起）。
3.  或者，将 FFmpeg 的 `bin` 目录添加到系统环境变量 PATH 中。

### macOS
```bash
brew install ffmpeg
```

### Linux
```bash
sudo apt install ffmpeg
```

## 🚀 如何运行

确保您已安装 Rust 环境。

1.  **克隆项目**
    ```bash
    git clone https://github.com/StarsUnsurpass/Vodeo2ASCII.git
    cd Vodeo2ASCII
    ```

2.  **编译并运行** (推荐使用 release 模式以获得最佳流畅度)
    ```bash
    cargo run --release
    ```

3.  **操作指南**
    *   **选择视频**：使用 `↑` `↓` 键选择，`Enter` 确认。
    *   **手动输入**：选择列表底部的 `[ Manual Input ]` 选项，然后输入路径或拖入文件。
    *   **选择风格**：在弹出的菜单中选择 `Pixel Art` 或 `ASCII Art`。
    *   **退出播放**：按 `q` 或 `Esc` 键。

## ⚙️ 常见问题

*   **报错 "program not found" 或 "Failed to run ffprobe"**：
    请检查是否已将 `ffmpeg.exe` 和 `ffprobe.exe` 放在项目根目录下，或者是否正确配置了环境变量。
*   **画面撕裂或闪烁**：
    建议使用支持 GPU 加速的现代终端模拟器，如 **Windows Terminal**、**Alacritty**、**Kitty** 或 **WezTerm**。
*   **画面比例不对**：
    程序默认终端字体的宽高比约为 1:2。

---

**项目地址**: [https://github.com/StarsUnsurpass/Vodeo2ASCII](https://github.com/StarsUnsurpass/Vodeo2ASCII)  
**作者主页**: [StarsUnsurpass](https://github.com/StarsUnsurpass)  
**当前版本**: v0.1.0

## 📝 开源协议

MIT License