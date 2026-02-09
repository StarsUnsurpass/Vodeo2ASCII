use anyhow::{Context, Result};
use chrono::Local;
use crossterm::{
    event::{Event, KeyCode, KeyEventKind},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use glob::glob;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, BorderType, Clear, LineGauge, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::{
    fmt::Write,
    io::{self, Read, Write as IoWrite},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

#[derive(Debug, Clone, Copy, PartialEq)]
enum RenderMode {
    PixelArt,
    AsciiArt,
}

impl std::fmt::Display for RenderMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderMode::PixelArt => write!(f, "像素艺术 (半块字符 - 高保真)"),
            RenderMode::AsciiArt => write!(f, "ASCII 艺术 (经典字符模式)"),
        }
    }
}

struct App {
    files: Vec<PathBuf>,
    list_state: ListState,
    render_mode: RenderMode,
    system: System,
    should_quit: bool,
    video_metadata: String,
    show_mode_popup: bool,
    mode_list_state: ListState,
    show_input_popup: bool,
    input_buffer: String,
}

impl App {
    fn new() -> Result<Self> {
        let mut files = Vec::new();
        let patterns = ["*.mp4", "*.mkv", "*.avi", "*.mov", "*.flv", "*.webm", "*.MP4"];
        for pattern in patterns {
            if let Ok(paths) = glob(pattern) {
                for entry in paths {
                    if let Ok(path) = entry {
                        files.push(path);
                    }
                }
            }
        }
        files.sort();

        let mut system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        system.refresh_all();

        let mut list_state = ListState::default();
        if !files.is_empty() {
            list_state.select(Some(0));
        }

        let mut mode_list_state = ListState::default();
        mode_list_state.select(Some(0));

        Ok(Self {
            files,
            list_state,
            render_mode: RenderMode::PixelArt,
            system,
            should_quit: false,
            video_metadata: String::from("请选择一个视频文件以查看详情。"),
            show_mode_popup: false,
            mode_list_state,
            show_input_popup: false,
            input_buffer: String::new(),
        })
    }

    fn on_tick(&mut self) {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        self.update_metadata();
    }

    fn update_metadata(&mut self) {
        if let Some(idx) = self.list_state.selected() {
             if let Some(path) = self.files.get(idx) {
                 match probe_video(path) {
                    Ok(info) => {
                        let size_mb = std::fs::metadata(path).map(|m| m.len() as f64 / 1024.0 / 1024.0).unwrap_or(0.0);
                        let duration_str = format!("{:02}:{:02}:{:02}", 
                            (info.duration / 3600.0).floor(),
                            ((info.duration % 3600.0) / 60.0).floor(),
                            (info.duration % 60.0).floor()
                        );
                        let bitrate_str = if let Some(br) = info.bitrate {
                            format!("{:.2} Mbps", br as f64 / 1000.0 / 1000.0)
                        } else {
                            "N/A".to_string()
                        };
                        
                        self.video_metadata = format!(
                            "分辨率: {}x{}\n帧率: {:.2} FPS\n时长: {}\n大小: {:.2} MB\n码率: {}\n视频编码: {}\n音频编码: {}", 
                            info.width, info.height, info.fps,
                            duration_str,
                            size_mb,
                            bitrate_str,
                            info.video_codec,
                            info.audio_codec.as_deref().unwrap_or("无")
                        );
                    },
                    Err(_) => {
                        self.video_metadata = "无法解析视频元数据".to_string();
                    }
                 }
             }
        } else {
            self.video_metadata = "未选择文件".to_string();
        }
    }

    fn next_item(&mut self) {
        if self.show_mode_popup {
            let i = match self.mode_list_state.selected() {
                Some(i) => if i >= 1 { 0 } else { i + 1 },
                None => 0,
            };
            self.mode_list_state.select(Some(i));
        } else if !self.show_input_popup {
            if self.files.is_empty() { return; }
            let i = match self.list_state.selected() {
                Some(i) => {
                    if i >= self.files.len() - 1 {
                        0
                    } else {
                        i + 1
                    }
                }
                None => 0,
            };
            self.list_state.select(Some(i));
        }
    }

    fn previous_item(&mut self) {
        if self.show_mode_popup {
            let i = match self.mode_list_state.selected() {
                Some(i) => if i == 0 { 1 } else { i - 1 },
                None => 0,
            };
            self.mode_list_state.select(Some(i));
        } else if !self.show_input_popup {
            if self.files.is_empty() { return; }
            let i = match self.list_state.selected() {
                Some(i) => {
                    if i == 0 {
                        self.files.len() - 1
                    } else {
                        i - 1
                    }
                }
                None => 0,
            };
            self.list_state.select(Some(i));
        }
    }
    
    fn select_mode(&mut self) {
        if let Some(idx) = self.mode_list_state.selected() {
            self.render_mode = match idx {
                0 => RenderMode::PixelArt,
                1 => RenderMode::AsciiArt,
                _ => RenderMode::PixelArt,
            };
        }
        self.show_mode_popup = false;
    }
    
    fn submit_input(&mut self) {
        let path_str = self.input_buffer.trim().trim_matches('"').trim_matches('\'').to_string();
        if !path_str.is_empty() {
             let path = PathBuf::from(&path_str);
             if path.exists() {
                 self.files.push(path);
                 self.list_state.select(Some(self.files.len() - 1));
             }
        }
        self.input_buffer.clear();
        self.show_input_popup = false;
    }
}

fn main() -> Result<()> {
    // Setup terminal
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create App
    let mut app = App::new()?;

    // Main Loop
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind == KeyEventKind::Press {
                    if app.show_input_popup {
                        match key.code {
                            KeyCode::Enter => app.submit_input(),
                            KeyCode::Esc => {
                                app.show_input_popup = false;
                                app.input_buffer.clear();
                            },
                            KeyCode::Backspace => {
                                app.input_buffer.pop();
                            },
                            KeyCode::Char(c) => {
                                app.input_buffer.push(c);
                            },
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                if app.show_mode_popup {
                                    app.show_mode_popup = false;
                                } else {
                                    app.should_quit = true;
                                }
                            },
                            KeyCode::Char('j') | KeyCode::Down => app.next_item(),
                            KeyCode::Char('k') | KeyCode::Up => app.previous_item(),
                            KeyCode::Char('m') | KeyCode::Char('M') | KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Tab | KeyCode::BackTab => {
                                 app.show_mode_popup = !app.show_mode_popup;
                                 let idx = match app.render_mode {
                                     RenderMode::PixelArt => 0,
                                     RenderMode::AsciiArt => 1,
                                 };
                                 app.mode_list_state.select(Some(idx));
                            },
                            KeyCode::Char('o') | KeyCode::Char('O') => {
                                app.show_input_popup = true;
                            },
                            KeyCode::Enter => {
                                if app.show_mode_popup {
                                    app.select_mode();
                                } else {
                                    if let Some(idx) = app.list_state.selected() {
                                        if let Some(path) = app.files.get(idx).cloned() {
                                            terminal::disable_raw_mode()?;
                                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                            let _ = play_video(&path, app.render_mode);
                                            terminal::enable_raw_mode()?;
                                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                                            terminal.clear()?;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    
    Ok(())
}

// Custom widget for Gradient Gauge
struct GradientGauge {
    ratio: f64,
    start_color: (u8, u8, u8),
    end_color: (u8, u8, u8),
    label: Option<String>,
}

impl GradientGauge {
    fn new(ratio: f64, start: (u8, u8, u8), end: (u8, u8, u8)) -> Self {
        Self { ratio, start_color: start, end_color: end, label: None }
    }
    
    fn label(mut self, label: String) -> Self {
        self.label = Some(label);
        self
    }
}

impl Widget for GradientGauge {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 1 || area.height < 1 { return; }
        
        let width = area.width as usize;
        let filled_width = (self.ratio * width as f64).round() as usize;
        
        for i in 0..width {
            if i < filled_width {
                // Interpolate color
                let t = i as f32 / width.max(1) as f32;
                let r = (self.start_color.0 as f32 * (1.0 - t) + self.end_color.0 as f32 * t) as u8;
                let g = (self.start_color.1 as f32 * (1.0 - t) + self.end_color.1 as f32 * t) as u8;
                let b = (self.start_color.2 as f32 * (1.0 - t) + self.end_color.2 as f32 * t) as u8;
                
                buf.get_mut(area.x + i as u16, area.y)
                    .set_char('█') // Full block
                    .set_fg(Color::Rgb(r, g, b));
            } else {
                buf.get_mut(area.x + i as u16, area.y)
                    .set_char('░') // Light shade for empty
                    .set_fg(Color::DarkGray);
            }
        }
        
        if let Some(label) = self.label {
            let label_len = label.chars().count() as u16;
             // Center label if possible, or left align
            let x = area.x; // Just draw at start for simplicity or center
            // Simple overlay would require calculating center and rendering spans again.
            // For now, let's keep it simple: Just draw the bar. Label can be separate.
        }
    }
}

// Helper to generate a gradient of colors for a span of text
fn get_gradient_text(text: &str, start_color: (u8, u8, u8), end_color: (u8, u8, u8)) -> Line<'static> {
    let mut spans = Vec::new();
    let len = text.chars().count();
    
    for (i, c) in text.chars().enumerate() {
        let t = i as f32 / len.max(1) as f32;
        let r = (start_color.0 as f32 * (1.0 - t) + end_color.0 as f32 * t) as u8;
        let g = (start_color.1 as f32 * (1.0 - t) + end_color.1 as f32 * t) as u8;
        let b = (start_color.2 as f32 * (1.0 - t) + end_color.2 as f32 * t) as u8;
        
        spans.push(Span::styled(
            c.to_string(),
            Style::default().fg(Color::Rgb(r, g, b)),
        ));
    }
    Line::from(spans)
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    // 1. Header with Gradient
    let header_text = get_gradient_text(" 视频转字符画播放器 Vodeo2ASCII v0.1.0 ", (0, 255, 255), (255, 0, 255));
    let time_str = Local::now().format("%H:%M:%S").to_string();
    let header_content = Line::from(vec![
        header_text.spans.into_iter().collect::<Vec<_>>(), 
        vec![Span::raw(format!(" | {}", time_str)).style(Style::default().fg(Color::DarkGray))]
    ].concat());
    
    let header = Paragraph::new(header_content)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Cyan)));
    f.render_widget(header, chunks[0]);

    // 2. Main Content
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // File List
            Constraint::Percentage(60), // Details & Stats
        ])
        .split(chunks[1]);

    // Left: File List
    let files: Vec<ListItem> = app
        .files
        .iter()
        .map(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let icon = match path.extension().and_then(|e| e.to_str()) {
                Some("mp4") | Some("MP4") => "🎥 ",
                Some("mkv") => "🎞️ ",
                Some("avi") => "📼 ",
                _ => "📄 ",
            };
            // Style file items
             ListItem::new(Line::from(vec![
                 Span::styled(icon, Style::default().fg(Color::Blue)), 
                 Span::raw(name)
             ]))
        })
        .collect();

    // highlight selection with gradient effect (simulated by bold + bright color)
    let files_list = List::new(files)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" 视频文件列表 ")
            .border_style(Style::default().fg(Color::Blue))) // Blue border for active look
        .highlight_style(Style::default().bg(Color::Rgb(30, 30, 60)).add_modifier(Modifier::BOLD))
        .highlight_symbol(" ➤ ");
        
    f.render_stateful_widget(files_list, main_chunks[0], &mut app.list_state);

    // Right: Details + Stats
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // Details
            Constraint::Percentage(50), // Stats
        ])
        .split(main_chunks[1]);

    // Video Details (Dimmed logic if not active, but here we keep it clean)
    let details_text = Text::from(app.video_metadata.as_str());
    let details = Paragraph::new(details_text)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" 视频详情 ")
            .border_style(Style::default().fg(Color::Magenta))) // Different color
        .style(Style::default().fg(Color::White)); // Bright text
    f.render_widget(details, right_chunks[0]);

    // System Stats (Modern Gauges)
    let stats_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Label CPU
            Constraint::Length(1), // Gauge CPU
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Label Mem
            Constraint::Length(1), // Gauge Mem
        ])
        .margin(1)
        .split(right_chunks[1]);
        
    let stats_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" 系统状态 ")
        .border_style(Style::default().fg(Color::Green));
    f.render_widget(stats_block, right_chunks[1]);

    // CPU
    let cpu_usage = app.system.global_cpu_usage();
    f.render_widget(Paragraph::new(format!("CPU 使用率: {:.1}%", cpu_usage)).style(Style::default().fg(Color::LightCyan)), stats_chunks[0]);
    
    let cpu_gauge = GradientGauge::new(
        cpu_usage as f64 / 100.0,
        (0, 255, 0), // Green
        (255, 0, 0)  // Red
    );
    f.render_widget(cpu_gauge, stats_chunks[1]);

    // Memory
    let total_mem = app.system.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let used_mem = app.system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    f.render_widget(Paragraph::new(format!("内存使用率: {:.1} GB / {:.1} GB", used_mem, total_mem)).style(Style::default().fg(Color::LightMagenta)), stats_chunks[3]);

    let mem_gauge = GradientGauge::new(
        used_mem / total_mem,
        (0, 255, 255), // Cyan
        (255, 0, 255)  // Magenta
    );
    f.render_widget(mem_gauge, stats_chunks[4]);

    // Footer
    let footer_text = " [↑/↓]: 导航 | [回车]: 播放/确认 | [M/S/Tab]: 切换模式 | [O]: 打开文件 | [Q/Esc]: 退出/返回 ";
    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::DarkGray)))
        .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[2]);

    // Popup for Mode Selection
    if app.show_mode_popup {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area); // Clear background
        
        // Gradient border for popup
        let block = Block::default()
            .title(" 选择渲染模式 ")
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(Color::Cyan)); // Dark blue bg
        f.render_widget(block.clone(), area);

        let modes = vec![
            ListItem::new(Line::from(vec![Span::styled(" 🎨 ", Style::default()), Span::raw("像素艺术 (半块字符 - 高保真)")])),
            ListItem::new(Line::from(vec![Span::styled(" 🔢 ", Style::default()), Span::raw("ASCII 艺术 (经典字符模式)")])),
        ];
        
        let list = List::new(modes)
            .block(Block::default().borders(Borders::NONE))
            .highlight_style(Style::default().bg(Color::Rgb(50, 50, 100)).add_modifier(Modifier::BOLD)) // Subtle highlighting
            .highlight_symbol(" >> ");
        
        let inner_area = block.inner(area);
        f.render_stateful_widget(list, inner_area, &mut app.mode_list_state);
    }
    
    // Popup for File Input
    if app.show_input_popup {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area);
        
        let block = Block::default()
            .title(" 手动输入文件路径 ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(Color::Yellow));
        f.render_widget(block.clone(), area);
        
        let inner_area = block.inner(area);
        
        let input_text = vec![
            Line::from("请输入视频文件的完整路径 (支持拖拽):").style(Style::default().fg(Color::Gray)),
            Line::from(""),
            Line::from(app.input_buffer.as_str()).style(Style::default().fg(Color::White).add_modifier(Modifier::UNDERLINED)),
        ];
        
        let p = Paragraph::new(input_text).wrap(Wrap { trim: false }); 
        f.render_widget(p, inner_area);
    }
}

// Helper to center the popup
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

// Reuse existing logic, slightly adapted to not fail on missing inquiry
fn play_video(video_path: &Path, mode: RenderMode) -> Result<()> {
    let info = probe_video(video_path)?;
    let (orig_w, orig_h) = (info.width, info.height);
    let (term_w, term_h) = terminal::size()?;
    
    // Determine processing resolution
    let (target_width, target_height) = match mode {
        RenderMode::PixelArt => {
             // STRATEGY: Half-Block Rendering (▀)
            let effective_term_w = term_w as u32;
            let effective_term_h = (term_h as u32) * 2; 
            
            let video_aspect = orig_w as f32 / orig_h as f32;
            let term_aspect = effective_term_w as f32 / effective_term_h as f32;

            let (mut w, mut h) = if video_aspect > term_aspect {
                let h = effective_term_w as f32 / video_aspect;
                (effective_term_w, h as u32)
            } else {
                let w = effective_term_h as f32 * video_aspect;
                (w as u32, effective_term_h)
            };
            
             // Ensure even and non-zero
            w = (w / 2) * 2;
            h = (h / 2) * 2;
            if w == 0 { w = 2; }
            if h == 0 { h = 2; }
            (w, h)
        },
        RenderMode::AsciiArt => {
            let char_aspect = 0.5; 
            let video_aspect = orig_w as f32 / orig_h as f32;
            
            let mut w = term_w as u32;
            let mut h = (w as f32 / video_aspect * char_aspect) as u32;

            if h > term_h as u32 {
                h = term_h as u32;
                w = (h as f32 * video_aspect / char_aspect) as u32;
            }
            
            // Ensure even and non-zero
            w = (w / 2) * 2;
            h = (h / 2) * 2;
            if w == 0 { w = 2; }
            if h == 0 { h = 2; }
            (w, h)
        }
    };

    let frame_size = (target_width * target_height * 3) as usize;

    let ffmpeg_cmd = get_command_path("ffmpeg");
    let mut child = Command::new(&ffmpeg_cmd)
        .arg("-re") 
        .arg("-i")
        .arg(video_path)
        .arg("-vf")
        .arg(format!("scale={}:{}", target_width, target_height))
        .arg("-vcodec")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgb24")
        .arg("-f")
        .arg("image2pipe")
        .arg("-") 
        .stdout(Stdio::piped())
        .stderr(Stdio::null()) 
        .spawn()
        .context("Failed to spawn ffmpeg")?;

    let mut stdout = child.stdout.take().context("Failed to open stdout")?;
    let mut buffer = vec![0u8; frame_size];

    terminal::enable_raw_mode()?;
    let mut stdout_term = std::io::stdout();
    execute!(stdout_term, EnterAlternateScreen, crossterm::cursor::Hide)?;

    let mut render_buffer = String::with_capacity((target_width * target_height * 30) as usize);
    let ascii_chars = b" .:-=+*#%@";

    let result = (|| -> Result<()> {
        loop {
            if let Err(_) = stdout.read_exact(&mut buffer) {
                break; 
            }

            let img = image::RgbImage::from_raw(target_width, target_height, buffer.clone())
                .context("Failed to create image from buffer")?;

            render_buffer.clear();
            render_buffer.push_str("\x1b[H"); 
            
            let mut last_fg: Option<(u8, u8, u8)> = None;
            let mut last_bg: Option<(u8, u8, u8)> = None;

            // Centering logic
            let display_height = match mode {
                RenderMode::PixelArt => target_height / 2,
                RenderMode::AsciiArt => target_height,
            };
            
            let offset_y = (term_h as u32).saturating_sub(display_height) / 2;
            let offset_x = (term_w as u32).saturating_sub(target_width) / 2;

            for _ in 0..offset_y {
                render_buffer.push_str("\r\n");
            }

            match mode {
                RenderMode::PixelArt => {
                    for y in 0..(target_height / 2) {
                        if offset_x > 0 {
                            write!(render_buffer, "\x1b[0m{:width$}", "", width=offset_x as usize).unwrap();
                            last_fg = None; last_bg = None;
                        }

                        for x in 0..target_width {
                            let p1 = img.get_pixel(x, y * 2);
                            let [r1, g1, b1] = p1.0;
                            let p2 = img.get_pixel(x, y * 2 + 1);
                            let [r2, g2, b2] = p2.0;

                            let curr_fg = (r1, g1, b1);
                            if last_fg != Some(curr_fg) {
                                write!(render_buffer, "\x1b[38;2;{};{};{}m", r1, g1, b1).unwrap();
                                last_fg = Some(curr_fg);
                            }

                            let curr_bg = (r2, g2, b2);
                            if last_bg != Some(curr_bg) {
                                write!(render_buffer, "\x1b[48;2;{};{};{}m", r2, g2, b2).unwrap();
                                last_bg = Some(curr_bg);
                            }

                            render_buffer.push('▀');
                        }
                        render_buffer.push_str("\x1b[0m\r\n");
                        last_fg = None; last_bg = None;
                    }
                },
                RenderMode::AsciiArt => {
                    for y in 0..target_height {
                        if offset_x > 0 {
                            write!(render_buffer, "\x1b[0m{:width$}", "", width=offset_x as usize).unwrap();
                            last_fg = None; 
                        }

                        for x in 0..target_width {
                            let pixel = img.get_pixel(x, y);
                            let [r, g, b] = pixel.0;

                            let brightness = ((r as u16 * 77 + g as u16 * 150 + b as u16 * 29) >> 8) as u8;
                            let char_idx = (brightness as usize * (ascii_chars.len() - 1)) / 255;
                            let ascii = ascii_chars[char_idx] as char;

                            let curr_fg = (r, g, b);
                            if last_fg != Some(curr_fg) {
                                write!(render_buffer, "\x1b[38;2;{};{};{}m", r, g, b).unwrap();
                                last_fg = Some(curr_fg);
                            }
                            render_buffer.push(ascii);
                        }
                        render_buffer.push_str("\x1b[0m\r\n");
                        last_fg = None;
                    }
                }
            }
            
            stdout_term.write_all(render_buffer.as_bytes())?;
            stdout_term.flush()?;
            
            if crossterm::event::poll(Duration::from_millis(0))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.code == crossterm::event::KeyCode::Char('q') || key.code == crossterm::event::KeyCode::Esc {
                        break;
                    }
                }
            }
        }
        Ok(())
    })();

    let _ = stdout_term.write(b"\x1b[0m"); 
    execute!(stdout_term, crossterm::cursor::Show, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    let _ = child.kill();

    result
}

struct VideoInfo {
    width: u32,
    height: u32,
    fps: f32,
    duration: f64,
    video_codec: String,
    audio_codec: Option<String>,
    bitrate: Option<u64>,
}

fn probe_video(path: &Path) -> Result<VideoInfo> {
    let ffprobe_cmd = get_command_path("ffprobe");
    
    // 1. Probe Video Stream
    let output = Command::new(&ffprobe_cmd)
        .arg("-v").arg("error")
        .arg("-select_streams").arg("v:0")
        .arg("-show_entries").arg("stream=width,height,r_frame_rate,duration,codec_name,bit_rate")
        .arg("-of").arg("default=noprint_wrappers=1")
        .arg(path)
        .output()
        .context("Failed to run ffprobe for video stream")?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    
    let mut width = 0;
    let mut height = 0;
    let mut fps = 30.0;
    let mut duration = 0.0;
    let mut video_codec = String::from("Unknown");
    let mut bitrate = None;

    for line in output_str.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "width" => width = value.trim().parse().unwrap_or(0),
                "height" => height = value.trim().parse().unwrap_or(0),
                "r_frame_rate" => {
                    let fps_str = value.trim();
                    if fps_str.contains('/') {
                        let parts: Vec<&str> = fps_str.split('/').collect();
                        if parts.len() == 2 {
                            let num: f32 = parts[0].parse().unwrap_or(0.0);
                            let den: f32 = parts[1].parse().unwrap_or(1.0);
                            if den != 0.0 { fps = num / den; }
                        }
                    } else {
                        fps = fps_str.parse().unwrap_or(30.0);
                    }
                },
                "duration" => duration = value.trim().parse().unwrap_or(0.0),
                "codec_name" => video_codec = value.trim().to_string(),
                "bit_rate" => {
                    if let Ok(br) = value.trim().parse::<u64>() {
                        bitrate = Some(br);
                    }
                },
                _ => {}
            }
        }
    }

    if width == 0 || height == 0 {
        anyhow::bail!("Failed to parse essential video metadata.");
    }

    // 2. Probe Audio Stream
    let audio_output = Command::new(&ffprobe_cmd)
        .arg("-v").arg("error")
        .arg("-select_streams").arg("a:0")
        .arg("-show_entries").arg("stream=codec_name")
        .arg("-of").arg("default=noprint_wrappers=1")
        .arg(path)
        .output()
        .ok(); // Optional

    let mut audio_codec = None;
    if let Some(out) = audio_output {
        let out_str = String::from_utf8_lossy(&out.stdout);
        for line in out_str.lines() {
             if let Some((key, value)) = line.split_once('=') {
                 if key.trim() == "codec_name" {
                     audio_codec = Some(value.trim().to_string());
                 }
             }
        }
    }

    Ok(VideoInfo {
        width,
        height,
        fps,
        duration,
        video_codec,
        audio_codec,
        bitrate,
    })
}

fn get_command_path(cmd: &str) -> String {
    let exe_name = if cfg!(target_os = "windows") {
        format!("{}.exe", cmd)
    } else {
        cmd.to_string()
    };

    if std::path::Path::new(&exe_name).exists() {
        if let Ok(path) = std::env::current_dir() {
            return path.join(exe_name).to_string_lossy().to_string();
        }
    }
    cmd.to_string()
}
