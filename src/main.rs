use std::{
    sync::Arc,
    time::{Duration, Instant},
    fs::metadata,
};
use anyhow::Result;
use chrono::Local;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
    cursor::{Hide, Show},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, Mutex},
    time,
    fs::OpenOptions,
};
use tokio_serial::{SerialPortBuilderExt, DataBits, FlowControl, Parity, StopBits};

const VALID_BAUD_RATES: &[u32] = &[300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200];

fn validate_baud_rate(baud: &str) -> Result<u32, String> {
    let baud: u32 = baud.parse().map_err(|_| {
        format!("Baud rate must be a number, one of {:?}", VALID_BAUD_RATES)
    })?;
    if VALID_BAUD_RATES.contains(&baud) {
        Ok(baud)
    } else {
        Err(format!(
            "Invalid baud rate: {}. Must be one of {:?}", 
            baud, VALID_BAUD_RATES
        ))
    }
}

fn validate_port(port: &str) -> Result<String, String> {
    // Check if port matches Unix-like (/dev/tty*) or Windows (COM*) patterns
    let is_valid_pattern = 
        port.starts_with("/dev/tty") || // Unix-like systems (Linux/macOS)
        port.to_uppercase().starts_with("COM"); // Windows (e.g., COM1, COM2)
    
    if !is_valid_pattern {
        return Err(format!(
            "Invalid port: {}. Must start with '/dev/tty' (Unix) or 'COM' (Windows)", 
            port
        ));
    }

    // Optionally check if port exists (non-async, but fine for startup)
    if metadata(port).is_err() {
        // Warn but don't fail, as ports may appear dynamically or require permissions
        eprintln!("Warning: Port '{}' may not exist or is inaccessible", port);
    }

    Ok(port.to_string())
}

#[derive(Parser, Debug)]
#[command(about = "Serial monitor for Arduino communication")]
struct Args {
    /// Serial port name (e.g., /dev/ttyUSB0 or COM1)
    #[arg(long, default_value = "/dev/ttyUSB0", value_parser = validate_port)]
    port: String,

    /// Baud rate for serial communication
    #[arg(long, default_value_t = 57600, value_parser = validate_baud_rate)]
    baud_rate: u32,

    /// Log file path
    #[arg(long, default_value = "serial_monitor.log")]
    log_file: String,

    /// Disable logging to file
    #[arg(long)]
    no_log: bool,
}

async fn log_to_file(file: &Arc<Mutex<tokio::fs::File>>, text: &str) {
    let timestamp = Local::now().to_rfc3339();
    let log_line = format!("[{}] {}\n", timestamp, text);
    let mut file = file.lock().await;
    if let Err(e) = file.write_all(log_line.as_bytes()).await {
        eprintln!("Log write error: {e}");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // Open log file (if not disabled)
    let log_file = if !args.no_log {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&args.log_file)
            .await?;
        Some(Arc::new(Mutex::new(file)))
    } else {
        None
    };

    // Open serial port
    let port = tokio_serial::new(&args.port, args.baud_rate)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .open_native_async()?;

    // Split into async read/write halves
    let (mut reader, writer) = tokio::io::split(port);
    let writer = Arc::new(Mutex::new(writer));

    // Channels for data exchange between UI and serial
    let (tx_serial, mut rx_serial) = mpsc::unbounded_channel::<String>();
    let (tx_write, mut rx_write) = mpsc::unbounded_channel::<String>();

    // Reader task (reads from Arduino)
    tokio::spawn({
        let tx_serial = tx_serial.clone();
        async move {
            let mut buf = [0u8; 512];
            let mut line = String::new();
            loop {
                match reader.read(&mut buf).await {
                    Ok(n) if n > 0 => {
                        let chunk = String::from_utf8_lossy(&buf[..n]);
                        for c in chunk.chars() {
                            if c == '\n' || c == '\r' {
                                if !line.is_empty() {
                                    let _ = tx_serial.send(line.clone());
                                    line.clear();
                                }
                            } else {
                                line.push(c);
                            }
                        }
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        eprintln!("Serial read error: {e}");
                        time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
    });

    // Writer task (sends to Arduino)
    tokio::spawn({
        let writer = writer.clone();
        async move {
            while let Some(cmd) = rx_write.recv().await {
                let mut writer = writer.lock().await;
                if let Err(e) = writer.write_all(cmd.as_bytes()).await {
                    eprintln!("Serial write error: {e}");
                } else {
                    let _ = writer.write_all(b"\n").await;
                }
            }
        }
    });

    // Terminal UI
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, crossterm::terminal::EnterAlternateScreen, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut input = String::new();
    let mut cursor_pos = 0;
    let mut output_lines: Vec<Line> = Vec::new();
    let mut history: Vec<String> = Vec::new();
    let mut history_index: Option<usize> = None;
    let mut scroll_offset: usize = 0;
    let mut cursor_visible = true;
    let mut last_blink = Instant::now();
    const MAX_LINES: usize = 1000; // Memory cap for output_lines

    loop {
        if last_blink.elapsed() >= Duration::from_millis(500) {
            cursor_visible = !cursor_visible;
            last_blink = Instant::now();
        }

        terminal.draw(|f| {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .margin(1)
                .split(f.area());

            // Scrolling region
            let visible_height = layout[0].height as usize - 2;
            let start = output_lines
                .len()
                .saturating_sub(visible_height + scroll_offset);
            let visible = output_lines[start..].to_vec();

            let monitor = Paragraph::new(visible)
                .block(Block::default().title("Arduino Monitor").borders(Borders::ALL))
                .wrap(Wrap { trim: false });
            f.render_widget(monitor, layout[0]);

            // Input area
            let input_area = layout[1];
            let inner_width = input_area.width.saturating_sub(2);
            let input_offset = (cursor_pos as u16).saturating_sub(inner_width.saturating_sub(1));
            let input_widget = Paragraph::new(Line::from(input.as_str()))
                .style(Style::default().fg(Color::Yellow))
                .scroll((0, input_offset))
                .block(Block::default().title("Input").borders(Borders::ALL));
            f.render_widget(input_widget, input_area);

            if cursor_visible {
                let cursor_x = input_area.x + 1 + (cursor_pos as u16).saturating_sub(input_offset);
                let cursor_y = input_area.y + 1;
                f.set_cursor_position((cursor_x, cursor_y));
            }
        })?;

        if cursor_visible {
            execute!(terminal.backend_mut(), Show)?;
        } else {
            execute!(terminal.backend_mut(), Hide)?;
        }

        // Process serial lines
        while let Ok(line) = rx_serial.try_recv() {
            let style = if line.contains("ERROR") {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            let line_text = format!("[Arduino] {}", line);
            output_lines.push(Line::from(Span::styled(line_text.clone(), style)));
            // Log to file (if enabled) and enforce memory cap
            if let Some(log_file) = &log_file {
                log_to_file(log_file, &line_text).await;
            }
            if output_lines.len() > MAX_LINES {
                output_lines.remove(0);
                if scroll_offset > 0 {
                    scroll_offset = scroll_offset.saturating_sub(1);
                }
            }
        }

        // Handle user input
        if event::poll(Duration::from_millis(10))? 
            && let Event::Key(key) = event::read()? 
        {
                match key.code {
                    KeyCode::Char(c) => {
                        input.insert(cursor_pos, c);
                        cursor_pos += 1;
                    }
                    KeyCode::Backspace => {
                        if cursor_pos > 0 {
                            input.remove(cursor_pos - 1);
                            cursor_pos -= 1;
                        }
                    }
                    KeyCode::Left => {
                        cursor_pos = cursor_pos.saturating_sub(1);
                    }
                    KeyCode::Right => {
                        if cursor_pos < input.len() {
                            cursor_pos += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if !input.trim().is_empty() {
                            history.push(input.clone());
                            let _ = tx_write.send(input.clone());
                            let line_text = format!("> {}", input);
                            output_lines.push(Line::from(Span::styled(
                                line_text.clone(),
                                Style::default().fg(Color::Yellow),
                            )));
                            // Log to file (if enabled) and enforce memory cap
                            if let Some(log_file) = &log_file {
                                log_to_file(log_file, &line_text).await;
                            }
                            if output_lines.len() > MAX_LINES {
                                output_lines.remove(0);
                                if scroll_offset > 0 {
                                    scroll_offset = scroll_offset.saturating_sub(1);
                                }
                            }
                            input.clear();
                            cursor_pos = 0;
                            history_index = None;
                        }
                    }
                    KeyCode::Up => {
                        if let Some(new_idx) = history_index.map(|i| i.saturating_sub(1)).or_else(|| {
                            if !history.is_empty() {
                                Some(history.len() - 1)
                            } else {
                                None
                            }
                        }) {
                            input = history[new_idx].clone();
                            cursor_pos = input.len();
                            history_index = Some(new_idx);
                        }
                    }
                    KeyCode::Down => {
                        if let Some(i) = history_index {
                            if i + 1 < history.len() {
                                input = history[i + 1].clone();
                                cursor_pos = input.len();
                                history_index = Some(i + 1);
                            } else {
                                input.clear();
                                cursor_pos = 0;
                                history_index = None;
                            }
                        }
                    }
                    KeyCode::PageUp => {
                        scroll_offset = (scroll_offset + 3).min(output_lines.len().saturating_sub(1));
                    }
                    KeyCode::PageDown => {
                        scroll_offset = scroll_offset.saturating_sub(3);
                    }
                    KeyCode::Esc => break,
                    _ => {}
                }
        
        }

        time::sleep(Duration::from_millis(10)).await;
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        Show
    )?;
    Ok(())
}