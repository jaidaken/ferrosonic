//! cava subprocess: spawn, drain, config generation.

use std::os::unix::io::FromRawFd;

use tracing::{error, info};

use super::*;

impl App {
    /// Spawn cava on a pty sized to the terminal, replacing any running instance.
    pub fn start_cava(
        &mut self,
        cava_gradient: &[String; 8],
        cava_horizontal_gradient: &[String; 8],
        cava_size: u32,
    ) {
        self.stop_cava();

        // Backstop: remove cava configs leaked by a SIGKILLed prior session.
        crate::io_util::sweep_stale_tmp_files(
            "ferrosonic-cava-",
            ".conf",
            std::time::Duration::from_secs(3600),
        );

        let (term_w, term_h) = crossterm::terminal::size().unwrap_or((80, 24));
        let cava_h = (term_h as u32 * cava_size / 100).max(4) as u16;
        let cava_w = term_w;

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

            let ws = libc::winsize {
                ws_row: cava_h,
                ws_col: cava_w,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            libc::ioctl(slave, libc::TIOCSWINSZ, &ws);
        }

        // Dup before from_raw_fd takes ownership.
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
                    unsafe {
                        libc::close(master);
                    }
                    return;
                }
                f
            }
            Err(e) => {
                error!("Failed to create cava temp file: {}", e);
                unsafe {
                    libc::close(master);
                }
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

        crate::proc_util::set_die_with_parent(&mut cmd);
        match cmd.spawn() {
            Ok(child) => {
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

    /// Kill the cava subprocess and drop its pty state.
    pub fn stop_cava(&mut self) {
        if let Some(ref mut child) = self.cava_process {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.cava_process = None;
        self.cava_pty_master = None;
        self.cava_parser = None;
        self.cava_config = None;
    }

    /// Drains cava pty into client_state.cava_screen for local render.
    pub async fn read_cava_output(&mut self) {
        let (Some(ref mut master), Some(ref mut parser)) =
            (&mut self.cava_pty_master, &mut self.cava_parser)
        else {
            return;
        };

        let outcome = drain_into_parser(master, parser);
        match outcome {
            DrainOutcome::Bytes => {
                let cava_screen = screen_to_cava_rows(parser.screen());
                let mut cs = self.client_state.write().await;
                cs.cava_screen = cava_screen;
            }
            DrainOutcome::NoData => {}
            DrainOutcome::Eof | DrainOutcome::HardError => {
                if let Some(mut child) = self.cava_process.take() {
                    let _ = child.try_wait();
                }
                self.cava_pty_master = None;
                self.cava_parser = None;
            }
        }
    }
}

/// Result of one non-blocking drain of the cava pty.
#[derive(Debug, PartialEq, Eq)]
pub enum DrainOutcome {
    /// Fresh bytes were parsed; the screen changed.
    Bytes,
    /// Nothing available; normal between frames.
    NoData,
    /// Slave end closed; the cava subprocess has exited.
    Eof,
    /// Unrecoverable read error; caller resets cava state.
    HardError,
}

/// Drain `reader` into `parser`. Caller resets state on `Eof` or
/// `HardError`; `NoData` (WouldBlock) is normal between frames.
pub fn drain_into_parser<R: std::io::Read>(
    reader: &mut R,
    parser: &mut vt100::Parser,
) -> DrainOutcome {
    let mut buf = [0u8; 16384];
    let mut got_data = false;
    let mut saw_eof = false;
    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                saw_eof = true;
                break;
            }
            Ok(n) => {
                parser.process(&buf[..n]);
                got_data = true;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => return DrainOutcome::HardError,
        }
    }
    if got_data {
        DrainOutcome::Bytes
    } else if saw_eof {
        DrainOutcome::Eof
    } else {
        DrainOutcome::NoData
    }
}

/// Pure logic: vt100 screen to CavaRow vec. Tests feed bytes into a
/// vt100::Parser then call this directly.
pub fn screen_to_cava_rows(screen: &vt100::Screen) -> Vec<CavaRow> {
    let (rows, cols) = screen.size();
    let mut cava_screen = Vec::with_capacity(rows as usize);

    for row in 0..rows {
        let mut spans: Vec<CavaSpan> = Vec::new();
        let mut cur_text = String::new();
        let mut cur_fg = CavaColor::Default;
        let mut cur_bg = CavaColor::Default;

        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
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
    cava_screen
}

fn vt100_color_to_cava(color: vt100::Color) -> CavaColor {
    match color {
        vt100::Color::Default => CavaColor::Default,
        vt100::Color::Idx(i) => CavaColor::Indexed(i),
        vt100::Color::Rgb(r, g, b) => CavaColor::Rgb(r, g, b),
    }
}

/// Render a cava config with the theme's vertical and horizontal gradients.
pub fn generate_cava_config(g: &[String; 8], h: &[String; 8]) -> String {
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
        g0 = g[0],
        g1 = g[1],
        g2 = g[2],
        g3 = g[3],
        g4 = g[4],
        g5 = g[5],
        g6 = g[6],
        g7 = g[7],
        h0 = h[0],
        h1 = h[1],
        h2 = h[2],
        h3 = h[3],
        h4 = h[4],
        h5 = h[5],
        h6 = h[6],
        h7 = h[7],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(rows: u16, cols: u16, input: &[u8]) -> Vec<CavaRow> {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(input);
        screen_to_cava_rows(parser.screen())
    }

    #[test]
    fn empty_input_yields_empty_rows() {
        let rows = parse(2, 4, b"");
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert!(row.spans.iter().all(|s| s.text.chars().all(|c| c == ' ')));
        }
    }

    #[test]
    fn ascii_text_renders_into_a_single_span() {
        let rows = parse(1, 8, b"hello");
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        let text: String = row.spans.iter().map(|s| s.text.as_str()).collect();
        assert!(text.starts_with("hello"));
    }

    #[test]
    fn fg_color_change_produces_multiple_spans() {
        let mut input = Vec::new();
        input.extend_from_slice(b"\x1b[31mAA");
        input.extend_from_slice(b"\x1b[32mBB");
        let rows = parse(1, 4, &input);
        let row = &rows[0];
        let colors: Vec<CavaColor> = row.spans.iter().map(|s| s.fg).collect();
        assert!(
            colors.windows(2).any(|w| w[0] != w[1]),
            "expected at least one color change across spans; got {:?}",
            colors
        );
    }

    #[test]
    fn rgb_color_round_trips() {
        let input = b"\x1b[38;2;255;128;64mX";
        let rows = parse(1, 1, input);
        let row = &rows[0];
        assert!(
            row.spans
                .iter()
                .any(|s| matches!(s.fg, CavaColor::Rgb(255, 128, 64))),
            "expected RGB(255,128,64) span; got {:?}",
            row.spans.iter().map(|s| s.fg).collect::<Vec<_>>()
        );
    }

    #[test]
    fn indexed_color_is_preserved() {
        let input = b"\x1b[38;5;201mZ";
        let rows = parse(1, 1, input);
        let row = &rows[0];
        assert!(
            row.spans
                .iter()
                .any(|s| matches!(s.fg, CavaColor::Indexed(201))),
            "expected Indexed(201) span"
        );
    }

    #[test]
    fn rows_match_screen_dimensions() {
        let rows = parse(5, 10, b"hi");
        assert_eq!(rows.len(), 5);
    }
}
