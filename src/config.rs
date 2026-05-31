use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PresetConfig {
    pub name: String,
    pub proxy_type: String,
    pub host: String,
    pub port: String,
    pub no_proxy: String,
    pub test_url: String,
}

impl Default for PresetConfig {
    fn default() -> Self {
        Self {
            name: "默认".to_string(),
            proxy_type: "HTTP".to_string(),
            host: "127.0.0.1".to_string(),
            port: "7890".to_string(),
            no_proxy: "localhost,127.0.0.1,::1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16".to_string(),
            test_url: "http://www.google.com".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub current_preset: usize,
    pub last_running: bool,
    pub presets: Vec<PresetConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            current_preset: 0,
            last_running: false,
            presets: vec![PresetConfig::default()],
        }
    }
}

impl AppConfig {
    pub fn normalize(mut self) -> Self {
        if self.presets.is_empty() {
            self.presets.push(PresetConfig::default());
        }
        if self.current_preset >= self.presets.len() {
            self.current_preset = 0;
        }
        for (index, preset) in self.presets.iter_mut().enumerate() {
            if preset.name.trim().is_empty() {
                preset.name = format!("预设 {}", index + 1);
            }
        }
        self
    }

    pub fn current(&self) -> &PresetConfig {
        &self.presets[self.current_preset]
    }

    pub fn set_current(&mut self, preset: PresetConfig) {
        if self.current_preset >= self.presets.len() {
            self.current_preset = 0;
        }
        self.presets[self.current_preset] = preset;
    }

    pub fn upsert_preset(&mut self, mut preset: PresetConfig) {
        let name = preset.name.trim();
        if name.is_empty() {
            preset.name = "未命名".to_string();
        } else {
            preset.name = name.to_string();
        }

        if let Some(index) = self.presets.iter().position(|p| p.name == preset.name) {
            self.presets[index] = preset;
            self.current_preset = index;
        } else {
            self.presets.push(preset);
            self.current_preset = self.presets.len() - 1;
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ConfigFile {
    Current(AppConfig),
    Legacy(LegacyConfig),
}

#[derive(Debug, Deserialize)]
struct LegacyConfig {
    proxy_type: String,
    host: String,
    port: String,
    no_proxy: String,
    test_url: String,
}

impl From<LegacyConfig> for AppConfig {
    fn from(value: LegacyConfig) -> Self {
        Self {
            current_preset: 0,
            last_running: false,
            presets: vec![PresetConfig {
                name: "默认".to_string(),
                proxy_type: value.proxy_type,
                host: value.host,
                port: value.port,
                no_proxy: value.no_proxy,
                test_url: value.test_url,
            }],
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

pub fn load_config() -> (AppConfig, Option<String>) {
    let Ok(path) = get_config_path() else {
        return (
            AppConfig::default(),
            Some("无法访问配置目录，已使用默认配置".to_string()),
        );
    };

    if !path.exists() {
        return (AppConfig::default(), None);
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<ConfigFile>(&content) {
            Ok(ConfigFile::Current(config)) => (config.normalize(), None),
            Ok(ConfigFile::Legacy(config)) => (AppConfig::from(config).normalize(), None),
            Err(_) => (
                AppConfig::default(),
                Some("配置文件损坏，已使用默认配置".to_string()),
            ),
        },
        Err(_) => (
            AppConfig::default(),
            Some("读取配置失败，已使用默认配置".to_string()),
        ),
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path()?;
    let content =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {}", e))?;
    std::fs::write(&path, content).map_err(|e| format!("写入配置文件失败: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_migrates_to_single_preset() {
        let json = r#"{
            "proxy_type": "SOCKS5",
            "host": "127.0.0.1",
            "port": "1080",
            "no_proxy": "localhost",
            "test_url": "https://example.com"
        }"#;

        let parsed = serde_json::from_str::<ConfigFile>(json).unwrap();
        let config = match parsed {
            ConfigFile::Legacy(legacy) => AppConfig::from(legacy).normalize(),
            ConfigFile::Current(_) => panic!("expected legacy config"),
        };

        assert_eq!(config.current_preset, 0);
        assert!(!config.last_running);
        assert_eq!(config.presets.len(), 1);
        assert_eq!(config.current().name, "默认");
        assert_eq!(config.current().proxy_type, "SOCKS5");
    }

    #[test]
    fn normalize_adds_default_preset_and_bounds_index() {
        let config = AppConfig {
            current_preset: 3,
            last_running: true,
            presets: Vec::new(),
        }
        .normalize();

        assert_eq!(config.current_preset, 0);
        assert_eq!(config.presets.len(), 1);
        assert_eq!(config.current().name, "默认");
    }

    #[test]
    fn upsert_preset_replaces_existing_name() {
        let mut config = AppConfig::default();
        let mut preset = PresetConfig {
            name: "默认".to_string(),
            ..PresetConfig::default()
        };
        preset.port = "1080".to_string();

        config.upsert_preset(preset);

        assert_eq!(config.presets.len(), 1);
        assert_eq!(config.current().port, "1080");
    }
}
