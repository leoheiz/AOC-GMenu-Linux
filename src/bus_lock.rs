//! Single-flight `flock` across processos (GUI / tray / CLI).

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

/// Guard: libera o lock ao dropar.
#[derive(Debug)]
pub struct BusLockGuard {
    file: File,
}

impl Drop for BusLockGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = libc_flock(self.file.as_raw_fd(), LOCK_UN);
        }
    }
}

const LOCK_EX: i32 = 2;
const LOCK_UN: i32 = 8;

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

unsafe fn libc_flock(fd: i32, op: i32) -> io::Result<()> {
    let rc = flock(fd, op);
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Caminho padrão: `$XDG_RUNTIME_DIR/aoc-gmenu.lock` ou `/tmp/aoc-gmenu-$UID.lock`.
pub fn default_lock_path() -> PathBuf {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        if !runtime.is_empty() {
            return PathBuf::from(runtime).join("aoc-gmenu.lock");
        }
    }
    let uid = unsafe { libc_getuid() };
    PathBuf::from(format!("/tmp/aoc-gmenu-{uid}.lock"))
}

extern "C" {
    fn getuid() -> u32;
}

unsafe fn libc_getuid() -> u32 {
    getuid()
}

/// Adquire lock exclusivo (bloqueante) no caminho padrão.
pub fn acquire() -> Result<BusLockGuard, String> {
    acquire_at(&default_lock_path())
}

/// Adquire lock exclusivo em `path` (útil em testes com dir temporário).
pub fn acquire_at(path: &Path) -> Result<BusLockGuard, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("lock dir: {e}"))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|e| format!("abrir lock {}: {e}", path.display()))?;
    unsafe {
        libc_flock(file.as_raw_fd(), LOCK_EX)
            .map_err(|e| format!("flock {}: {e}", path.display()))?;
    }
    Ok(BusLockGuard { file })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn lock_creates_file_and_releases() {
        let dir = std::env::temp_dir().join(format!(
            "aoc-gmenu-lock-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("aoc-gmenu.lock");
        {
            let _g = acquire_at(&path).unwrap();
            assert!(path.exists());
            // Segundo lock no mesmo processo com outro fd bloquearia — só
            // verificamos que o arquivo existe sob o guard.
        }
        // Após drop o arquivo permanece; o flock foi liberado.
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_path_uses_runtime_or_tmp() {
        let p = default_lock_path();
        let s = p.to_string_lossy();
        assert!(
            s.contains("aoc-gmenu") && s.ends_with(".lock"),
            "unexpected path {s}"
        );
    }
}
