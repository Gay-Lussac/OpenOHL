// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The memory this computer has (owner, 2026-11-12: the memory a
//! calculation may take is a setting of the whole software, offered up to
//! what the computer has).

use std::sync::OnceLock;

/// The physical memory of this computer, bytes: asked of the system once
/// (macOS `sysctl hw.memsize`, Linux `/proc/meminfo`, Windows through
/// PowerShell), then kept. `None`: not found out.
///
/// Expected cost: the first call a few milliseconds (macOS, Linux) to
/// about a second (Windows, PowerShell starting); later calls nothing.
pub fn system_bytes() -> Option<u64> {
    static FOUND: OnceLock<Option<u64>> = OnceLock::new();
    *FOUND.get_or_init(ask)
}

#[cfg(target_os = "macos")]
fn ask() -> Option<u64> {
    let out = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(target_os = "linux")]
fn ask() -> Option<u64> {
    meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(windows)]
fn ask() -> Option<u64> {
    use std::os::windows::process::CommandExt;
    // No console window flashing up.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn ask() -> Option<u64> {
    None
}

/// `MemTotal` of a `/proc/meminfo` text, bytes.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn meminfo(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_is_read() {
        let text = "MemTotal:       16303428 kB\nMemFree:         1234 kB\n";
        assert_eq!(meminfo(text), Some(16_303_428 * 1024));
        assert_eq!(meminfo("nothing"), None);
    }

    /// On the computers the tests run on, found and plausible.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn this_computer_has_memory() {
        let b = system_bytes().expect("found");
        assert!(b > 512 * 1024 * 1024, "{b}");
    }
}
