#![windows_subsystem = "windows"]

mod config;
mod launcher;
mod proxy;

use slint::SharedString;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = AppWindow::new()?;

    // 加载配置
    let saved_config = config::load_config();

    // 设置初始值
    app.set_proxy_type_index(match saved_config.proxy_type.as_str() {
        "SOCKS5" => 1,
        _ => 0,
    });
    app.set_host(SharedString::from(&saved_config.host));
    app.set_port(SharedString::from(&saved_config.port));
    app.set_no_proxy(SharedString::from(&saved_config.no_proxy));
    app.set_test_url(SharedString::from(&saved_config.test_url));

    // 保存配置回调
    {
        let app_weak = app.as_weak();
        app.on_save_config(move |proxy_type, host, port, no_proxy, test_url| {
            let app = app_weak.unwrap();

            let cfg = config::AppConfig {
                proxy_type: proxy_type.to_string(),
                host: host.to_string(),
                port: port.to_string(),
                no_proxy: no_proxy.to_string(),
                test_url: test_url.to_string(),
            };

            match config::save_config(&cfg) {
                Ok(_) => {
                    app.set_status_text(SharedString::from("配置已保存"));
                    app.set_status_type(1);
                }
                Err(e) => {
                    app.set_status_text(SharedString::from(format!("保存失败: {}", e)));
                    app.set_status_type(2);
                }
            }
        });
    }

    // 启动代理回调
    {
        let app_weak = app.as_weak();

        app.on_launch_clicked(move |proxy_type_str, host, port, no_proxy| {
            let app = app_weak.unwrap();

            let proxy_type = match proxy_type_str.as_str() {
                "SOCKS5" => proxy::ProxyType::Socks5,
                _ => proxy::ProxyType::Http,
            };

            let port_num: u16 = match port.to_string().parse() {
                Ok(p) => p,
                Err(_) => {
                    app.set_status_text(SharedString::from("错误: 无效的端口号"));
                    app.set_status_type(2);
                    return;
                }
            };

            let config = proxy::ProxyConfig::new(
                proxy_type,
                &host.to_string(),
                port_num,
                &no_proxy.to_string(),
            );

            app.set_status_text(SharedString::from("正在设置代理..."));
            app.set_status_type(0);

            let app_weak2 = app.as_weak();
            std::thread::spawn(move || {
                let result = launcher::set_proxy_env(&config);

                let _ = slint::invoke_from_event_loop(move || {
                    let app = app_weak2.unwrap();

                    match result {
                        Ok(_) => {
                            app.set_proxy_running(true);
                            app.set_status_text(SharedString::from("代理已启动，请打开新终端使用"));
                            app.set_status_type(1);
                        }
                        Err(e) => {
                            app.set_status_text(SharedString::from(format!("启动失败: {}", e)));
                            app.set_status_type(2);
                        }
                    }
                });
            });
        });
    }

    // 停止代理回调
    {
        let app_weak = app.as_weak();

        app.on_stop_clicked(move || {
            let app = app_weak.unwrap();

            app.set_status_text(SharedString::from("正在停止代理..."));
            app.set_status_type(0);

            let app_weak2 = app.as_weak();
            std::thread::spawn(move || {
                let result = launcher::unset_proxy_env();

                let _ = slint::invoke_from_event_loop(move || {
                    let app = app_weak2.unwrap();

                    match result {
                        Ok(_) => {
                            app.set_proxy_running(false);
                            app.set_status_text(SharedString::from("代理已停止，请打开新终端生效"));
                            app.set_status_type(0);
                        }
                        Err(e) => {
                            app.set_status_text(SharedString::from(format!("停止失败: {}", e)));
                            app.set_status_type(2);
                        }
                    }
                });
            });
        });
    }

    // 测试连接回调
    {
        let app_weak = app.as_weak();

        app.on_test_clicked(move |proxy_type_str, host, port, test_url| {
            let app = app_weak.unwrap();

            let proxy_type = match proxy_type_str.as_str() {
                "SOCKS5" => proxy::ProxyType::Socks5,
                _ => proxy::ProxyType::Http,
            };

            let port_num: u16 = match port.to_string().parse() {
                Ok(p) => p,
                Err(_) => {
                    app.set_status_text(SharedString::from("错误: 无效的端口号"));
                    app.set_status_type(2);
                    return;
                }
            };

            let config = proxy::ProxyConfig::new(
                proxy_type,
                &host.to_string(),
                port_num,
                "",
            );

            app.set_status_text(SharedString::from("正在测试连接..."));
            app.set_status_type(0);

            let app_weak2 = app.as_weak();
            let test_url = test_url.to_string();
            std::thread::spawn(move || {
                let result = launcher::test_proxy_connection(&config, &test_url);

                let _ = slint::invoke_from_event_loop(move || {
                    let app = app_weak2.unwrap();

                    match result {
                        Ok(msg) => {
                            app.set_status_text(SharedString::from(msg));
                            app.set_status_type(1);
                        }
                        Err(e) => {
                            app.set_status_text(SharedString::from(e));
                            app.set_status_type(2);
                        }
                    }
                });
            });
        });
    }

    app.run()?;

    Ok(())
}
