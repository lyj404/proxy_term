#![windows_subsystem = "windows"]

mod config;
mod launcher;
mod proxy;

use std::cell::Cell;
use std::rc::Rc;

use slint::SharedString;

slint::include_modules!();

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    MouseButton, TrayIconBuilder, TrayIconEvent,
};

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

    // ── 系统托盘 ──────────────────────────────────────────────

    // 加载图标文件为 RGBA
    let icon_img = image::load_from_memory(include_bytes!("../assets/logo.ico"))
        .map_err(|e| format!("加载托盘图标失败: {}", e))?
        .into_rgba8();
    let (icon_w, icon_h) = icon_img.dimensions();
    let tray_icon = tray_icon::Icon::from_rgba(icon_img.into_raw(), icon_w, icon_h)
        .map_err(|e| format!("创建托盘图标失败: {}", e))?;

    // 创建托盘（含初始菜单 + tooltip）
    let tray = {
        let running = app.get_proxy_running();
        let menu = build_tray_menu(running)?;
        let t = TrayIconBuilder::new()
            .with_tooltip(tooltip_text(running))
            .with_icon(tray_icon)
            .with_menu(Box::new(menu))
            .build()?;
        t.set_show_menu_on_left_click(false);
        Rc::new(t)
    };

    // 定时器：轮询菜单事件 + 同步托盘状态
    let poll_timer = slint::Timer::default();
    let tray_for_timer = tray.clone();
    let app_weak = app.as_weak();
    let last_running = Cell::new(app.get_proxy_running());

    poll_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            let app = match app_weak.upgrade() {
                Some(a) => a,
                None => return,
            };

            // 同步托盘 tooltip 和菜单文字
            let running = app.get_proxy_running();
            if running != last_running.get() {
                last_running.set(running);
                let _ = tray_for_timer.set_tooltip(Some(tooltip_text(running)));
                if let Ok(new_menu) = build_tray_menu(running) {
                    tray_for_timer.set_menu(Some(Box::new(new_menu)));
                }
            }

            // 处理菜单事件（右键）
            if let Ok(event) = MenuEvent::receiver().try_recv() {
                match event.id().0.as_str() {
                    "show" => {
                        let _ = app.window().show();
                    }
                    "toggle-proxy" => {
                        if app.get_proxy_running() {
                            app.invoke_stop_clicked();
                        } else {
                            let proxy_type = if app.get_proxy_type_index() == 1 {
                                SharedString::from("SOCKS5")
                            } else {
                                SharedString::from("HTTP")
                            };
                            app.invoke_launch_clicked(
                                proxy_type,
                                app.get_host(),
                                app.get_port(),
                                app.get_no_proxy(),
                            );
                        }
                    }
                    "exit" => {
                        let _ = slint::quit_event_loop();
                    }
                    _ => {}
                }
            }

            // 处理左键点击托盘图标（切换窗口显示），忽略 Enter/Move/Leave 等事件
            if let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, .. }) {
                    if app.window().is_visible() {
                        let _ = app.window().hide();
                    } else {
                        let _ = app.window().show();
                    }
                }
            }
        },
    );

    // 窗口关闭 → 隐藏到托盘（不退出）
    {
        let app_weak = app.as_weak();
        app.window().on_close_requested(move || {
            if let Some(a) = app_weak.upgrade() {
                let _ = a.window().hide();
            }
            slint::CloseRequestResponse::KeepWindowShown
        });
    }

    // 退出程序（从托盘菜单触发）
    {
        app.on_exit_app(move || {
            let _ = slint::quit_event_loop();
        });
    }

    // ── 保存配置回调 ──────────────────────────────────────────
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

    // ── 启动代理回调 ──────────────────────────────────────────
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

    // ── 停止代理回调 ──────────────────────────────────────────
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
                            app.set_status_text(SharedString::from("代理已停止，新终端将不再使用代理"));
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

    // ── 测试连接回调 ──────────────────────────────────────────
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

    // 必须使用 run_event_loop_until_quit，否则窗口隐藏后事件循环会退出
    app.show()?;
    slint::run_event_loop_until_quit()?;

    Ok(())
}

fn tooltip_text(running: bool) -> &'static str {
    if running {
        "Proxy Term - 代理已启动"
    } else {
        "Proxy Term - 代理未启动"
    }
}

fn build_tray_menu(proxy_running: bool) -> Result<Menu, tray_icon::menu::Error> {
    let toggle_text = if proxy_running {
        "停止代理"
    } else {
        "启动代理"
    };
    Menu::with_items(&[
        &MenuItem::with_id("show", "显示窗口", true, None),
        &MenuItem::with_id("toggle-proxy", toggle_text, true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id("exit", "退出", true, None),
    ])
}
