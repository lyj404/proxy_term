#[cfg(target_os = "windows")]
use winreg::enums::*;
#[cfg(target_os = "windows")]
use winreg::RegKey;

#[cfg(target_os = "windows")]
use crate::config;
use crate::proxy::ProxyConfig;

#[allow(dead_code)]
const START_MARKER: &str = "# Proxy Term - Start";
#[allow(dead_code)]
const END_MARKER: &str = "# Proxy Term - End";
#[cfg(target_os = "windows")]
const WINDOWS_PROXY_KEYS: [&str; 8] = [
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
];

#[cfg(target_os = "windows")]
type SavedEnv = std::collections::HashMap<String, Option<String>>;

#[cfg(target_os = "windows")]
fn broadcast_env_change() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    const HWND_BROADCAST: isize = 0xFFFF;
    const WM_SETTINGCHANGE: u32 = 0x001A;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;

    extern "system" {
        fn SendMessageTimeoutW(
            hWnd: isize,
            Msg: u32,
            wParam: usize,
            lParam: isize,
            fuFlags: u32,
            uTimeout: u32,
            lpdwResult: *mut usize,
        ) -> isize;
    }

    let wide: Vec<u16> = OsStr::new("Environment")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            wide.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            ptr::null_mut(),
        );
    }
}

pub fn set_proxy_env(config: &ProxyConfig) -> Result<(), String> {
    let env_vars = config.to_env_vars();

    #[cfg(target_os = "windows")]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env_key = hkcu
            .open_subkey_with_flags("Environment", KEY_SET_VALUE | KEY_QUERY_VALUE)
            .map_err(|e| format!("打开注册表失败: {}", e))?;

        save_existing_windows_env(&env_key)?;

        for (key, value) in &env_vars {
            env_key
                .set_value(key.as_str(), value)
                .map_err(|e| format!("设置 {} 失败: {}", key, e))?;
        }

        broadcast_env_change();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shell = detect_user_shell();
        let rc_file = get_shell_rc_file(&shell)?;

        let mut lines = Vec::new();
        lines.push(START_MARKER.to_string());
        for (key, value) in &env_vars {
            let escaped = shell_single_quote(value);
            match shell.as_str() {
                "fish" => lines.push(format!("set -gx {} {}", key, escaped)),
                _ => lines.push(format!("export {}={}", key, escaped)),
            }
        }
        lines.push(END_MARKER.to_string());
        let new_config = lines.join("\n");

        let content = std::fs::read_to_string(&rc_file).unwrap_or_default();
        let new_content = replace_or_append_config(&content, &new_config);
        std::fs::write(&rc_file, new_content).map_err(|e| format!("写入文件失败: {}", e))?;
    }

    Ok(())
}

pub fn unset_proxy_env() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env_key = hkcu
            .open_subkey_with_flags("Environment", KEY_SET_VALUE | KEY_QUERY_VALUE)
            .map_err(|e| format!("打开注册表失败: {}", e))?;

        restore_windows_env(&env_key)?;

        broadcast_env_change();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shell = detect_user_shell();
        let rc_file = get_shell_rc_file(&shell)?;

        if let Ok(content) = std::fs::read_to_string(&rc_file) {
            let new_content = remove_config(&content);
            std::fs::write(&rc_file, new_content).map_err(|e| format!("写入文件失败: {}", e))?;
        }
    }

    Ok(())
}

pub fn is_proxy_env_set() -> bool {
    #[cfg(target_os = "windows")]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(env_key) = hkcu.open_subkey_with_flags("Environment", KEY_QUERY_VALUE) else {
            return false;
        };
        WINDOWS_PROXY_KEYS
            .iter()
            .any(|key| env_key.get_value::<String, _>(key).is_ok())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shell = detect_user_shell();
        let Ok(rc_file) = get_shell_rc_file(&shell) else {
            return false;
        };
        let Ok(content) = std::fs::read_to_string(&rc_file) else {
            return false;
        };
        content.contains(START_MARKER) && content.contains(END_MARKER)
    }
}

pub fn open_terminal() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("powershell.exe")
            .spawn()
            .map_err(|e| format!("打开终端失败: {}", e))?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-a")
            .arg("Terminal")
            .spawn()
            .map_err(|e| format!("打开终端失败: {}", e))?;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let terminals = [
            "x-terminal-emulator",
            "kgx",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal",
            "alacritty",
            "kitty",
            "xterm",
        ];
        let mut last_error = None;
        for terminal in terminals {
            match std::process::Command::new(terminal).spawn() {
                Ok(_) => return Ok(()),
                Err(e) => last_error = Some(e),
            }
        }
        Err(format!(
            "打开终端失败: {}",
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "未找到可用终端".to_string())
        ))
    }
}

#[cfg(not(target_os = "windows"))]
fn detect_user_shell() -> String {
    // 优先读取 SHELL 环境变量
    if let Ok(shell) = std::env::var("SHELL") {
        if let Some(name) = std::path::Path::new(&shell).file_name() {
            return name.to_string_lossy().to_string();
        }
    }

    // 默认返回 bash
    "bash".to_string()
}

#[cfg(not(target_os = "windows"))]
fn get_shell_rc_file(shell: &str) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|_| "无法获取 HOME 目录".to_string())?;

    let rc_file = match shell {
        "bash" => format!("{}/.bashrc", home),
        "zsh" => format!("{}/.zshrc", home),
        "fish" => format!("{}/.config/fish/config.fish", home),
        _ => format!("{}/.profile", home),
    };

    Ok(rc_file)
}

#[cfg(target_os = "windows")]
fn windows_state_path() -> Result<std::path::PathBuf, String> {
    Ok(config::get_config_dir()?.join("windows-env-backup.json"))
}

#[cfg(target_os = "windows")]
fn save_existing_windows_env(env_key: &RegKey) -> Result<(), String> {
    let path = windows_state_path()?;
    if path.exists() {
        return Ok(());
    }

    let mut saved = SavedEnv::new();
    for key in WINDOWS_PROXY_KEYS {
        let value = env_key.get_value::<String, _>(key).ok();
        saved.insert(key.to_string(), value);
    }

    let content = serde_json::to_string_pretty(&saved)
        .map_err(|e| format!("序列化环境变量备份失败: {}", e))?;
    std::fs::write(path, content).map_err(|e| format!("写入环境变量备份失败: {}", e))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn restore_windows_env(env_key: &RegKey) -> Result<(), String> {
    let path = windows_state_path()?;
    if !path.exists() {
        for key in WINDOWS_PROXY_KEYS {
            let _ = env_key.delete_value(key);
        }
        return Ok(());
    }

    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("读取环境变量备份失败: {}", e))?;
    let saved: SavedEnv =
        serde_json::from_str(&content).map_err(|e| format!("解析环境变量备份失败: {}", e))?;

    for key in WINDOWS_PROXY_KEYS {
        match saved.get(key).and_then(|value| value.as_ref()) {
            Some(value) => env_key
                .set_value(key, value)
                .map_err(|e| format!("恢复 {} 失败: {}", key, e))?,
            None => {
                let _ = env_key.delete_value(key);
            }
        }
    }

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[allow(dead_code)]
fn replace_or_append_config(content: &str, new_config: &str) -> String {
    if let Some(start) = content.find(START_MARKER) {
        let end_search_start = start + START_MARKER.len();
        if let Some(relative_end) = content[end_search_start..].find(END_MARKER) {
            let end = end_search_start + relative_end;
            let end_pos = end + END_MARKER.len();
            // 替换标记及其之间的内容
            let before = &content[..start];
            let after = &content[end_pos..];
            // 移除多余的空行
            let before = before.trim_end_matches('\n');
            let after = after.trim_start_matches('\n');
            return format!("{}\n{}\n{}", before, new_config, after);
        }
    }

    // 没有找到标记，在文件末尾追加
    let content = content.trim_end_matches('\n');
    if content.is_empty() {
        new_config.to_string()
    } else {
        format!("{}\n\n{}", content, new_config)
    }
}

#[allow(dead_code)]
fn remove_config(content: &str) -> String {
    if let Some(start) = content.find(START_MARKER) {
        let end_search_start = start + START_MARKER.len();
        if let Some(relative_end) = content[end_search_start..].find(END_MARKER) {
            let end = end_search_start + relative_end;
            let end_pos = end + END_MARKER.len();
            let before = &content[..start];
            let after = &content[end_pos..];
            // 移除多余的空行
            let before = before.trim_end_matches('\n');
            let after = after.trim_start_matches('\n');
            if before.is_empty() {
                return after.to_string();
            } else if after.is_empty() {
                return before.to_string();
            } else {
                return format!("{}\n{}", before, after);
            }
        }
    }
    content.to_string()
}

pub fn test_proxy_connection(config: &ProxyConfig, test_url: &str) -> Result<String, String> {
    let proxy_url = config.proxy_url();
    let start = std::time::Instant::now();

    let proxy = ureq::Proxy::new(&proxy_url).map_err(|e| format!("无效的代理地址: {}", e))?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .proxy(Some(proxy))
        .timeout_global(Some(std::time::Duration::from_secs(5)))
        .build()
        .into();

    let response = agent
        .get(test_url)
        .call()
        .map_err(|e| format!("连接失败: {}", e))?;

    let status = response.status().as_u16();
    let elapsed = start.elapsed().as_millis();
    if (200..400).contains(&status) {
        let mut msg = format!("连接成功 (HTTP {}, {} ms)", status, elapsed);
        if let Ok(body) = response.into_body().read_to_string() {
            let text = body.trim();
            if is_likely_ip(text) {
                msg.push_str(&format!(", 出口 IP {}", text));
            }
        }
        Ok(msg)
    } else {
        Err(format!("连接失败 (HTTP {}, {} ms)", status, elapsed))
    }
}

fn is_likely_ip(value: &str) -> bool {
    if value.len() > 45 || value.is_empty() {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii_hexdigit() || matches!(c, '.' | ':'))
        && (value.contains('.') || value.contains(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_or_append_config() {
        let content = "# some config\nalias ll='ls -la'";
        let new_config =
            "# Proxy Term - Start\nexport http_proxy=\"http://127.0.0.1:7890\"\n# Proxy Term - End";

        let result = replace_or_append_config(content, new_config);
        assert!(result.contains("# Proxy Term - Start"));
        assert!(result.contains("alias ll='ls -la'"));
    }

    #[test]
    fn test_replace_existing_config() {
        let content = "# some config\n# Proxy Term - Start\nexport http_proxy=\"old\"\n# Proxy Term - End\nalias ll='ls -la'";
        let new_config = "# Proxy Term - Start\nexport http_proxy=\"new\"\n# Proxy Term - End";

        let result = replace_or_append_config(content, new_config);
        assert!(result.contains("http_proxy=\"new\""));
        assert!(!result.contains("http_proxy=\"old\""));
        assert!(result.contains("alias ll='ls -la'"));
    }

    #[test]
    fn test_remove_config() {
        let content = "# some config\n# Proxy Term - Start\nexport http_proxy=\"http://127.0.0.1:7890\"\n# Proxy Term - End\nalias ll='ls -la'";

        let result = remove_config(content);
        assert!(!result.contains("# Proxy Term - Start"));
        assert!(!result.contains("http_proxy"));
        assert!(result.contains("alias ll='ls -la'"));
    }

    #[test]
    fn test_marker_end_before_start_is_ignored() {
        let content = "# Proxy Term - End\nkeep\n# Proxy Term - Start\nold";
        let new_config = "# Proxy Term - Start\nnew\n# Proxy Term - End";

        let result = replace_or_append_config(content, new_config);
        assert!(result.starts_with(content));
        assert!(result.ends_with(new_config));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_shell_single_quote_escapes_quotes_and_substitution() {
        let result = shell_single_quote("a'b$(touch /tmp/nope)");
        assert_eq!(result, "'a'\\''b$(touch /tmp/nope)'");
    }

    #[test]
    fn test_is_likely_ip() {
        assert!(is_likely_ip("127.0.0.1"));
        assert!(is_likely_ip("2001:db8::1"));
        assert!(!is_likely_ip("hello"));
        assert!(!is_likely_ip(""));
    }
}
