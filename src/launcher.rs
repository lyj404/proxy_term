use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::proxy::ProxyConfig;

#[allow(dead_code)]
const START_MARKER: &str = "# Proxy Term - Start";
#[allow(dead_code)]
const END_MARKER: &str = "# Proxy Term - End";

pub fn set_proxy_env(config: &ProxyConfig) -> Result<(), String> {
    let env_vars = config.to_env_vars();

    #[cfg(target_os = "windows")]
    {
        for (key, value) in &env_vars {
            let output = Command::new("setx")
                .args(&[key, value])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output()
                .map_err(|e| format!("执行 setx 失败: {}", e))?;

            if !output.status.success() {
                return Err(format!("设置 {} 失败", key));
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shell = detect_user_shell();
        let rc_file = get_shell_rc_file(&shell)?;

        let mut lines = Vec::new();
        lines.push(START_MARKER.to_string());
        for (key, value) in &env_vars {
            match shell.as_str() {
                "fish" => lines.push(format!("set -gx {} \"{}\"", key, value)),
                _ => lines.push(format!("export {}=\"{}\"", key, value)),
            }
        }
        lines.push(END_MARKER.to_string());
        let new_config = lines.join("\n");

        let content = std::fs::read_to_string(&rc_file).unwrap_or_default();
        let new_content = replace_or_append_config(&content, &new_config);
        std::fs::write(&rc_file, new_content)
            .map_err(|e| format!("写入文件失败: {}", e))?;
    }

    Ok(())
}

pub fn unset_proxy_env() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let keys = vec![
            "http_proxy", "https_proxy", "all_proxy", "no_proxy",
            "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY",
        ];

        for key in keys {
            let _ = Command::new("reg")
                .args(&["delete", "HKCU\\Environment", "/v", key, "/f"])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output();
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shell = detect_user_shell();
        let rc_file = get_shell_rc_file(&shell)?;

        if let Ok(content) = std::fs::read_to_string(&rc_file) {
            let new_content = remove_config(&content);
            std::fs::write(&rc_file, new_content)
                .map_err(|e| format!("写入文件失败: {}", e))?;
        }
    }

    Ok(())
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

#[allow(dead_code)]
fn replace_or_append_config(content: &str, new_config: &str) -> String {
    if let Some(start) = content.find(START_MARKER) {
        if let Some(end) = content.find(END_MARKER) {
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
        if let Some(end) = content.find(END_MARKER) {
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

    let proxy = ureq::Proxy::new(&proxy_url)
        .map_err(|e| format!("无效的代理地址: {}", e))?;
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
    if (200..400).contains(&status) {
        Ok(format!("连接成功 (HTTP {})", status))
    } else {
        Err(format!("连接失败 (HTTP {})", status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_or_append_config() {
        let content = "# some config\nalias ll='ls -la'";
        let new_config = "# Proxy Term - Start\nexport http_proxy=\"http://127.0.0.1:7890\"\n# Proxy Term - End";

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
}
