use clap::Parser;
use crossterm::{
    event::{poll, read, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
    Terminal,
};
use std::io::{self, Stdout};
use std::os::fd::RawFd;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

#[derive(Parser, Debug)]
#[command(name = "corridor")]
#[command(about = "Terminal emulator with web panel", long_about = None)]
struct Args {
    #[arg(short, long, default_value = None)]
    session: Option<String>,

    #[arg(short, long, default_value = None)]
    url: Option<String>,

    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    debug: bool,
}

#[repr(C)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

#[repr(C)]
struct Timeval {
    tv_sec: libc::c_long,
    tv_usec: libc::c_long,
}

extern "C" {
    fn openpty(main: *mut RawFd, client: *mut RawFd, name: *mut std::ffi::c_char, term: *const std::ffi::c_void, win: *const Winsize) -> i32;
    fn fork() -> i32;
    fn setsid() -> i32;
    fn select(nfds: i32, readfds: *mut libc::fd_set, writefds: *mut libc::fd_set, exceptfds: *mut libc::fd_set, timeout: *const Timeval) -> i32;
}

const PANEL_HEIGHT: u16 = 5;
const HTTP_POLL_INTERVAL_MS: u64 = 200;
const MIN_TERM_HEIGHT: u16 = 7;
const PTY_TIMEOUT_US: libc::c_long = 20_000;

enum HttpMessage {
    Data(String),
    Error(String),
}

struct HttpState {
    data: String,
    error: Option<String>,
    is_offline: bool,
}

fn color_from_vt100(fg: vt100::Color) -> Color {
    match fg {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => match i {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::White,
            8 => Color::DarkGray,
            9 => Color::LightRed,
            10 => Color::LightGreen,
            11 => Color::LightYellow,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            15 => Color::Gray,
            n => Color::Indexed(n),
        },
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

struct TerminalScreen<'a> {
    screen: &'a vt100::Screen,
}

impl<'a> TerminalScreen<'a> {
    fn new(screen: &'a vt100::Screen) -> Self {
        Self { screen }
    }
}

impl<'a> Widget for TerminalScreen<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let (rows, cols) = self.screen.size();

        for row in 0..rows {
            for col in 0..cols {
                if row as u16 >= area.height || col as u16 >= area.width {
                    continue;
                }

                if let Some(cell) = self.screen.cell(row, col) {
                    let x = area.x + col as u16;
                    let y = area.y + row as u16;

                    let c = if cell.contents().is_empty() { ' ' } else { cell.contents().chars().next().unwrap_or(' ') };

                    let fg = cell.fgcolor();
                    let bg = cell.bgcolor();
                    let fg_color = color_from_vt100(fg);
                    let bg_color = color_from_vt100(bg);

                    let mut style = Style::default()
                        .fg(fg_color)
                        .bg(bg_color);

                    if cell.bold() {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if cell.underline() {
                        style = style.add_modifier(Modifier::UNDERLINED);
                    }
                    if cell.inverse() {
                        style = style.add_modifier(Modifier::REVERSED);
                    }

                    buf[(x, y)].set_char(c).set_style(style);
                }
            }
        }
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    for line in text.split('\n') {
        if line.len() <= width {
            lines.push(line.to_string());
        } else {
            let words: Vec<&str> = line.split_whitespace().collect();
            let mut current = String::new();
            for word in words {
                if current.len() + word.len() + 1 <= width {
                    if !current.is_empty() {
                        current.push(' ');
                    }
                    current.push_str(word);
                } else {
                    if !current.is_empty() {
                        lines.push(current.clone());
                    }
                    current = word.to_string();
                }
            }
            if !current.is_empty() {
                lines.push(current);
            }
        }
    }
    lines
}

fn interpret_http_status(code: u16) -> &'static str {
    match code {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        429 => "Too Many Requests",
        500 => "Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ if code >= 400 && code < 500 => "Client Error",
        _ if code >= 500 => "Server Error",
        _ => "Unknown",
    }
}

async fn fetch_session_data(session: &str, url: &str, debug: bool) -> (String, Option<String>) {
    let api_url = format!("{}/api/message?session={}", url, session);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build();

    match client {
        Ok(client) => {
            match client.get(&api_url).header("User-Agent", "curl/8.0.1").send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.text().await {
                            Ok(text) => {
                                match serde_json::from_str::<serde_json::Value>(&text) {
                                    Ok(json) => (json.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string(), None),
                                    Err(e) => {
                                        if debug {
                                            eprintln!("DEBUG: JSON parse error: {}", e);
                                        }
                                        (String::new(), Some("Invalid response".to_string()))
                                    }
                                }
                            }
                            Err(e) => (String::new(), Some(format!("Response error: {}", e.to_string().split(" for ").next().unwrap_or("unknown")))),
                        }
                    } else {
                        let code = resp.status().as_u16();
                        let msg = format!("HTTP {} - {}", code, interpret_http_status(code));
                        if debug {
                            eprintln!("DEBUG: {}", msg);
                        }
                        (String::new(), Some(msg))
                    }
                }
                Err(e) => {
                    if debug {
                        eprintln!("DEBUG: fetch failed: {}", e);
                    }
                    let msg = e.to_string();
                    let clean_msg = msg.split(" for ").next().unwrap_or("Connection failed");
                    (String::new(), Some(format!("Network error: {}", clean_msg)))
                }
            }
        }
        Err(_) => (String::new(), Some("HTTP client error".to_string())),
    }
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn handle_key_event(key: KeyEvent, main_fd: RawFd) {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    
    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                if c.is_ascii_lowercase() {
                    let ctrl_char = (c as u8) & 0x1F;
                    unsafe { libc::write(main_fd, &ctrl_char as *const _ as *const libc::c_void, 1); }
                } else if c.is_ascii_uppercase() {
                    let ctrl_char = (c.to_ascii_lowercase() as u8) & 0x1F;
                    unsafe { libc::write(main_fd, &ctrl_char as *const _ as *const libc::c_void, 1); }
                }
            } else if alt {
                let bytes = [0x1b, c as u8];
                unsafe { libc::write(main_fd, bytes.as_ptr() as *const libc::c_void, 2); }
            } else {
                let bytes = [c as u8];
                unsafe { libc::write(main_fd, bytes.as_ptr() as *const libc::c_void, 1); }
            }
        }
        KeyCode::Enter => {
            if alt {
                unsafe { libc::write(main_fd, b"\x1b\n".as_ptr() as *const libc::c_void, 2); }
            } else {
                unsafe { libc::write(main_fd, b"\n".as_ptr() as *const libc::c_void, 1); }
            }
        }
        KeyCode::Backspace => {
            if alt {
                unsafe { libc::write(main_fd, b"\x1b\x7f".as_ptr() as *const libc::c_void, 2); }
            } else {
                let b = [0x7f];
                unsafe { libc::write(main_fd, b.as_ptr() as *const libc::c_void, 1); }
            }
        }
        KeyCode::Up => {
            let seq = if shift { "\x1b[1;2A" } else if alt { "\x1b[1;3A" } else if ctrl { "\x1b[1;5A" } else { "\x1b[A" };
            unsafe { libc::write(main_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
        }
        KeyCode::Down => {
            let seq = if shift { "\x1b[1;2B" } else if alt { "\x1b[1;3B" } else if ctrl { "\x1b[1;5B" } else { "\x1b[B" };
            unsafe { libc::write(main_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
        }
        KeyCode::Right => {
            let seq = if shift { "\x1b[1;2C" } else if alt { "\x1b[1;3C" } else if ctrl { "\x1b[1;5C" } else { "\x1b[C" };
            unsafe { libc::write(main_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
        }
        KeyCode::Left => {
            let seq = if shift { "\x1b[1;2D" } else if alt { "\x1b[1;3D" } else if ctrl { "\x1b[1;5D" } else { "\x1b[D" };
            unsafe { libc::write(main_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
        }
        KeyCode::Esc => {
            unsafe { libc::write(main_fd, b"\x1b".as_ptr() as *const libc::c_void, 1); }
        }
        KeyCode::Tab => {
            if shift {
                unsafe { libc::write(main_fd, b"\x1b[Z".as_ptr() as *const libc::c_void, 3); }
            } else if alt {
                unsafe { libc::write(main_fd, b"\x1b\t".as_ptr() as *const libc::c_void, 2); }
            } else {
                unsafe { libc::write(main_fd, b"\t".as_ptr() as *const libc::c_void, 1); }
            }
        }
        KeyCode::Home => {
            unsafe { libc::write(main_fd, b"\x1b[H".as_ptr() as *const libc::c_void, 3); }
        }
        KeyCode::End => {
            unsafe { libc::write(main_fd, b"\x1b[F".as_ptr() as *const libc::c_void, 3); }
        }
        KeyCode::PageUp => {
            unsafe { libc::write(main_fd, b"\x1b[5~".as_ptr() as *const libc::c_void, 4); }
        }
        KeyCode::PageDown => {
            unsafe { libc::write(main_fd, b"\x1b[6~".as_ptr() as *const libc::c_void, 4); }
        }
        KeyCode::Delete => {
            unsafe { libc::write(main_fd, b"\x1b[3~".as_ptr() as *const libc::c_void, 4); }
        }
        KeyCode::Insert => {
            unsafe { libc::write(main_fd, b"\x1b[2~".as_ptr() as *const libc::c_void, 4); }
        }
        KeyCode::F(n) => {
            let seq = match n {
                1 => "\x1bOP",
                2 => "\x1bOQ",
                3 => "\x1bOR",
                4 => "\x1bOS",
                5 => "\x1b[15~",
                6 => "\x1b[17~",
                7 => "\x1b[18~",
                8 => "\x1b[19~",
                9 => "\x1b[20~",
                10 => "\x1b[21~",
                11 => "\x1b[23~",
                12 => "\x1b[24~",
                _ => return,
            };
            unsafe { libc::write(main_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
        }
        _ => {}
    }
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    let session = args.session.unwrap_or_else(|| std::env::var("SESSION").unwrap_or_else(|_| "default".to_string()));
    let url = args.url.unwrap_or_else(|| std::env::var("URL").unwrap_or_else(|_| "http://localhost:8080".to_string())).trim_end_matches('/').to_string();
    let debug = args.debug;

    if debug {
        eprintln!("DEBUG: session={}, url={}", session, url);
    }

    let (cols, rows) = std::process::Command::new("stty")
        .args(["size"])
        .output()
        .ok()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            let mut parts = out.split_whitespace();
            let rows: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(24);
            let cols: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(80);
            (cols, rows)
        })
        .unwrap_or((80, 24));

    let main_rows = (rows.saturating_sub(PANEL_HEIGHT as usize + 1)) as u16;
    let cols = cols as u16;

    let mut main_fd: RawFd = 0;
    let mut client_fd: RawFd = 0;
    let child_pid: i32;

    let win = Winsize {
        ws_row: main_rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe {
        if openpty(&mut main_fd, &mut client_fd, std::ptr::null_mut(), std::ptr::null(), &win) != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }

    unsafe {
        let pid = fork();
        if pid < 0 {
            return Err(std::io::Error::last_os_error());
        }

        if pid == 0 {
            setsid();

            libc::dup2(client_fd, 0);
            libc::dup2(client_fd, 1);
            libc::dup2(client_fd, 2);

            if client_fd > 2 { libc::close(client_fd); }
            if main_fd > 2 { libc::close(main_fd); }

            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            let shell_cstr = std::ffi::CString::new(shell).unwrap();
            libc::execv(shell_cstr.as_ptr(), std::ptr::null());
            libc::_exit(1);
        }

        child_pid = pid;
        libc::close(client_fd);
    }

    let mut terminal = setup_terminal()?;
    
    let area = terminal.size()?;
    let actual_cols = area.width;
    let actual_rows = area.height.saturating_sub(PANEL_HEIGHT + 1);
    
    if actual_cols != cols || actual_rows != main_rows {
        unsafe {
            let win = Winsize {
                ws_row: actual_rows,
                ws_col: actual_cols,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            libc::ioctl(main_fd, 0x5414, &win);
        }
    }
    
    std::thread::sleep(std::time::Duration::from_millis(50));
    unsafe {
        libc::kill(child_pid, libc::SIGWINCH);
    }

    let mut parser = vt100::Parser::new(actual_rows, actual_cols, 0);
    let mut buf = [0u8; 4096];
    let mut last_term_size: (u16, u16) = (actual_cols, actual_rows);

    let (http_tx, http_rx): (Sender<HttpMessage>, Receiver<HttpMessage>) = mpsc::channel();
    let retry_requested = Arc::new(AtomicBool::new(false));
    let retry_flag = Arc::clone(&retry_requested);
    let running = Arc::new(AtomicBool::new(true));
    let running_flag = Arc::clone(&running);

    let session_clone = session.clone();
    let url_clone = url.clone();
    
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut is_offline = false;
        let mut last_fetch = Instant::now();

        while running_flag.load(Ordering::Relaxed) {
            let should_poll = !is_offline && last_fetch.elapsed().as_millis() >= HTTP_POLL_INTERVAL_MS as u128;
            let should_retry = retry_flag.swap(false, Ordering::Relaxed);

            if should_poll || should_retry {
                let (data, err) = rt.block_on(async { 
                    fetch_session_data(&session_clone, &url_clone, false).await 
                });
                
                if let Some(e) = err {
                    let _ = http_tx.send(HttpMessage::Error(e));
                    is_offline = true;
                } else {
                    let _ = http_tx.send(HttpMessage::Data(data));
                    is_offline = false;
                }
                last_fetch = Instant::now();
            } else {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    });

    let mut http_state = HttpState {
        data: String::new(),
        error: None,
        is_offline: false,
    };
    let mut refresh_started: Option<Instant> = None;

    loop {
        unsafe {
            let mut status: i32 = 0;
            if libc::waitpid(-1, &mut status, libc::WNOHANG) > 0 {
                break;
            }
        }

        while let Ok(msg) = http_rx.try_recv() {
            match msg {
                HttpMessage::Data(d) => {
                    http_state.data = d;
                    http_state.error = None;
                    http_state.is_offline = false;
                }
                HttpMessage::Error(e) => {
                    http_state.error = Some(e);
                    http_state.is_offline = true;
                }
            }
        }

        unsafe {
            let mut readfds: libc::fd_set = std::mem::zeroed();
            libc::FD_SET(main_fd, &mut readfds);

            let timeout = Timeval { tv_sec: 0, tv_usec: PTY_TIMEOUT_US };
            let n = select(main_fd + 1, &mut readfds, std::ptr::null_mut(), std::ptr::null_mut(), &timeout);

            if n > 0 && libc::FD_ISSET(main_fd, &mut readfds) as i32 != 0 {
                let n = libc::read(main_fd, buf.as_mut_ptr() as *mut _, buf.len());
                if n > 0 {
                    parser.process(&buf[..n as usize]);
                } else if n == 0 {
                    break;
                }
            }
        }

        let area = terminal.size()?;
        let new_cols = area.width;
        let new_rows = area.height.saturating_sub(PANEL_HEIGHT + 1);
        
        if area.height < MIN_TERM_HEIGHT {
            terminal.draw(|f| {
                let msg = "Terminal too small (min 7 rows)";
                let text = Line::from(Span::styled(msg, Style::default().fg(Color::Red)));
                f.render_widget(text, f.area());
            })?;
            continue;
        }
        
        if new_cols != last_term_size.0 || new_rows != last_term_size.1 {
            parser.set_size(new_rows, new_cols);
            last_term_size = (new_cols, new_rows);
            
            unsafe {
                let win = Winsize {
                    ws_row: new_rows,
                    ws_col: new_cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                libc::ioctl(main_fd, 0x5414, &win);
            }
        }

        terminal.draw(|f| {
            let area = f.area();
            let term_height = area.height.saturating_sub(PANEL_HEIGHT + 1);
            
            if term_height == 0 {
                let msg = "Terminal too small";
                let text = Line::from(Span::styled(msg, Style::default().fg(Color::Red)));
                f.render_widget(text, area);
                return;
            }
            
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(term_height),
                    Constraint::Length(1),
                    Constraint::Length(PANEL_HEIGHT),
                ])
                .split(area);

            let term_area = chunks[0];
            let sep_area = chunks[1];
            let web_area = chunks[2];

            let screen_widget = TerminalScreen::new(parser.screen());
            f.render_widget(screen_widget, term_area);

            let session_text = if http_state.is_offline {
                format!(" {} @ {} (offline) ", session, url)
            } else {
                format!(" {} @ {} ", session, url)
            };
            let copy_hint: String = if !http_state.is_offline && !http_state.data.is_empty() {
                "[F6: copy] ".to_string()
            } else {
                String::new()
            };
            let total_width = sep_area.width;
            let combined_len = session_text.len() + copy_hint.len();
            let max_text_len = total_width.saturating_sub(4) as usize;
            let (session_text, copy_hint) = if combined_len > max_text_len && max_text_len > 3 {
                let available = max_text_len;
                if copy_hint.is_empty() {
                    (format!("{}...", &session_text[..available.saturating_sub(3)]), copy_hint)
                } else if available > 25 {
                    let session_max = available.saturating_sub(copy_hint.len());
                    (format!("{}...", &session_text[..session_max.saturating_sub(3)]), copy_hint)
                } else {
                    (format!("{}...", &session_text[..available.saturating_sub(3)]), String::new())
                }
            } else {
                (session_text, copy_hint)
            };
            let text_len = session_text.len() as u16 + copy_hint.len() as u16;
            let left_dashes = total_width.saturating_sub(text_len + 2) / 2;
            let right_dashes = total_width.saturating_sub(text_len + 2) - left_dashes;

            let sep_line = if copy_hint.is_empty() {
                Line::from(vec![
                    Span::styled("├", Style::default().fg(Color::Cyan)),
                    Span::styled("─".repeat(left_dashes as usize), Style::default().fg(Color::Cyan)),
                    Span::styled(session_text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled("─".repeat(right_dashes as usize), Style::default().fg(Color::Cyan)),
                    Span::styled("┤", Style::default().fg(Color::Cyan)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(copy_hint, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                    Span::styled("─".repeat(left_dashes as usize), Style::default().fg(Color::Cyan)),
                    Span::styled(session_text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled("─".repeat(right_dashes as usize), Style::default().fg(Color::Cyan)),
                    Span::styled("┤", Style::default().fg(Color::Cyan)),
                ])
            };
            f.render_widget(sep_line, sep_area);

            let web_content: Vec<Line> = if let Some(start) = refresh_started {
                if start.elapsed().as_millis() < 1000 {
                    vec![Line::from(Span::styled("[Refreshing...]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)))]
                } else {
                    refresh_started = None;
                    if let Some(ref err) = http_state.error {
                        vec![
                            Line::from(Span::styled(format!("[{}]", err), Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD))),
                            Line::from(Span::styled("[F5 to retry]", Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD))),
                        ]
                    } else {
                        vec![Line::from(Span::styled("[web panel empty - send data via web interface]".to_string(), Style::default().fg(Color::Cyan)))]
                    }
                }
            } else if !http_state.data.is_empty() {
                let wrapped = wrap_text(&http_state.data, web_area.width as usize);
                wrapped.iter().take(4).map(|s| Line::from(Span::styled(s.clone(), Style::default().add_modifier(Modifier::BOLD)))).collect()
            } else if let Some(ref err) = http_state.error {
                vec![
                    Line::from(Span::styled(format!("[{}]", err), Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD))),
                    Line::from(Span::styled("[F5 to retry]", Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD))),
                ]
            } else {
                vec![Line::from(Span::styled("[web panel empty - send data via web interface]".to_string(), Style::default().fg(Color::Cyan)))]
            };

            let web_paragraph = Paragraph::new(web_content);
            f.render_widget(web_paragraph, web_area);

            let (cursor_row, cursor_col) = parser.screen().cursor_position();
            if cursor_row < term_area.height && cursor_col < term_area.width {
                f.set_cursor_position((term_area.x + cursor_col, term_area.y + cursor_row));
            }
        })?;

        if poll(std::time::Duration::from_millis(10))? {
            if let Event::Key(key) = read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::F(5) => {
                            retry_requested.store(true, Ordering::Relaxed);
                            refresh_started = Some(Instant::now());
                        }
                        KeyCode::F(6) => {
                            if !http_state.is_offline && !http_state.data.is_empty() {
                                let escaped = http_state.data.replace("\r\n", "\\n").replace('\n', "\\n").replace('\r', "\\r");
                                unsafe {
                                    libc::write(main_fd, escaped.as_ptr() as *const _, escaped.len());
                                }
                            }
                        }
                        _ => {
                            handle_key_event(key, main_fd);
                        }
                    }
                }
            }
        }
    }

    running.store(false, Ordering::Relaxed);
    restore_terminal()?;
    unsafe { libc::close(main_fd); }

    Ok(())
}
