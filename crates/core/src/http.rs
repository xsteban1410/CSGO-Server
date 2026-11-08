//! Downloads through `curl`, the one HTTP client of the app. Only https, also after redirects, and never more
//! than the given size, so a wrong or hostile answer cannot fill the disk.
use anyhow::{bail, Context};
use std::path::Path;
use std::process::Command;

pub const KB: u64 = 1024;
pub const MB: u64 = 1024 * KB;

/// Tests serve their files with file://, which is never allowed outside them.
const PROTO: &str = if cfg!(test) { "=https,file" } else { "=https" };

fn curl(max_bytes: u64, timeout_s: u32) -> Command {
    let mut c = Command::new("curl");
    c.args(["-fsSL", "--proto", PROTO, "--proto-redir", "=https", "--max-time"])
        .arg(timeout_s.to_string())
        .arg("--max-filesize")
        .arg(max_bytes.to_string());
    c
}

/// The body of a request as text. `args` are the URL and any curl data options (`-d`).
pub fn text(args: &[String], max_bytes: u64) -> crate::Result<String> {
    let o = curl(max_bytes, 30).args(args).output().context("running curl")?;
    if !o.status.success() {
        bail!("network error: {}", String::from_utf8_lossy(&o.stderr).trim());
    }
    if o.stdout.len() as u64 > max_bytes {
        bail!("the answer is larger than {} KB", max_bytes / KB);
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Downloads `url` to `to` through `<to>.part`, so a failed download never leaves a file with the final name.
/// `expected` is the size the source announced, when it did.
pub fn download(url: &str, to: &Path, max_bytes: u64, timeout_s: u32, expected: Option<u64>) -> crate::Result<()> {
    let part = to.with_extension(match to.extension() {
        Some(e) => format!("{}.part", e.to_string_lossy()),
        None => "part".into(),
    });
    let result = (|| {
        let o = curl(max_bytes, timeout_s).arg("-o").arg(&part).arg(url).output().context("running curl")?;
        if !o.status.success() {
            bail!("the download failed: {}", String::from_utf8_lossy(&o.stderr).trim());
        }
        let got = std::fs::metadata(&part)?.len();
        if got > max_bytes {
            bail!("the download is larger than {} MB", max_bytes / MB);
        }
        if let Some(e) = expected.filter(|e| *e > 0 && *e != got) {
            bail!("the download is incomplete: {got} of {e} bytes");
        }
        std::fs::rename(&part, to)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_and_plain_http() {
        let dir = std::env::temp_dir().join(format!("csls-http-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.bin");
        std::fs::write(&src, vec![0u8; 4096]).unwrap();
        let url = format!("file://{}", src.display());
        let to = dir.join("x.bin");
        assert!(download(&url, &to, 1024, 5, None).is_err(), "over the limit");
        assert!(download(&url, &to, MB, 5, Some(10)).is_err(), "not the announced size");
        assert!(!to.exists() && !dir.join("x.bin.part").exists());
        download(&url, &to, MB, 5, Some(4096)).unwrap();
        assert!(text(&["http://127.0.0.1:9/".into()], MB).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
