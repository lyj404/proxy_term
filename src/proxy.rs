use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ProxyType {
    Http,
    Socks5,
}

impl ProxyType {

    pub fn scheme(&self) -> &str {
        match self {
            ProxyType::Http => "http",
            ProxyType::Socks5 => "socks5",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub no_proxy: String,
}

impl ProxyConfig {
    pub fn new(proxy_type: ProxyType, host: &str, port: u16, no_proxy: &str) -> Self {
        Self {
            proxy_type,
            host: host.to_string(),
            port,
            no_proxy: no_proxy.to_string(),
        }
    }

    pub fn proxy_url(&self) -> String {
        format!("{}://{}:{}", self.proxy_type.scheme(), self.host, self.port)
    }

    pub fn to_env_vars(&self) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        let url = self.proxy_url();

        match self.proxy_type {
            ProxyType::Http => {
                vars.insert("http_proxy".to_string(), url.clone());
                vars.insert("https_proxy".to_string(), url.clone());
                vars.insert("HTTP_PROXY".to_string(), url.clone());
                vars.insert("HTTPS_PROXY".to_string(), url.clone());
            }
            ProxyType::Socks5 => {
                vars.insert("all_proxy".to_string(), url.clone());
                vars.insert("ALL_PROXY".to_string(), url.clone());
            }
        }

        if !self.no_proxy.is_empty() {
            vars.insert("no_proxy".to_string(), self.no_proxy.clone());
            vars.insert("NO_PROXY".to_string(), self.no_proxy.clone());
        }

        vars
    }
}

pub fn validate_host(host: &str) -> Result<(), String> {
    if host.trim().is_empty() {
        return Err("错误: 代理地址不能为空".to_string());
    }
    if contains_control_or_newline(host) {
        return Err("错误: 代理地址不能包含控制字符或换行".to_string());
    }
    Ok(())
}

pub fn validate_no_proxy(no_proxy: &str) -> Result<(), String> {
    if contains_control_or_newline(no_proxy) {
        return Err("错误: 绕过列表不能包含控制字符或换行".to_string());
    }
    Ok(())
}

fn contains_control_or_newline(value: &str) -> bool {
    value.chars().any(|c| c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_proxy_env_vars() {
        let config = ProxyConfig::new(ProxyType::Http, "127.0.0.1", 7890, "localhost");
        let vars = config.to_env_vars();
        assert_eq!(vars.get("http_proxy").unwrap(), "http://127.0.0.1:7890");
        assert_eq!(vars.get("https_proxy").unwrap(), "http://127.0.0.1:7890");
        assert_eq!(vars.get("no_proxy").unwrap(), "localhost");
    }

    #[test]
    fn test_socks5_proxy_env_vars() {
        let config = ProxyConfig::new(ProxyType::Socks5, "127.0.0.1", 1080, "");
        let vars = config.to_env_vars();
        assert_eq!(vars.get("all_proxy").unwrap(), "socks5://127.0.0.1:1080");
        assert!(!vars.contains_key("no_proxy"));
    }

    #[test]
    fn test_validate_host_rejects_empty_and_control_chars() {
        assert!(validate_host("").is_err());
        assert!(validate_host("127.0.0.1\nexport BAD=1").is_err());
        assert!(validate_host("127.0.0.1").is_ok());
    }

    #[test]
    fn test_validate_no_proxy_rejects_control_chars() {
        assert!(validate_no_proxy("localhost,127.0.0.1").is_ok());
        assert!(validate_no_proxy("localhost\nbad").is_err());
    }
}
