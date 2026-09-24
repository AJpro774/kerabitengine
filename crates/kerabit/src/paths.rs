//! User-data and packaged-game roots. Same folder name on every OS so a Mac
//! `~/.kerabit` copy onto a Windows PC lands in `%USERPROFILE%\.kerabit`.

use std::path::{Path, PathBuf};

/// Override with `KERABIT_HOME`. Otherwise `$HOME/.kerabit` /
/// `%USERPROFILE%\.kerabit`.
pub fn user_data_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KERABIT_HOME") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".kerabit"))
}

/// Directory that contains `marker` (for example `levels` or `scenes`).
///
/// Order: macOS `.app` Resources, folder next to the executable (Windows zip),
/// then `fallback` (`CARGO_MANIFEST_DIR` when running from cargo).
pub fn packaged_data_root(marker: &str, fallback: impl AsRef<Path>) -> PathBuf {
    let fallback = fallback.as_ref();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent.file_name().is_some_and(|n| n == "MacOS") {
                let resources = parent.join("../Resources");
                if resources.join(marker).is_dir() {
                    return resources.canonicalize().unwrap_or(resources);
                }
            }
            if parent.join(marker).is_dir() {
                return parent.to_path_buf();
            }
        }
    }
    fallback.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_dir_respects_kerabit_home() {
        let prev = std::env::var_os("KERABIT_HOME");
        std::env::set_var("KERABIT_HOME", "/tmp/kerabit-home-test");
        let dir = user_data_dir().expect("set");
        assert_eq!(dir, PathBuf::from("/tmp/kerabit-home-test"));
        match prev {
            Some(v) => std::env::set_var("KERABIT_HOME", v),
            None => std::env::remove_var("KERABIT_HOME"),
        }
    }

    #[test]
    fn packaged_data_root_falls_back() {
        let fb = PathBuf::from("/no-such-fallback-root");
        assert_eq!(packaged_data_root("levels", &fb), fb);
    }
}
