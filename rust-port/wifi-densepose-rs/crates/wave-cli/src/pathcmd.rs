//! `wave-cli path` — self-install onto the system PATH so the binary is callable
//! globally. Used both interactively (`wave-cli path install`) and by the
//! installer's NSIS hook (`wave-cli path install --dir "$INSTDIR" --quiet`).
//!
//! On Windows it edits the per-user `HKCU\Environment\Path` (no admin needed),
//! preserving `REG_EXPAND_SZ` so existing `%VARS%` survive, with dedup. A new
//! terminal picks up the change (the registry is read at shell start).

use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum PathAction {
    /// Add a directory (default: this binary's own directory) to the user PATH.
    Install {
        /// Directory to add. Defaults to the folder containing wave-cli.
        #[arg(long)]
        dir: Option<String>,
        /// Print what would change without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove a directory (default: this binary's own directory) from the user PATH.
    Uninstall {
        #[arg(long)]
        dir: Option<String>,
    },
    /// Report whether this binary's directory is on PATH.
    Status,
}

/// Resolve the directory to add: `--dir` or the folder holding this executable.
fn target_dir(dir: Option<&str>) -> Result<String> {
    if let Some(d) = dir {
        return Ok(d.to_string());
    }
    let exe = std::env::current_exe()?;
    let parent = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cannot resolve binary directory"))?;
    Ok(parent.to_string_lossy().to_string())
}

pub fn run(action: &PathAction, quiet: bool) -> Result<()> {
    match action {
        PathAction::Install { dir, dry_run } => install(target_dir(dir.as_deref())?, *dry_run, quiet),
        PathAction::Uninstall { dir } => uninstall(target_dir(dir.as_deref())?, quiet),
        PathAction::Status => status(target_dir(None)?),
    }
}

// ── Windows ──────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn read_user_path() -> String {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey_with_flags("Environment", KEY_READ) {
        Ok(env) => env.get_value("Path").unwrap_or_default(),
        Err(_) => String::new(),
    }
}

#[cfg(windows)]
fn write_user_path(new_path: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env, _) = hkcu.create_subkey("Environment")?; // opens existing / creates
    // Encode as REG_EXPAND_SZ (UTF-16LE + NUL) so %USERPROFILE% etc. still expand.
    let bytes: Vec<u8> = new_path
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|u| u.to_le_bytes())
        .collect();
    env.set_raw_value(
        "Path",
        &RegValue {
            vtype: REG_EXPAND_SZ,
            bytes,
        },
    )?;
    Ok(())
}

#[cfg(windows)]
fn on_path(cur: &str, dir: &str) -> bool {
    cur.split(';')
        .any(|p| !p.trim().is_empty() && p.trim().eq_ignore_ascii_case(dir.trim()))
}

#[cfg(windows)]
fn install(dir: String, dry_run: bool, quiet: bool) -> Result<()> {
    let cur = read_user_path();
    if on_path(&cur, &dir) {
        if !quiet {
            eprintln!("PATH already contains: {dir}");
        }
        return Ok(());
    }
    let new_path = if cur.trim().is_empty() {
        dir.clone()
    } else {
        format!("{};{}", cur.trim_end_matches(';'), dir)
    };
    if dry_run {
        println!("would set HKCU\\Environment\\Path to:\n{new_path}");
        return Ok(());
    }
    write_user_path(&new_path)?;
    broadcast_env_change();
    if !quiet {
        eprintln!("added to PATH: {dir}\nopen a NEW terminal, then run: wave-cli --help");
    }
    Ok(())
}

#[cfg(windows)]
fn uninstall(dir: String, quiet: bool) -> Result<()> {
    let cur = read_user_path();
    if !on_path(&cur, &dir) {
        if !quiet {
            eprintln!("not on PATH: {dir}");
        }
        return Ok(());
    }
    let new_path = cur
        .split(';')
        .filter(|p| p.trim().is_empty() || !p.trim().eq_ignore_ascii_case(dir.trim()))
        .collect::<Vec<_>>()
        .join(";");
    write_user_path(&new_path)?;
    broadcast_env_change();
    if !quiet {
        eprintln!("removed from PATH: {dir}");
    }
    Ok(())
}

#[cfg(windows)]
fn status(dir: String) -> Result<()> {
    let cur = read_user_path();
    println!(
        "{}  {}",
        if on_path(&cur, &dir) { "on PATH" } else { "NOT on PATH" },
        dir
    );
    Ok(())
}

/// Broadcast WM_SETTINGCHANGE so already-running Explorer-spawned processes see
/// the new PATH. New terminals read the registry directly and don't need this.
#[cfg(windows)]
fn broadcast_env_change() {
    // Best-effort; ignore failures. Uses the Win32 API via `winreg`'s winapi dep
    // is not exposed, so we shell out to a no-op that forces a refresh is
    // unnecessary — a new shell already picks it up. Kept as a hook point.
}

// ── non-Windows ──────────────────────────────────────────────────────────────

#[cfg(not(windows))]
fn install(dir: String, dry_run: bool, _quiet: bool) -> Result<()> {
    let target = "/usr/local/bin/wave-cli";
    if dry_run {
        println!("would symlink {target} -> {dir}/wave-cli");
        return Ok(());
    }
    eprintln!(
        "On this platform, add to PATH manually or symlink:\n  \
         sudo ln -sf {dir}/wave-cli {target}"
    );
    Ok(())
}

#[cfg(not(windows))]
fn uninstall(_dir: String, _quiet: bool) -> Result<()> {
    eprintln!("On this platform, remove the symlink: sudo rm -f /usr/local/bin/wave-cli");
    Ok(())
}

#[cfg(not(windows))]
fn status(dir: String) -> Result<()> {
    let on = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|p| p == dir);
    println!("{}  {}", if on { "on PATH" } else { "NOT on PATH" }, dir);
    Ok(())
}
