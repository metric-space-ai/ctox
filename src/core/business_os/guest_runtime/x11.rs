// Origin: CTOX
// License: AGPL-3.0-only

//! Local X11 effects inside an already provisioned Linux guest. Configuration
//! comes from native guest startup, never from the model's request. No SSH,
//! network listener, VM startup or host-wide process management lives here.

use super::{
    identifier, GuestDriver, GuestFrame, GuestInput, GuestKey, MouseButton, ScrollDirection,
};
use anyhow::{ensure, Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

const FRAME_LIMIT: usize = 16 * 1024 * 1024;
const EFFECT_TIMEOUT: Duration = Duration::from_secs(5);

pub(in crate::business_os) struct X11GuestConfig {
    pub guest_id: String,
    pub display: String,
    pub xauthority: PathBuf,
}

pub(in crate::business_os) struct X11GuestDriver {
    config: X11GuestConfig,
}

impl X11GuestDriver {
    pub(in crate::business_os) fn new(config: X11GuestConfig) -> Result<Self> {
        ensure!(
            cfg!(target_os = "linux"),
            "X11 guest backend requires Linux"
        );
        ensure!(identifier(&config.guest_id), "guest identity is invalid");
        let Some(display) = config.display.strip_prefix(':') else {
            anyhow::bail!("guest display must be a local X11 display");
        };
        let parts = display.split('.').collect::<Vec<_>>();
        ensure!(
            (1..=2).contains(&parts.len())
                && parts.iter().all(|part| !part.is_empty()
                    && part.len() <= 5
                    && part.bytes().all(|b| b.is_ascii_digit())),
            "guest display is invalid"
        );
        ensure!(
            config.xauthority.is_absolute(),
            "guest Xauthority must be a native absolute path"
        );
        Ok(Self { config })
    }

    async fn run(
        &self,
        program: &'static str,
        args: &[String],
        input: Option<&str>,
        limit: usize,
    ) -> Result<Vec<u8>> {
        let mut command = Command::new(program);
        command
            .args(args)
            .env_clear()
            .env("DISPLAY", &self.config.display)
            .env("XAUTHORITY", &self.config.xauthority)
            .env("LANG", "C.UTF-8")
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .context("guest desktop helper could not start")?;
        let mut stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .context("guest helper stdout is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("guest helper stderr is unavailable")?;
        let result = tokio::time::timeout(EFFECT_TIMEOUT, async {
            let write = async {
                if let (Some(mut pipe), Some(text)) = (stdin.take(), input) {
                    pipe.write_all(text.as_bytes()).await?;
                    pipe.shutdown().await?;
                }
                Ok::<(), anyhow::Error>(())
            };
            let (output, _, ()) = tokio::try_join!(
                read_bounded(stdout, limit),
                read_bounded(stderr, 8192),
                write,
            )?;
            let status = child.wait().await?;
            ensure!(
                status.success(),
                "guest desktop helper rejected the operation"
            );
            Ok::<_, anyhow::Error>(output)
        })
        .await;
        match result {
            Ok(Ok(output)) => Ok(output),
            error => {
                // Kill and reap only the child this operation spawned. A
                // dropped request also kills its child through kill_on_drop.
                let _ = child.kill().await;
                match error {
                    Ok(Err(error)) => Err(error),
                    Err(_) => anyhow::bail!("guest desktop operation timed out"),
                    Ok(Ok(_)) => unreachable!(),
                }
            }
        }
    }

    async fn dimensions(&self) -> Result<(u32, u32)> {
        let output = self
            .run(
                "/usr/bin/xdotool",
                &["getdisplaygeometry".into()],
                None,
                128,
            )
            .await?;
        let text = std::str::from_utf8(&output).context("guest display geometry is invalid")?;
        let values = text.split_whitespace().collect::<Vec<_>>();
        ensure!(values.len() == 2, "guest display geometry is invalid");
        let width: u32 = values[0].parse().context("guest width is invalid")?;
        let height: u32 = values[1].parse().context("guest height is invalid")?;
        validate_dimensions(width, height)?;
        Ok((width, height))
    }
}

async fn read_bounded(reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    ensure!(
        bytes.len() <= limit,
        "guest desktop helper output exceeded its limit"
    );
    Ok(bytes)
}

fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    ensure!(
        width > 0
            && height > 0
            && width <= 4096
            && height <= 4096
            && u64::from(width) * u64::from(height) <= 8_388_608,
        "guest display dimensions exceed the limit"
    );
    Ok(())
}

fn png_frame(png: Vec<u8>, dimensions: (u32, u32)) -> Result<GuestFrame> {
    ensure!(
        png.len() >= 33
            && png.len() <= FRAME_LIMIT
            && &png[..8] == b"\x89PNG\r\n\x1a\n"
            && &png[12..16] == b"IHDR"
            && png[8..12] == 13u32.to_be_bytes(),
        "guest capture is not a bounded PNG frame"
    );
    let width = u32::from_be_bytes(png[16..20].try_into()?);
    let height = u32::from_be_bytes(png[20..24].try_into()?);
    validate_dimensions(width, height)?;
    ensure!(
        (width, height) == dimensions,
        "guest display changed during capture"
    );
    Ok(GuestFrame { png, width, height })
}

fn input_arguments(input: &GuestInput) -> (Vec<String>, Option<&str>) {
    let at = |x: u32, y: u32| {
        vec![
            "mousemove".into(),
            "--sync".into(),
            x.to_string(),
            y.to_string(),
        ]
    };
    match input {
        GuestInput::Click { x, y, button } => {
            let mut args = at(*x, *y);
            args.extend([
                "click".into(),
                match button {
                    MouseButton::Left => "1",
                    MouseButton::Middle => "2",
                    MouseButton::Right => "3",
                }
                .into(),
            ]);
            (args, None)
        }
        GuestInput::Type { text } => (
            vec![
                "type".into(),
                "--clearmodifiers".into(),
                "--delay".into(),
                "0".into(),
                "--file".into(),
                "-".into(),
            ],
            Some(text),
        ),
        GuestInput::Scroll {
            x,
            y,
            direction,
            steps,
        } => {
            let mut args = at(*x, *y);
            args.extend([
                "click".into(),
                "--repeat".into(),
                steps.to_string(),
                "--delay".into(),
                "10".into(),
                match direction {
                    ScrollDirection::Up => "4",
                    ScrollDirection::Down => "5",
                    ScrollDirection::Left => "6",
                    ScrollDirection::Right => "7",
                }
                .into(),
            ]);
            (args, None)
        }
        GuestInput::Key { key } => (
            vec![
                "key".into(),
                "--clearmodifiers".into(),
                match key {
                    GuestKey::Enter => "Return",
                    GuestKey::Escape => "Escape",
                    GuestKey::Tab => "Tab",
                    GuestKey::Backspace => "BackSpace",
                    GuestKey::Delete => "Delete",
                    GuestKey::Up => "Up",
                    GuestKey::Down => "Down",
                    GuestKey::Left => "Left",
                    GuestKey::Right => "Right",
                    GuestKey::Home => "Home",
                    GuestKey::End => "End",
                    GuestKey::PageUp => "Prior",
                    GuestKey::PageDown => "Next",
                    GuestKey::SelectAll => "ctrl+a",
                    GuestKey::Copy => "ctrl+c",
                    GuestKey::Paste => "ctrl+v",
                }
                .into(),
            ],
            None,
        ),
    }
}

impl GuestDriver for X11GuestDriver {
    fn guest_id(&self) -> &str {
        &self.config.guest_id
    }

    async fn capture(&self) -> Result<GuestFrame> {
        let dimensions = self.dimensions().await?;
        let png = self
            .run(
                "/usr/bin/maim",
                &["--format".into(), "png".into(), "--hidecursor".into()],
                None,
                FRAME_LIMIT,
            )
            .await?;
        png_frame(png, dimensions)
    }

    async fn input(&self, input: &GuestInput) -> Result<()> {
        input.validate()?;
        let (width, height) = self.dimensions().await?;
        if let GuestInput::Click { x, y, .. } | GuestInput::Scroll { x, y, .. } = input {
            ensure!(
                *x < width && *y < height,
                "guest input lies outside the current display"
            );
        }
        let (args, text) = input_arguments(input);
        self.run("/usr/bin/xdotool", &args, text, 1024).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn real_x11_capture_pointer_and_text_use_the_guest_display() -> Result<()> {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let root = tempfile::tempdir()?;
        let xauthority = root.path().join("test-Xauthority");
        std::fs::write(&xauthority, [])?;
        let mut server = Command::new("/usr/bin/Xvfb")
            .args([
                "-displayfd",
                "1",
                "-screen",
                "0",
                "800x600x24",
                "-nolisten",
                "tcp",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        eprintln!("guest_x11_smoke xvfb_pid={:?} owner=real_x11_capture_pointer_and_text stop=test-end-or-drop", server.id());
        let result = async {
            let mut line = String::new();
            let mut ready = BufReader::new(server.stdout.take().context("Xvfb readiness pipe")?);
            tokio::time::timeout(EFFECT_TIMEOUT, ready.read_line(&mut line)).await??;
            let driver = X11GuestDriver::new(X11GuestConfig {
                guest_id: "isolated-ci-guest".into(),
                display: format!(":{}", line.trim()),
                xauthority,
            })?;
            let mut events = Command::new("/usr/bin/stdbuf")
                .args(["-oL", "/usr/bin/xev", "-name", "ctox-guest-driver-test", "-geometry", "200x200+0+0", "-event", "keyboard"])
                .env_clear().env("DISPLAY", &driver.config.display)
                .env("XAUTHORITY", &driver.config.xauthority)
                .stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn()?;
            eprintln!("guest_x11_smoke xev_pid={:?} owner=real_x11_capture_pointer_and_text stop=test-end-or-drop", events.id());
            let result = async {
                driver.run("/usr/bin/xdotool", &[
                    "search".into(), "--sync".into(), "--onlyvisible".into(), "--name".into(),
                    "ctox-guest-driver-test".into(), "windowfocus".into(), "--sync".into(),
                ], None, 4096).await?;
                let frame = driver.capture().await?;
                assert_eq!((frame.width, frame.height), (800, 600));
                assert!(frame.png.len() > 33);
                driver.input(&GuestInput::Click { x: 12, y: 14, button: MouseButton::Left }).await?;
                let pointer = driver.run("/usr/bin/xdotool",
                    &["getmouselocation".into(), "--shell".into()], None, 1024).await?;
                let pointer = std::str::from_utf8(&pointer)?;
                assert!(pointer.lines().any(|line| line == "X=12"));
                assert!(pointer.lines().any(|line| line == "Y=14"));
                driver.input(&GuestInput::Type { text: "hi".into() }).await?;
                let mut observed = BufReader::new(events.stdout.take().context("xev event pipe")?);
                tokio::time::timeout(EFFECT_TIMEOUT, async {
                    loop {
                        let mut line = String::new();
                        ensure!(observed.read_line(&mut line).await? > 0, "xev exited before keyboard receipt");
                        if line.contains("keysym 0x68, h") { break; }
                    }
                    Ok::<(), anyhow::Error>(())
                }).await??;
                assert!(driver.input(&GuestInput::Click { x: 800, y: 0, button: MouseButton::Left }).await.is_err());
                Ok::<(), anyhow::Error>(())
            }.await;
            let _ = events.kill().await;
            result
        }.await;
        let _ = server.kill().await;
        result
    }

    #[test]
    fn text_goes_to_stdin_and_cannot_add_helper_arguments() {
        let input = GuestInput::Type {
            text: "--window 1; $(secret)\n".into(),
        };
        let (args, stdin) = input_arguments(&input);
        assert_eq!(
            args,
            ["type", "--clearmodifiers", "--delay", "0", "--file", "-"]
        );
        assert_eq!(stdin, Some("--window 1; $(secret)\n"));
    }

    #[tokio::test]
    async fn helper_output_is_bounded_before_it_is_returned() {
        assert!(read_bounded(&b"12345"[..], 4).await.is_err());
        assert_eq!(read_bounded(&b"1234"[..], 4).await.unwrap(), b"1234");
    }

    #[test]
    fn png_header_rejects_changed_or_unbounded_display() {
        let mut png = vec![0; 33];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[8..12].copy_from_slice(&13u32.to_be_bytes());
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&800u32.to_be_bytes());
        png[20..24].copy_from_slice(&600u32.to_be_bytes());
        assert!(png_frame(png.clone(), (800, 600)).is_ok());
        assert!(png_frame(png.clone(), (1024, 768)).is_err());
        png[16..20].copy_from_slice(&8192u32.to_be_bytes());
        assert!(png_frame(png, (8192, 600)).is_err());
    }
}
