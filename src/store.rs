//! Small key=value files in %APPDATA%\vista:
//!   config.txt  settings you edit (name=Sam)
//!   state.txt   when a terminal last opened, to skip the fog on quick reopens

use std::collections::HashMap;
use std::path::PathBuf;

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("vista"))
}

fn read(file: &str) -> HashMap<String, String> {
    let text = dir().and_then(|d| std::fs::read_to_string(d.join(file)).ok()).unwrap_or_default();
    parse(&text)
}

fn parse(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

pub fn config(key: &str) -> Option<String> {
    read("config.txt").remove(key).filter(|v| !v.is_empty())
}

/// Set `key` in config.txt, keeping comments and every other line as they are.
pub fn set_config(key: &str, value: &str) -> std::io::Result<()> {
    let d = dir().ok_or_else(|| std::io::Error::other("no settings folder"))?;
    std::fs::create_dir_all(&d)?;
    let path = d.join("config.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| "# vista settings\n".into());
    std::fs::write(path, with_setting(&text, key, value))
}

fn with_setting(text: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| match l.split_once('=') {
            Some((k, _)) if !l.trim_start().starts_with('#') && k.trim() == key => {
                found = true;
                format!("{key}={value}")
            }
            _ => l.to_string(),
        })
        .collect();
    if !found {
        out.push(format!("{key}={value}"));
    }
    out.join("\n") + "\n"
}

/// Delete the settings folder. Returns it if there was one.
pub fn remove_all() -> std::io::Result<Option<PathBuf>> {
    match dir() {
        Some(d) if d.is_dir() => std::fs::remove_dir_all(&d).map(|_| Some(d)),
        _ => Ok(None),
    }
}

/// Record this opening and return seconds since the previous one.
pub fn touch_last_open(now_unix: u64, save: bool) -> Option<u64> {
    let prev = read("state.txt").get("last_open_unix").and_then(|v| v.parse::<u64>().ok());
    if save {
        if let Some(d) = dir() {
            let _ = std::fs::create_dir_all(&d);
            let _ = std::fs::write(d.join("state.txt"), format!("last_open_unix={now_unix}\n"));
        }
    }
    prev.map(|p| now_unix.saturating_sub(p))
}

#[cfg(test)]
mod tests {
    use super::{parse, with_setting};

    #[test]
    fn setting_replaces_or_appends() {
        assert_eq!(with_setting("# c\nname=A\nsize=50\n", "name", "Sam"), "# c\nname=Sam\nsize=50\n");
        assert_eq!(with_setting("# c\n", "name", "Sam"), "# c\nname=Sam\n");
        assert_eq!(with_setting("# name=x\n", "name", "Sam"), "# name=x\nname=Sam\n");
    }

    #[test]
    fn parses_keys_and_skips_comments() {
        let m = parse("# hi\nname = Aldy\nbroken line\nfog=1.5\n");
        assert_eq!(m.get("name").map(String::as_str), Some("Aldy"));
        assert_eq!(m.get("fog").map(String::as_str), Some("1.5"));
        assert_eq!(m.len(), 2);
    }
}
