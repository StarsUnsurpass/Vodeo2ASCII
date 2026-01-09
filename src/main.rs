use std::io::{Read, Write as IoWrite};
use std::process::{Command, Stdio};
use std::time::Duration;
use std::path::{PathBuf, Path};
use std::fmt::Write;

use anyhow::{Context, Result};
use crossterm::{
    cursor, execute,
    style::{Color, Print, SetForegroundColor},
    terminal::{self},
};
use image::{RgbImage};
use inquire::{Select, Text};
use glob::glob;

#[derive(Debug, Clone, Copy, PartialEq)]
enum RenderMode {
    PixelArt,
    AsciiArt,
}

impl std::fmt::Display for RenderMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderMode::PixelArt => write!(f, "Pixel Art (High Fidelity - Half Block)"),
            RenderMode::AsciiArt => write!(f, "ASCII Art (Classic - Character)"),
        }
    }
}

fn main() -> Result<()> {
    // 1. Scan for video files
    let mut files: Vec<String> = Vec::new();
    let patterns = ["*.mp4", "*.mkv", "*.avi", "*.mov", "*.flv", "*.webm", "*.MP4"];
    for pattern in patterns {
        for entry in glob(pattern)? {
            if let Ok(path) = entry {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }

    let selected_path = loop {
        let choice = if files.is_empty() {
            println!("No video files found in the current directory.");
            Text::new("Please enter the path to a video file (or drag & drop it here):").prompt()
        } else {
            files.push("[ Manual Input / Other Path ]".to_string());
            files.sort(); 
            files.dedup();
            
            if let Some(pos) = files.iter().position(|x| x == "[ Manual Input / Other Path ]") {
                let item = files.remove(pos);
                files.push(item);
            }

            Select::new("Choose a video to play (Use arrow keys):", files.clone()).prompt()
        };

        match choice {
            Ok(selection) => {
                let input_str = if selection == "[ Manual Input / Other Path ]" {
                    match Text::new("Enter video path (or drag & drop it here):").prompt() {
                        Ok(text) => text,
                        Err(_) => return Ok(()), 
                    }
                } else {
                    selection
                };

                let cleaned_input = input_str.trim()
                    .trim_matches('"')
                    .trim_matches('\'').trim_start_matches('@')
                    .trim();
                
                let cleaned_string = cleaned_input.replace(r"\ ", " ");
                let path = PathBuf::from(&cleaned_string);

                if path.exists() {
                    break path;
                } else {
                    execute!(std::io::stdout(), 
                        SetForegroundColor(Color::Red), 
                        Print(format!("Error: File '{}' not found.\n", path.display())),
                        SetForegroundColor(Color::Reset)
                    )?;
                    continue;
                }
            }
            Err(_) => return Ok(()), 
        }
    };

    // 2. Select Render Mode
    let mode = Select::new(
        "Choose rendering style:",
        vec![RenderMode::PixelArt, RenderMode::AsciiArt],
    ).prompt()?;

    play_video(&selected_path, mode)
}

fn play_video(video_path: &Path, mode: RenderMode) -> Result<()> {
    // 3. Probe metadata
    let (orig_w, orig_h, _fps) = probe_video(video_path)?;
    let (term_w, term_h) = terminal::size()?;
    
    // 4. Determine processing resolution
    let (target_width, target_height) = match mode {
        RenderMode::PixelArt => {
            // STRATEGY: Half-Block Rendering (▀)
            // Effective grid is roughly 1:1 square pixels because term char is 1:2.
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
            // STRATEGY: Classic ASCII
            // 1 char = 1 pixel. But char is 1:2 aspect ratio.
            // So we need height to be 0.5 * width * video_aspect.
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

    // 5. Start ffmpeg
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
    execute!(stdout_term, terminal::EnterAlternateScreen, cursor::Hide)?;

    // Buffer: worst case 30 chars per pixel
    let mut render_buffer = String::with_capacity((target_width * target_height * 30) as usize);
    let ascii_chars = b" .:-=+*#%@";

    let result = (|| -> Result<()> {
        loop {
            if let Err(_) = stdout.read_exact(&mut buffer) {
                break; 
            }

            let img = RgbImage::from_raw(target_width, target_height, buffer.clone())
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
                        // Pad left
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
                        // Pad left
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
    execute!(stdout_term, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    let _ = child.kill();

    result
}

fn probe_video(path: &Path) -> Result<(u32, u32, f32)> {
    let ffprobe_cmd = get_command_path("ffprobe");
    let output = Command::new(&ffprobe_cmd)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height,r_frame_rate")
        .arg("-of")
        .arg("csv=p=0")
        .arg(path)
        .output()
        .with_context(|| format!("Failed to run '{}'. Is FFmpeg installed? (Try putting ffmpeg.exe and ffprobe.exe in the current folder)", ffprobe_cmd))?;
    
    let output_str = String::from_utf8(output.stdout)?;
    let parts: Vec<&str> = output_str.trim().split(',').collect();
    
    if parts.len() < 3 {
        anyhow::bail!("Failed to parse ffprobe output.");
    }

    let width: u32 = parts[0].trim().parse()?;
    let height: u32 = parts[1].trim().parse()?;
    
    let fps_str = parts[2].trim();
    let fps: f32 = if fps_str.contains('/') {
        let frac: Vec<&str> = fps_str.split('/').collect();
        let num: f32 = frac[0].parse()?;
        let den: f32 = frac[1].parse()?;
        if den == 0.0 { 0.0 } else { num / den }
    } else {
        fps_str.parse().unwrap_or(30.0)
    };

    Ok((width, height, fps))
}

fn get_command_path(cmd: &str) -> String {
    // Check for local .exe on Windows (or just file on Linux)
    // Priority: Local directory > System PATH
    let exe_name = if cfg!(target_os = "windows") {
        format!("{}.exe", cmd)
    } else {
        cmd.to_string()
    };

    if std::path::Path::new(&exe_name).exists() {
        // If found in current dir, use absolute path to be safe
        if let Ok(path) = std::env::current_dir() {
            return path.join(exe_name).to_string_lossy().to_string();
        }
    }
    
    // Fallback to system command
    cmd.to_string()
}
