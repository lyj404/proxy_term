use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub proxy_type: String,
    pub host: String,
    pub port: String,
    pub no_proxy: String,
    pub test_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            proxy_type: "HTTP".to_string(),
            host: "127.0.0.1".to_string(),
            port: "7890".to_string(),
            no_proxy: "localhost,127.0.0.1,::1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16".to_string(),
            test_url: "http://www.google.com".to_string(),
        }
    }
}

pub fn get_config_dir() -> Result<PathBuf, String> {
    let config_dir = if cfg!(target_os = "windows") {
        std::env::var("APPDATA").map_err(|_| "无法获取 APPDATA 目录".to_string())?
    } else {
        let home = std::env::var("HOME").map_err(|_| "无法获取 HOME 目录".to_string())?;
        format!("{}/.config", home)
    };

    let dir = PathBuf::from(config_dir).join("proxy_term");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {}", e))?;

    Ok(dir)
}

fn get_config_path() -> Result<PathBuf, String> {
    Ok(get_config_dir()?.join("config.json"))
}

pub fn load_config() -> AppConfig {
    match get_config_path() {
        Ok(path) => {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                    Err(_) => AppConfig::default(),
                }
            } else {
                AppConfig::default()
            }
        }
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path()?;
    let content =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {}", e))?;
    std::fs::write(&path, content).map_err(|e| format!("写入配置文件失败: {}", e))?;
    Ok(())
}
