use clap::Parser;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    style::{Attribute, Color, Print, SetAttribute, SetForegroundColor},
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use std::io::{self, Write};
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

const PANEL_HEIGHT: usize = 5;

fn color_from_vt100(fg: vt100::Color) -> Color {
    match fg {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => match i {
            0 => Color::Black,
            1 => Color::DarkRed,
            2 => Color::DarkGreen,
            3 => Color::DarkYellow,
            4 => Color::DarkBlue,
            5 => Color::DarkMagenta,
            6 => Color::DarkCyan,
            7 => Color::Grey,
            8 => Color::DarkGrey,
            9 => Color::Red,
            10 => Color::Green,
            11 => Color::Yellow,
            12 => Color::Blue,
            13 => Color::Magenta,
            14 => Color::Cyan,
            15 => Color::White,
            _ => Color::Reset,
        },
        vt100::Color::Rgb(r, g, b) => Color::Rgb { r, g, b },
    }
}

fn render_screen(screen: &vt100::Screen, stdout: &mut io::Stdout) {
    let (rows, cols) = screen.size();

    for row in 0..rows {
        for col in 0..cols {
            if let Some(cell) = screen.cell(row, col) {
                stdout.execute(MoveTo(col as u16, row as u16)).ok();

                let c = if cell.contents().is_empty() { ' ' } else { cell.contents().chars().next().unwrap_or(' ') };

                let mut attrs = Vec::new();
                if cell.bold() { attrs.push(Attribute::Bold); }
                if cell.underline() { attrs.push(Attribute::Underlined); }
                if cell.inverse() { attrs.push(Attribute::Reverse); }

                let fg = cell.fgcolor();
                let fg_color = color_from_vt100(fg);

                for attr in &attrs {
                    stdout.execute(SetAttribute(*attr)).ok();
                }
                stdout.execute(SetForegroundColor(fg_color)).ok();
                stdout.execute(Print(c)).ok();
                stdout.execute(SetAttribute(Attribute::Reset)).ok();
            }
        }
    }
    stdout.flush().ok();
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

    let main_rows = (rows.saturating_sub(PANEL_HEIGHT + 1)) as u16;
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

    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen).ok();
    stdout.execute(Hide).ok();
    stdout.execute(Clear(ClearType::All)).ok();

    let mut parser = vt100::Parser::new(main_rows, cols, 0);

    let mut buf = [0u8; 4096];
    use crossterm::event::{poll, read, Event, KeyCode, KeyEventKind};

    let rt = tokio::runtime::Runtime::new().unwrap();
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
                    stdout.execute(Clear(ClearType::All)).ok();
                    render_screen(parser.screen(), &mut stdout);

                    let sep_y = main_rows as i32;

                    stdout.execute(SetForegroundColor(Color::Cyan)).ok();
                    stdout.execute(MoveTo(0, sep_y as u16)).ok();
                    stdout.execute(Print("├")).ok();

                    let session_text = format!(" {} @ {} ", session, url);
                    let text_start = ((cols as usize).saturating_sub(session_text.len())) / 2;
                    let text_start = std::cmp::max(1, text_start) as u16;

                    for x in 1..text_start {
                        stdout.execute(MoveTo(x, sep_y as u16)).ok();
                        stdout.execute(Print("─")).ok();
                    }

                    stdout.execute(MoveTo(text_start, sep_y as u16)).ok();
                    stdout.execute(SetAttribute(Attribute::Bold)).ok();
                    stdout.execute(Print(&session_text)).ok();
                    stdout.execute(SetAttribute(Attribute::Reset)).ok();

                    for x in (text_start as usize + session_text.len())..(cols as usize - 1) {
                        stdout.execute(MoveTo(x as u16, sep_y as u16)).ok();
                        stdout.execute(Print("─")).ok();
                    }

                    stdout.execute(MoveTo(cols - 1, sep_y as u16)).ok();
                    stdout.execute(Print("┤")).ok();
                    stdout.execute(SetForegroundColor(Color::Reset)).ok();

                    let web_start = sep_y as u16 + 1;
                    let web_lines = 4;

                    if !session_data.is_empty() {
                        let wrapped = wrap_text(&session_data, cols as usize - 1);
                        for (i, line) in wrapped.iter().take(web_lines).enumerate() {
                            stdout.execute(MoveTo(0, web_start + i as u16)).ok();
                            stdout.execute(SetAttribute(Attribute::Bold)).ok();
                            stdout.execute(Print(line)).ok();
                            stdout.execute(SetAttribute(Attribute::Reset)).ok();
                        }
                    } else if let Some(ref err) = last_error {
                        stdout.execute(MoveTo(0, web_start)).ok();
                        stdout.execute(SetForegroundColor(Color::Red)).ok();
                        stdout.execute(Print(format!("[{}]", err))).ok();
                        stdout.execute(SetForegroundColor(Color::Reset)).ok();
                    } else {
                        stdout.execute(MoveTo(0, web_start)).ok();
                        stdout.execute(SetForegroundColor(Color::Cyan)).ok();
                        stdout.execute(Print("[web panel empty - send data via web interface]")).ok();
                        stdout.execute(SetForegroundColor(Color::Reset)).ok();
                    }

                    stdout.flush().ok();
                } else if n == 0 {
                    break;
                }
            }
        }

        match poll(std::time::Duration::from_millis(10)) {
            Ok(true) => {
                if let Ok(Event::Key(key)) = read() {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char(c) => {
                                let bytes = [c as u8];
                                unsafe { libc::write(master_fd, bytes.as_ptr() as *const libc::c_void, 1); }
                            }
                            KeyCode::Enter => {
                                unsafe { libc::write(master_fd, b"\n".as_ptr() as *const libc::c_void, 1); }
                            }
                            KeyCode::Backspace => {
                                let b = [0x7f];
                                unsafe { libc::write(master_fd, b.as_ptr() as *const libc::c_void, 1); }
                            }
                            KeyCode::Up => {
                                unsafe { libc::write(master_fd, b"\x1b[A".as_ptr() as *const libc::c_void, 4); }
                            }
                            KeyCode::Down => {
                                unsafe { libc::write(master_fd, b"\x1b[B".as_ptr() as *const libc::c_void, 4); }
                            }
                            KeyCode::Right => {
                                unsafe { libc::write(master_fd, b"\x1b[C".as_ptr() as *const libc::c_void, 4); }
                            }
                            KeyCode::Left => {
                                unsafe { libc::write(master_fd, b"\x1b[D".as_ptr() as *const libc::c_void, 4); }
                            }
                            KeyCode::Esc => {
                                unsafe { libc::write(master_fd, b"\x1b".as_ptr() as *const libc::c_void, 1); }
                            }
                            KeyCode::Tab => {
                                unsafe { libc::write(master_fd, b"\t".as_ptr() as *const libc::c_void, 1); }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
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
    }

    stdout.execute(LeaveAlternateScreen).ok();
    stdout.execute(Show).ok();

    unsafe {
        libc::close(master_fd);
    }

    Ok(())
}
