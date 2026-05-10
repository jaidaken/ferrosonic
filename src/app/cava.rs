use std::io::Read as _;
use std::os::unix::io::FromRawFd;

use tracing::{error, info};

use super::*;

impl App {
    /// Start cava process in noncurses mode via a pty
    pub(super) fn start_cava(&mut self, cava_gradient: &[String; 8], cava_horizontal_gradient: &[String; 8], cava_size: u32) {
        self.stop_cava();

        // Compute pty dimensions to match the cava widget area
        let (term_w, term_h) = crossterm::terminal::size().unwrap_or((80, 24));
        let cava_h = (term_h as u32 * cava_size / 100).max(4) as u16;
        let cava_w = term_w;

        // Open a pty pair
        let mut master: libc::c_int = 0;
        let mut slave: libc::c_int = 0;
        unsafe {
            if libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ) != 0
            {
                error!("openpty failed");
                return;
            }

            // Set pty size so cava knows its dimensions
            let ws = libc::winsize {
                ws_row: cava_h,
                ws_col: cava_w,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            libc::ioctl(slave, libc::TIOCSWINSZ, &ws);
        }

        // Generate themed cava config and write to temp file
        // Dup slave fd before converting to File (from_raw_fd takes ownership)
        let slave_stdin_fd = unsafe { libc::dup(slave) };
        let slave_stderr_fd = unsafe { libc::dup(slave) };
        let slave_stdout = unsafe { std::fs::File::from_raw_fd(slave) };
        let slave_stdin = unsafe { std::fs::File::from_raw_fd(slave_stdin_fd) };
        let slave_stderr = unsafe { std::fs::File::from_raw_fd(slave_stderr_fd) };
        let cfg_body = generate_cava_config(cava_gradient, cava_horizontal_gradient);
        let cfg = match tempfile::Builder::new()
            .prefix("ferrosonic-cava-")
            .suffix(".conf")
            .tempfile()
        {
            Ok(mut f) => {
                use std::io::Write as _;
                if let Err(e) = f.write_all(cfg_body.as_bytes()) {
                    error!("Failed to write cava config: {}", e);
                    unsafe { libc::close(master); }
                    return;
                }
                f
            }
            Err(e) => {
                error!("Failed to create cava temp file: {}", e);
                unsafe { libc::close(master); }
                return;
            }
        };
        let config_path = cfg.path().to_path_buf();
        let mut cmd = std::process::Command::new("cava");
        cmd.arg("-p").arg(&config_path);
        cmd.stdout(std::process::Stdio::from(slave_stdout))
            .stderr(std::process::Stdio::from(slave_stderr))
            .stdin(std::process::Stdio::from(slave_stdin))
            .env("TERM", "xterm-256color");

        match cmd.spawn() {
            Ok(child) => {
                // Set master to non-blocking
                unsafe {
                    let flags = libc::fcntl(master, libc::F_GETFL);
                    libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }

                let master_file = unsafe { std::fs::File::from_raw_fd(master) };
                let parser = vt100::Parser::new(cava_h, cava_w, 0);

                self.cava_process = Some(child);
                self.cava_pty_master = Some(master_file);
                self.cava_parser = Some(parser);
                self.cava_config = Some(cfg);
                info!("Cava started in noncurses mode ({}x{})", cava_w, cava_h);
            }
            Err(e) => {
                error!("Failed to start cava: {}", e);
                unsafe {
                    libc::close(master);
                }
            }
        }
    }

    /// Stop cava process and clean up
    pub(super) fn stop_cava(&mut self) {
        if let Some(ref mut child) = self.cava_process {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.cava_process = None;
        self.cava_pty_master = None;
        self.cava_parser = None;
        self.cava_config = None;
    }

    /// Read cava pty output and snapshot screen to state
    pub(super) async fn read_cava_output(&mut self) {
        let (Some(ref mut master), Some(ref mut parser)) =
            (&mut self.cava_pty_master, &mut self.cava_parser)
            else {
                return;
            };

            // Read all available bytes from the pty master
            let mut buf = [0u8; 16384];
            let mut got_data = false;
            loop {
                match master.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        parser.process(&buf[..n]);
                        got_data = true;
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => return,
                }
            }

            if !got_data {
                return;
            }

            // Snapshot the vt100 screen into shared state
            let screen = parser.screen();
            let (rows, cols) = screen.size();
            let mut cava_screen = Vec::with_capacity(rows as usize);

            for row in 0..rows {
                let mut spans: Vec<CavaSpan> = Vec::new();
                let mut cur_text = String::new();
                let mut cur_fg = CavaColor::Default;
                let mut cur_bg = CavaColor::Default;

                for col in 0..cols {
                    // bounds come from screen.size() above so this should
                    // always succeed, but defensively skip rather than panic
                    // in the 60Hz render path if vt100 ever changes semantics.
                    let Some(cell) = screen.cell(row, col) else { continue };
                    let fg = vt100_color_to_cava(cell.fgcolor());
                    let bg = vt100_color_to_cava(cell.bgcolor());

                    if fg != cur_fg || bg != cur_bg {
                        if !cur_text.is_empty() {
                            spans.push(CavaSpan {
                                text: std::mem::take(&mut cur_text),
                                fg: cur_fg,
                                bg: cur_bg,
                            });
                        }
                        cur_fg = fg;
                        cur_bg = bg;
                    }

                    let contents = cell.contents();
                    if contents.is_empty() {
                        cur_text.push(' ');
                    } else {
                        cur_text.push_str(&contents);
                    }
                }
                if !cur_text.is_empty() {
                    spans.push(CavaSpan {
                        text: cur_text,
                        fg: cur_fg,
                        bg: cur_bg,
                    });
                }
                cava_screen.push(CavaRow { spans });
            }

            let mut cs = self.client_state.write().await;
            cs.cava_screen = cava_screen;
    }
}

/// Convert vt100 color to our CavaColor type
fn vt100_color_to_cava(color: vt100::Color) -> CavaColor {
    match color {
        vt100::Color::Default => CavaColor::Default,
        vt100::Color::Idx(i) => CavaColor::Indexed(i),
        vt100::Color::Rgb(r, g, b) => CavaColor::Rgb(r, g, b),
    }
}

/// Generate a cava configuration string with theme-appropriate gradient colors
pub(super) fn generate_cava_config(g: &[String; 8], h: &[String; 8]) -> String {

    format!(
        "\
[general]
framerate = 60
autosens = 1
overshoot = 0
bars = 0
bar_width = 1
bar_spacing = 0
lower_cutoff_freq = 10
higher_cutoff_freq = 18000

[input]
sample_rate = 96000
sample_bits = 32
remix = 1

[output]
method = noncurses
orientation = horizontal
channels = mono
mono_option = average
synchronized_sync = 1
disable_blanking = 1

[color]
gradient = 1
gradient_color_1 = '{g0}'
gradient_color_2 = '{g1}'
gradient_color_3 = '{g2}'
gradient_color_4 = '{g3}'
gradient_color_5 = '{g4}'
gradient_color_6 = '{g5}'
gradient_color_7 = '{g6}'
gradient_color_8 = '{g7}'
horizontal_gradient = 1
horizontal_gradient_color_1 = '{h0}'
horizontal_gradient_color_2 = '{h1}'
horizontal_gradient_color_3 = '{h2}'
horizontal_gradient_color_4 = '{h3}'
horizontal_gradient_color_5 = '{h4}'
horizontal_gradient_color_6 = '{h5}'
horizontal_gradient_color_7 = '{h6}'
horizontal_gradient_color_8 = '{h7}'

[smoothing]
monstercat = 0
waves = 0
noise_reduction = 11
",
        g0 = g[0], g1 = g[1], g2 = g[2], g3 = g[3],
        g4 = g[4], g5 = g[5], g6 = g[6], g7 = g[7],
        h0 = h[0], h1 = h[1], h2 = h[2], h3 = h[3],
        h4 = h[4], h5 = h[5], h6 = h[6], h7 = h[7],
    )
}
