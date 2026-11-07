//! Draws the app in memory, with no display, for `CSLS_SCREENSHOT` and never on the user's desktop.
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{EventLoopProxy, Platform, WindowAdapter};
use slint::{EventLoopError, PhysicalSize, PlatformError, Rgb8Pixel};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Task = Box<dyn FnOnce() + Send>;

#[derive(Clone, Default)]
struct Queue {
    tasks: Arc<Mutex<VecDeque<Task>>>,
    quit: Arc<AtomicBool>,
}

impl EventLoopProxy for Queue {
    fn quit_event_loop(&self) -> Result<(), EventLoopError> {
        self.quit.store(true, Ordering::Relaxed);
        Ok(())
    }
    fn invoke_from_event_loop(&self, event: Task) -> Result<(), EventLoopError> {
        self.tasks.lock().map_err(|_| EventLoopError::EventLoopTerminated)?.push_back(event);
        Ok(())
    }
}

struct Headless {
    window: Rc<MinimalSoftwareWindow>,
    queue: Queue,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(self.queue.clone()))
    }
    fn run_event_loop(&self) -> Result<(), PlatformError> {
        while !self.queue.quit.load(Ordering::Relaxed) {
            slint::platform::update_timers_and_animations();
            let next = self.queue.tasks.lock().ok().and_then(|mut q| q.pop_front());
            match next {
                Some(task) => task(),
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        Ok(())
    }
}

/// Size of the picture, the same as the window's preferred size.
const SIZE: (u32, u32) = (1040, 700);

/// Installs the platform. Call before creating any window.
pub fn install() -> Rc<MinimalSoftwareWindow> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless { window: window.clone(), queue: Queue::default() })).expect("first platform");
    window.set_size(PhysicalSize::new(SIZE.0, SIZE.1));
    window
}

/// Writes what the window shows to a PNG.
pub fn save(window: &MinimalSoftwareWindow, path: &std::path::Path) -> std::io::Result<()> {
    let (w, h) = (SIZE.0 as usize, SIZE.1 as usize);
    let mut buf = vec![Rgb8Pixel::new(0, 0, 0); w * h];
    window.request_redraw();
    window.draw_if_needed(|r| {
        r.render(&mut buf, w);
    });
    let mut rows = Vec::with_capacity((w * 3 + 1) * h);
    for row in buf.chunks(w) {
        rows.push(0);
        rows.extend(row.iter().flat_map(|p| [p.r, p.g, p.b]));
    }
    std::fs::write(path, png(SIZE.0, SIZE.1, &rows))
}

fn crc(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in data {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// A PNG without compression. shortcut: stored deflate, big files; only for debugging screenshots.
fn png(w: u32, h: u32, rows: &[u8]) -> Vec<u8> {
    let mut z = vec![0x78, 0x01];
    let mut chunks = rows.chunks(65535).peekable();
    while let Some(c) = chunks.next() {
        z.push(u8::from(chunks.peek().is_none()));
        z.extend((c.len() as u16).to_le_bytes());
        z.extend((!(c.len() as u16)).to_le_bytes());
        z.extend(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for x in rows {
        a = (a + u32::from(*x)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    for (kind, data) in [(b"IHDR", &ihdr[..]), (b"IDAT", &z[..]), (b"IEND", &[][..])] {
        out.extend((data.len() as u32).to_be_bytes());
        let mut with_kind = kind.to_vec();
        with_kind.extend(data);
        out.extend(&with_kind);
        out.extend(crc(&with_kind).to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn png_has_the_known_checksums() {
        assert_eq!(super::crc(b"IEND"), 0xAE42_6082);
        let p = super::png(1, 1, &[0, 255, 0, 0]);
        assert_eq!(&p[1..4], b"PNG");
        assert_eq!(&p[p.len() - 8..p.len() - 4], b"IEND");
    }
}
