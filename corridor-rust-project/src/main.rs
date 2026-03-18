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
    fn openpty(master: *mut RawFd, slave: *mut RawFd, name: *mut std::ffi::c_char, term: *const std::ffi::c_void, win: *const Winsize) -> i32;
    fn fork() -> i32;
    fn setsid() -> i32;
    fn select(nfds: i32, readfds: *mut libc::fd_set, writefds: *mut libc::fd_set, exceptfds: *mut libc::fd_set, timeout: *const Timeval) -> i32;
}

const PANEL_HEIGHT: u16 = 5;

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
                                        (String::new(), Some(format!("Parse error: {}", e)))
                                    }
                                }
                            }
                            Err(e) => (String::new(), Some(e.to_string())),
                        }
                    } else {
                        let err = format!("HTTP {}", resp.status().as_u16());
                        if debug {
                            eprintln!("DEBUG: {}", err);
                        }
                        (String::new(), Some(err))
                    }
                }
                Err(e) => {
                    if debug {
                        eprintln!("DEBUG: fetch failed: {}", e);
                    }
                    (String::new(), Some(e.to_string()))
                }
            }
        }
        Err(e) => (String::new(), Some(format!("Client error: {}", e))),
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

fn handle_key_event(key: KeyEvent, master_fd: RawFd) {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    
    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                if c.is_ascii_lowercase() {
                    let ctrl_char = (c as u8) & 0x1F;
                    unsafe { libc::write(master_fd, &ctrl_char as *const _ as *const libc::c_void, 1); }
                } else if c.is_ascii_uppercase() {
                    let ctrl_char = (c.to_ascii_lowercase() as u8) & 0x1F;
                    unsafe { libc::write(master_fd, &ctrl_char as *const _ as *const libc::c_void, 1); }
                }
            } else if alt {
                let bytes = [0x1b, c as u8];
                unsafe { libc::write(master_fd, bytes.as_ptr() as *const libc::c_void, 2); }
            } else {
                let bytes = [c as u8];
                unsafe { libc::write(master_fd, bytes.as_ptr() as *const libc::c_void, 1); }
            }
        }
        KeyCode::Enter => {
            if alt {
                unsafe { libc::write(master_fd, b"\x1b\n".as_ptr() as *const libc::c_void, 2); }
            } else {
                unsafe { libc::write(master_fd, b"\n".as_ptr() as *const libc::c_void, 1); }
            }
        }
        KeyCode::Backspace => {
            if alt {
                unsafe { libc::write(master_fd, b"\x1b\x7f".as_ptr() as *const libc::c_void, 2); }
            } else {
                let b = [0x7f];
                unsafe { libc::write(master_fd, b.as_ptr() as *const libc::c_void, 1); }
            }
        }
        KeyCode::Up => {
            let seq = if shift { "\x1b[1;2A" } else if alt { "\x1b[1;3A" } else if ctrl { "\x1b[1;5A" } else { "\x1b[A" };
            unsafe { libc::write(master_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
        }
        KeyCode::Down => {
            let seq = if shift { "\x1b[1;2B" } else if alt { "\x1b[1;3B" } else if ctrl { "\x1b[1;5B" } else { "\x1b[B" };
            unsafe { libc::write(master_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
        }
        KeyCode::Right => {
            let seq = if shift { "\x1b[1;2C" } else if alt { "\x1b[1;3C" } else if ctrl { "\x1b[1;5C" } else { "\x1b[C" };
            unsafe { libc::write(master_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
        }
        KeyCode::Left => {
            let seq = if shift { "\x1b[1;2D" } else if alt { "\x1b[1;3D" } else if ctrl { "\x1b[1;5D" } else { "\x1b[D" };
            unsafe { libc::write(master_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
        }
        KeyCode::Esc => {
            unsafe { libc::write(master_fd, b"\x1b".as_ptr() as *const libc::c_void, 1); }
        }
        KeyCode::Tab => {
            if shift {
                unsafe { libc::write(master_fd, b"\x1b[Z".as_ptr() as *const libc::c_void, 3); }
            } else if alt {
                unsafe { libc::write(master_fd, b"\x1b\t".as_ptr() as *const libc::c_void, 2); }
            } else {
                unsafe { libc::write(master_fd, b"\t".as_ptr() as *const libc::c_void, 1); }
            }
        }
        KeyCode::Home => {
            unsafe { libc::write(master_fd, b"\x1b[H".as_ptr() as *const libc::c_void, 3); }
        }
        KeyCode::End => {
            unsafe { libc::write(master_fd, b"\x1b[F".as_ptr() as *const libc::c_void, 3); }
        }
        KeyCode::PageUp => {
            unsafe { libc::write(master_fd, b"\x1b[5~".as_ptr() as *const libc::c_void, 4); }
        }
        KeyCode::PageDown => {
            unsafe { libc::write(master_fd, b"\x1b[6~".as_ptr() as *const libc::c_void, 4); }
        }
        KeyCode::Delete => {
            unsafe { libc::write(master_fd, b"\x1b[3~".as_ptr() as *const libc::c_void, 4); }
        }
        KeyCode::Insert => {
            unsafe { libc::write(master_fd, b"\x1b[2~".as_ptr() as *const libc::c_void, 4); }
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
            unsafe { libc::write(master_fd, seq.as_ptr() as *const libc::c_void, seq.len()); }
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

    let mut master_fd: RawFd = 0;
    let mut slave_fd: RawFd = 0;

    let win = Winsize {
        ws_row: main_rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe {
        if openpty(&mut master_fd, &mut slave_fd, std::ptr::null_mut(), std::ptr::null(), &win) != 0 {
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

            libc::dup2(slave_fd, 0);
            libc::dup2(slave_fd, 1);
            libc::dup2(slave_fd, 2);

            if slave_fd > 2 { libc::close(slave_fd); }
            if master_fd > 2 { libc::close(master_fd); }

            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            let shell_cstr = std::ffi::CString::new(shell).unwrap();
            libc::execv(shell_cstr.as_ptr(), std::ptr::null());
            libc::_exit(1);
        }

        libc::close(slave_fd);
    }

    let mut terminal = setup_terminal()?;
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut parser = vt100::Parser::new(main_rows, cols, 0);
    let mut buf = [0u8; 4096];
    let mut last_term_size: (u16, u16) = (cols, main_rows);

    let mut last_error: Option<String> = None;
    let mut session_data: String = String::new();

    loop {
        unsafe {
            let mut status: i32 = 0;
            if libc::waitpid(-1, &mut status, libc::WNOHANG) > 0 {
                break;
            }
        }

        unsafe {
            let mut readfds: libc::fd_set = std::mem::zeroed();
            libc::FD_SET(master_fd, &mut readfds);

            let timeout = Timeval { tv_sec: 0, tv_usec: 50_000 };
            let n = select(master_fd + 1, &mut readfds, std::ptr::null_mut(), std::ptr::null_mut(), &timeout);

            if n > 0 && libc::FD_ISSET(master_fd, &mut readfds) as i32 != 0 {
                let n = libc::read(master_fd, buf.as_mut_ptr() as *mut _, buf.len());
                if n > 0 {
                    parser.process(&buf[..n as usize]);
                } else if n == 0 {
                    break;
                }
            }
        }

        let session_clone = session.clone();
        let url_clone = url.clone();
        let (data, err) = rt.block_on(async { fetch_session_data(&session_clone, &url_clone, debug).await });
        if let Some(e) = err {
            last_error = Some(e);
        } else {
            session_data = data;
            last_error = None;
        }

        let area = terminal.size()?;
        let new_cols = area.width;
        let new_rows = area.height.saturating_sub(PANEL_HEIGHT + 1);
        
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
                libc::ioctl(master_fd, 0x5414, &win);
            }
        }

        terminal.draw(|f| {
            let area = f.area();
            let term_height = area.height.saturating_sub(PANEL_HEIGHT + 1);
            
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

            let session_text = format!(" {} @ {} ", session, url);
            let text_len = session_text.len() as u16;
            let total_width = sep_area.width;
            let left_dashes = total_width.saturating_sub(text_len + 2) / 2;
            let right_dashes = total_width.saturating_sub(text_len + 2) - left_dashes;

            let sep_line = Line::from(vec![
                Span::styled("├", Style::default().fg(Color::Cyan)),
                Span::styled("─".repeat(left_dashes as usize), Style::default().fg(Color::Cyan)),
                Span::styled(session_text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("─".repeat(right_dashes as usize), Style::default().fg(Color::Cyan)),
                Span::styled("┤", Style::default().fg(Color::Cyan)),
            ]);
            f.render_widget(sep_line, sep_area);

            let web_content: Vec<Line> = if !session_data.is_empty() {
                let wrapped = wrap_text(&session_data, web_area.width as usize);
                wrapped.iter().take(4).map(|s| Line::from(Span::styled(s.clone(), Style::default().add_modifier(Modifier::BOLD)))).collect()
            } else if let Some(ref err) = last_error {
                vec![Line::from(Span::styled(format!("[{}]", err), Style::default().fg(Color::Red)))]
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
                    handle_key_event(key, master_fd);
                }
            }
        }
    }

    restore_terminal()?;
    unsafe { libc::close(master_fd); }

    Ok(())
}
