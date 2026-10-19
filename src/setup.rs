//! `vista init` and `vista uninstall`: add or remove the few lines that run
//! vista when a shell starts.
//!
//! Every change lives between a begin and an end marker, so it can be
//! updated in place and removed cleanly without touching anything else in
//! the user's profile.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use crate::{store, sys};

const BEGIN: &str = ">>> vista >>>";
const END: &str = "<<< vista <<<";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(clippy::enum_variant_names)] // PowerShell is its real name
pub enum Shell {
    PowerShell,
    Cmd,
    Bash,
    Zsh,
}

impl Shell {
    pub fn parse(s: &str) -> Option<Shell> {
        match s.to_ascii_lowercase().as_str() {
            "powershell" | "pwsh" => Some(Shell::PowerShell),
            "cmd" | "clink" => Some(Shell::Cmd),
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            _ => None,
        }
    }

    fn comment(self) -> &'static str {
        match self {
            Shell::Cmd => "rem",
            _ => "#",
        }
    }

    /// The lines that start vista in this shell, without the markers.
    fn body(self) -> String {
        match self {
            Shell::PowerShell => "\
if ($Host.Name -eq 'ConsoleHost' -and $env:TERM_PROGRAM -ne 'vscode' -and -not $env:SSH_CLIENT -and
    (Get-Command vista -ErrorAction SilentlyContinue)) {
    vista
    Write-Host -NoNewline \"$([char]27)[?25h\"
}"
            .into(),
            Shell::Cmd => "if not \"%TERM_PROGRAM%\"==\"vscode\" vista".into(),
            Shell::Bash | Shell::Zsh => format!(
                "\
if [[ $- == *i* && -t 1 && \"$TERM_PROGRAM\" != \"vscode\" && -z \"$SSH_CONNECTION\" && ${{SHLVL:-1}} -le 1 ]]; then
  {} --cols \"${{COLUMNS:-120}}\" --rows \"${{LINES:-40}}\"
  printf '\\e[?25h'
fi",
                unix_command()
            ),
        }
    }

    /// The full block, markers included.
    pub fn block(self) -> String {
        let c = self.comment();
        format!("{c} {BEGIN} added by `vista init`, remove with `vista uninstall`\n{}\n{c} {END}\n", self.body())
    }
}

/// How bash or zsh should call vista. From WSL that is the Windows program
/// under /mnt/c; on Linux and macOS it is just `vista` on the PATH.
fn unix_command() -> String {
    if cfg!(windows) {
        if let Some(p) = std::env::current_exe().ok().and_then(|p| wsl_path(&p)) {
            return format!("'{}'", p.replace('\'', r"'\''"));
        }
    }
    "vista".into()
}

/// C:\Users\Sam\vista.exe  ->  /mnt/c/Users/Sam/vista.exe
fn wsl_path(p: &Path) -> Option<String> {
    let s = p.to_str()?;
    let s = s.strip_prefix(r"\\?\").unwrap_or(s);
    let (drive, rest) = s.split_once(":\\")?;
    (drive.len() == 1).then(|| format!("/mnt/{}/{}", drive.to_ascii_lowercase(), rest.replace('\\', "/")))
}

/// Put `block` into `text`, replacing an earlier vista block if there is one.
/// Returns None when the text already has exactly this block.
fn with_block(text: &str, block: &str) -> Option<String> {
    let updated = match without_block(text) {
        Some((before, after)) => format!("{before}{block}{after}"),
        None if text.is_empty() => block.to_string(),
        None if text.ends_with('\n') => format!("{text}\n{block}"),
        None => format!("{text}\n\n{block}"),
    };
    (updated != text).then_some(updated)
}

/// Split around an existing vista block: (text before, text after).
fn without_block(text: &str) -> Option<(&str, &str)> {
    let start = text.find(BEGIN)?;
    let start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let end_marker = start + text[start..].find(END)?;
    let end = text[end_marker..].find('\n').map_or(text.len(), |i| end_marker + i + 1);
    Some((&text[..start], &text[end..]))
}

/// `text` with the vista block taken out, or None if it has none.
fn removed(text: &str) -> Option<String> {
    let (before, after) = without_block(text)?;
    // Drop the blank line `with_block` put in front of the block
    let before = before.strip_suffix("\n\n").map_or(before.to_string(), |b| format!("{b}\n"));
    Some(before + after)
}

/// A profile file vista can hook into, and whether its shell is installed.
struct Target {
    shell: Shell,
    label: &'static str,
    path: PathBuf,
}

fn targets() -> Vec<Target> {
    let mut out = Vec::new();
    if cfg!(windows) {
        if let Some(docs) = sys::documents_dir() {
            let pwsh = docs.join("PowerShell").join("Microsoft.PowerShell_profile.ps1");
            if sys::on_path("pwsh") || pwsh.exists() {
                out.push(Target { shell: Shell::PowerShell, label: "PowerShell 7", path: pwsh });
            }
            // Windows PowerShell only runs profiles where scripts are allowed,
            // so it's touched only when the user already keeps a profile there.
            let winps = docs.join("WindowsPowerShell").join("Microsoft.PowerShell_profile.ps1");
            if winps.exists() {
                out.push(Target { shell: Shell::PowerShell, label: "Windows PowerShell", path: winps });
            }
        }
        // cmd itself has no profile; Clink gives it one.
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let clink = PathBuf::from(local).join("clink");
            if clink.is_dir() {
                out.push(Target { shell: Shell::Cmd, label: "cmd (Clink)", path: clink.join("clink_start.cmd") });
            }
        }
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for (shell, label, file) in [(Shell::Zsh, "zsh", ".zshrc"), (Shell::Bash, "bash", ".bashrc")] {
            let path = home.join(file);
            if path.exists() {
                out.push(Target { shell, label, path });
            }
        }
    }
    out
}

fn ask_name() -> Option<String> {
    let suggested = crate::user_name(None);
    if !std::io::stdin().is_terminal() {
        return None;
    }
    print!("What should vista call you? [{suggested}] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok()?;
    let name = line.trim();
    Some(if name.is_empty() { suggested } else { name.to_string() })
}

pub fn init(name: Option<String>) -> i32 {
    let targets = targets();
    if targets.is_empty() {
        println!("No shell profile found to set up.");
        println!("Add the lines from `vista init powershell`, `vista init cmd`, `vista init zsh` or `vista init bash` yourself.");
    }
    let mut failed = false;
    for t in &targets {
        let text = std::fs::read_to_string(&t.path).unwrap_or_default();
        let status = match with_block(&text, &t.shell.block()) {
            None => Ok("already set up"),
            Some(new) => t
                .path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|_| std::fs::write(&t.path, new))
                .map(|_| if text.contains(BEGIN) { "updated" } else { "added" }),
        };
        match status {
            Ok(s) => println!("  {:<20} {s}  ({})", t.label, t.path.display()),
            Err(e) => {
                failed = true;
                println!("  {:<20} failed: {e}  ({})", t.label, t.path.display());
            }
        }
    }

    if let Some(name) = name.or_else(|| store::config("name").is_none().then(ask_name).flatten()) {
        match store::set_config("name", &name) {
            Ok(()) => println!("  {:<20} {name}", "name"),
            Err(e) => {
                failed = true;
                println!("  {:<20} could not save: {e}", "name");
            }
        }
    }

    if cfg!(windows) {
        println!();
        println!("Using WSL? Run this inside it to add vista to zsh (or bash):");
        println!("  vista.exe init zsh >> ~/.zshrc");
    }
    println!();
    println!("Open a new terminal tab to see it. Undo everything with `vista uninstall`.");
    i32::from(failed)
}

/// Print the block for one shell, for adding it by hand.
pub fn print(shell: Shell) {
    print!("{}", shell.block());
}

pub fn uninstall() -> i32 {
    let mut failed = false;
    let mut any = false;
    for t in targets() {
        let Ok(text) = std::fs::read_to_string(&t.path) else { continue };
        let Some(new) = removed(&text) else { continue };
        any = true;
        match std::fs::write(&t.path, new) {
            Ok(()) => println!("  {:<20} removed  ({})", t.label, t.path.display()),
            Err(e) => {
                failed = true;
                println!("  {:<20} failed: {e}  ({})", t.label, t.path.display());
            }
        }
    }
    if !any {
        println!("  no vista lines found in your shell profiles");
    }
    match store::remove_all() {
        Ok(Some(dir)) => println!("  {:<20} deleted  ({})", "settings", dir.display()),
        Ok(None) => {}
        Err(e) => {
            failed = true;
            println!("  {:<20} could not delete: {e}", "settings");
        }
    }
    if cfg!(windows) {
        println!();
        println!("Set up vista in WSL too? Delete the lines between `{BEGIN}` and `{END}` in ~/.zshrc or ~/.bashrc.");
    }
    println!("Then delete the vista program itself, the way you installed it.");
    i32::from(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_to_an_empty_file() {
        let block = Shell::Cmd.block();
        assert_eq!(with_block("", &block).unwrap(), block);
    }

    #[test]
    fn appends_after_existing_lines_and_removes_cleanly() {
        let text = "@echo off\ndoskey ls=dir\n";
        let added = with_block(text, &Shell::Cmd.block()).unwrap();
        assert!(added.starts_with(text) && added.contains("vista"));
        assert_eq!(removed(&added).unwrap(), text);
    }

    #[test]
    fn running_twice_changes_nothing() {
        let once = with_block("x\n", &Shell::PowerShell.block()).unwrap();
        assert_eq!(with_block(&once, &Shell::PowerShell.block()), None);
    }

    #[test]
    fn replaces_an_old_block_in_place() {
        let old = format!("a\n# {BEGIN}\nold stuff\n# {END}\nb\n");
        let new = with_block(&old, &Shell::Zsh.block()).unwrap();
        assert!(new.starts_with("a\n# >>> vista") && new.ends_with("<<< vista <<<\nb\n"));
        assert!(!new.contains("old stuff"));
    }

    #[test]
    fn no_block_means_nothing_to_remove() {
        assert_eq!(removed("just my profile\n"), None);
    }

    #[test]
    fn windows_paths_become_wsl_paths() {
        assert_eq!(
            wsl_path(Path::new(r"C:\Users\Sam Lee\.cargo\bin\vista.exe")).unwrap(),
            "/mnt/c/Users/Sam Lee/.cargo/bin/vista.exe"
        );
        assert_eq!(wsl_path(Path::new("/usr/bin/vista")), None);
    }

    #[test]
    fn shells_parse() {
        assert_eq!(Shell::parse("pwsh"), Some(Shell::PowerShell));
        assert_eq!(Shell::parse("ZSH"), Some(Shell::Zsh));
        assert_eq!(Shell::parse("fish"), None);
    }
}
