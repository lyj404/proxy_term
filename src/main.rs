#![windows_subsystem = "windows"]

mod config;
mod launcher;
mod proxy;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use slint::{ModelRc, SharedString, VecModel};

slint::include_modules!();

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    MouseButton, TrayIconBuilder, TrayIconEvent,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 GTK（Linux 系统托盘依赖，需要在创建托盘前完成）
    #[cfg(target_os = "linux")]
    gtk::init().map_err(|e| format!("GTK 初始化失败: {}", e))?;

    let app = AppWindow::new()?;

    // 加载配置
    let (saved_config, config_warning) = config::load_config();
    let app_config = Arc::new(Mutex::new(saved_config));
    let initial_running = app_config.lock().unwrap().last_running || launcher::is_proxy_env_set();

    // 设置初始值
    {
        let config = app_config.lock().unwrap();
        sync_presets_to_app(&app, &config);
        apply_preset_to_app(&app, config.current());
    }
    app.set_proxy_running(initial_running);
    if let Some(warning) = config_warning {
        app.set_status_text(SharedString::from(warning));
        app.set_status_type(2);
    } else if initial_running {
        app.set_status_text(SharedString::from("检测到代理已启动"));
        app.set_status_type(1);
    }

    // ── 系统托盘 ──────────────────────────────────────────────

    // 嵌入预解码的图标 RGBA 数据（由 build.rs 生成）
    mod icon_data {
        include!(concat!(env!("OUT_DIR"), "/icon.rs"));
    }
    let tray_icon = tray_icon::Icon::from_rgba(
        icon_data::ICON_RGBA.to_vec(),
        icon_data::ICON_WIDTH,
        icon_data::ICON_HEIGHT,
    )
    .map_err(|e| format!("创建托盘图标失败: {}", e))?;

    // 创建托盘（含初始菜单 + tooltip）
    let tray = {
        let running = app.get_proxy_running();
        let config = app_config.lock().unwrap();
        let menu = build_tray_menu(running, &config)?;
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
    let last_menu_signature = Rc::new(RefCell::new(menu_signature(
        app.get_proxy_running(),
        &app_config.lock().unwrap(),
    )));
    let app_config_for_timer = app_config.clone();
    let last_menu_signature_for_timer = last_menu_signature.clone();

    poll_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(200),
            move || {
            let app = match app_weak.upgrade() {
                Some(a) => a,
                None => return,
            };

            // 同步托盘 tooltip 和菜单文字
            let running = app.get_proxy_running();
            let signature = menu_signature(running, &app_config_for_timer.lock().unwrap());
            if running != last_running.get() {
                last_running.set(running);
                let _ = tray_for_timer.set_tooltip(Some(tooltip_text(running)));
            }
            if signature != *last_menu_signature_for_timer.borrow() {
                *last_menu_signature_for_timer.borrow_mut() = signature;
                let config = app_config_for_timer.lock().unwrap();
                if let Ok(new_menu) = build_tray_menu(running, &config) {
                    tray_for_timer.set_menu(Some(Box::new(new_menu)));
                }
            }

            // 处理菜单事件（右键）
            if let Ok(event) = MenuEvent::receiver().try_recv() {
                let id = event.id().0.as_str();
                match id {
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
                    "test-proxy" => {
                        let proxy_type = if app.get_proxy_type_index() == 1 {
                            SharedString::from("SOCKS5")
                        } else {
                            SharedString::from("HTTP")
                        };
                        app.invoke_test_clicked(
                            proxy_type,
                            app.get_host(),
                            app.get_port(),
                            app.get_test_url(),
                        );
                    }
                    "open-terminal" => {
                        app.invoke_open_terminal();
                    }
                    "exit" => {
                        let _ = slint::quit_event_loop();
                    }
                    _ if id.starts_with("preset:") => {
                        if let Ok(index) = id["preset:".len()..].parse::<usize>() {
                            app.invoke_preset_selected(index as i32);
                            if app.get_proxy_running() {
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
                    }
                    _ => {}
                }
            }

            // 处理左键点击托盘图标（切换窗口显示），忽略 Enter/Move/Leave 等事件
            if let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        ..
                    }
                ) {
                    if app.window().is_visible() {
                        let _ = app.window().hide();
                    } else {
                        let _ = app.window().show();
                    }
                }
            }
        },
    );

    // AppIndicator/GTK owns the Linux tray menu; pump GTK events from Slint's loop.
    #[cfg(target_os = "linux")]
    let _gtk_event_timer = {
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(200),
            || {
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                }
            },
        );
        timer
    };

    // 窗口关闭 → 隐藏到托盘（不退出）
    {
        app.window()
            .on_close_requested(|| slint::CloseRequestResponse::HideWindow);
    }

    // 退出程序（从托盘菜单触发）
    {
        app.on_exit_app(move || {
            let _ = slint::quit_event_loop();
        });
    }

    // ── 预设回调 ──────────────────────────────────────────────
    {
        let app_weak = app.as_weak();
        let app_config = app_config.clone();
        app.on_preset_selected(move |index| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };

            let mut config_ref = app_config.lock().unwrap();
            let index = index as usize;
            if index >= config_ref.presets.len() {
                return;
            }
            config_ref.current_preset = index;
            let preset = config_ref.current().clone();
            drop(config_ref);
            apply_preset_to_app(&app, &preset);
            let config_ref = app_config.lock().unwrap();
            sync_presets_to_app(&app, &config_ref);
            let _ = config::save_config(&config_ref);
            app.set_status_text(SharedString::from(format!("已切换到预设: {}", preset.name)));
            app.set_status_type(1);
        });
    }

    {
        let app_weak = app.as_weak();
        let app_config = app_config.clone();
        app.on_save_config(
            move |preset_index, preset_name, proxy_type, host, port, no_proxy, test_url| {
                let Some(app) = app_weak.upgrade() else {
                    return;
                };

                let mut app_config = app_config.lock().unwrap();
                let preset_index = preset_index as usize;
                if preset_index < app_config.presets.len() {
                    app_config.current_preset = preset_index;
                }
                let cfg = config::PresetConfig {
                    name: preset_name.to_string(),
                    proxy_type: proxy_type.to_string(),
                    host: host.to_string(),
                    port: port.to_string(),
                    no_proxy: no_proxy.to_string(),
                    test_url: test_url.to_string(),
                };
                app_config.set_current(cfg);
                sync_presets_to_app(&app, &app_config);

                match config::save_config(&app_config) {
                    Ok(_) => {
                        app.set_status_text(SharedString::from("配置已保存"));
                        app.set_status_type(1);
                    }
                    Err(e) => {
                        app.set_status_text(SharedString::from(format!("保存失败: {}", e)));
                        app.set_status_type(2);
                    }
                }
            },
        );
    }

    {
        let app_weak = app.as_weak();
        let app_config = app_config.clone();
        app.on_save_as_preset(
            move |preset_name, proxy_type, host, port, no_proxy, test_url| {
                let Some(app) = app_weak.upgrade() else {
                    return;
                };

                let cfg = config::PresetConfig {
                    name: preset_name.to_string(),
                    proxy_type: proxy_type.to_string(),
                    host: host.to_string(),
                    port: port.to_string(),
                    no_proxy: no_proxy.to_string(),
                    test_url: test_url.to_string(),
                };

                let mut app_config = app_config.lock().unwrap();
                app_config.upsert_preset(cfg);
                let preset = app_config.current().clone();
                sync_presets_to_app(&app, &app_config);
                apply_preset_to_app(&app, &preset);
                match config::save_config(&app_config) {
                    Ok(_) => {
                        app.set_status_text(SharedString::from(format!(
                            "已保存预设: {}",
                            preset.name
                        )));
                        app.set_status_type(1);
                    }
                    Err(e) => {
                        app.set_status_text(SharedString::from(format!("保存失败: {}", e)));
                        app.set_status_type(2);
                    }
                }
            },
        );
    }

    // ── 打开终端回调 ──────────────────────────────────────────
    {
        let app_weak = app.as_weak();
        app.on_open_terminal(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            match launcher::open_terminal() {
                Ok(_) => {
                    app.set_status_text(SharedString::from("已打开终端"));
                    app.set_status_type(1);
                }
                Err(e) => {
                    app.set_status_text(SharedString::from(e));
                    app.set_status_type(2);
                }
            }
        });
    }

    // ── 启动代理回调 ──────────────────────────────────────────
    {
        let app_weak = app.as_weak();
        let app_config = app_config.clone();

        app.on_launch_clicked(move |proxy_type_str, host, port, no_proxy| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };

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

            if let Err(e) = proxy::validate_host(host.as_ref()) {
                app.set_status_text(SharedString::from(e));
                app.set_status_type(2);
                return;
            }
            if let Err(e) = proxy::validate_no_proxy(no_proxy.as_ref()) {
                app.set_status_text(SharedString::from(e));
                app.set_status_type(2);
                return;
            }

            let config =
                proxy::ProxyConfig::new(proxy_type, host.as_ref(), port_num, no_proxy.as_ref());

            app.set_status_text(SharedString::from("正在设置代理..."));
            app.set_status_type(0);

            let app_weak2 = app.as_weak();
            let app_config = app_config.clone();
            std::thread::spawn(move || {
                let result = launcher::set_proxy_env(&config);

                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = app_weak2.upgrade() else {
                        return;
                    };

                    match result {
                        Ok(_) => {
                            save_running_state(&app_config, true);
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
        let app_config = app_config.clone();

        app.on_stop_clicked(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };

            app.set_status_text(SharedString::from("正在停止代理..."));
            app.set_status_type(0);

            let app_weak2 = app.as_weak();
            let app_config = app_config.clone();
            std::thread::spawn(move || {
                let result = launcher::unset_proxy_env();

                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = app_weak2.upgrade() else {
                        return;
                    };

                    match result {
                        Ok(_) => {
                            save_running_state(&app_config, false);
                            app.set_proxy_running(false);
                            app.set_status_text(SharedString::from(
                                "代理已停止，新终端将不再使用代理",
                            ));
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
            let Some(app) = app_weak.upgrade() else {
                return;
            };

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

            if let Err(e) = proxy::validate_host(host.as_ref()) {
                app.set_status_text(SharedString::from(e));
                app.set_status_type(2);
                return;
            }

            let config = proxy::ProxyConfig::new(proxy_type, host.as_ref(), port_num, "");

            app.set_status_text(SharedString::from("正在测试连接..."));
            app.set_status_type(0);

            let app_weak2 = app.as_weak();
            let test_url = test_url.to_string();
            std::thread::spawn(move || {
                let result = launcher::test_proxy_connection(&config, &test_url);

                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = app_weak2.upgrade() else {
                        return;
                    };

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

    // 必须使用 run_event_loop_until_quit，退出只能由托盘菜单或应用回调显式触发
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

fn build_tray_menu(
    proxy_running: bool,
    config: &config::AppConfig,
) -> Result<Menu, tray_icon::menu::Error> {
    let toggle_text = if proxy_running {
        "停止代理"
    } else {
        "启动代理"
    };
    let current = config.current();
    let status_text = format!(
        "当前: {}://{}:{}",
        current.proxy_type.to_lowercase(),
        current.host,
        current.port
    );
    let preset_items = config
        .presets
        .iter()
        .enumerate()
        .map(|(index, preset)| {
            let label = if index == config.current_preset {
                format!("✓ {}", preset.name)
            } else {
                preset.name.clone()
            };
            MenuItem::with_id(format!("preset:{}", index), label, true, None)
        })
        .collect::<Vec<_>>();
    let preset_refs = preset_items
        .iter()
        .map(|item| item as &dyn tray_icon::menu::IsMenuItem)
        .collect::<Vec<_>>();
    let presets_menu = Submenu::with_items("切换预设", true, &preset_refs)?;

    Menu::with_items(&[
        &MenuItem::with_id("show", "显示窗口", true, None),
        &MenuItem::with_id("current-proxy", status_text, false, None),
        &MenuItem::with_id("toggle-proxy", toggle_text, true, None),
        &MenuItem::with_id("test-proxy", "测试连接", true, None),
        &MenuItem::with_id("open-terminal", "打开终端", true, None),
        &presets_menu,
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id("exit", "退出", true, None),
    ])
}

fn sync_presets_to_app(app: &AppWindow, config: &config::AppConfig) {
    let names = config
        .presets
        .iter()
        .map(|preset| SharedString::from(preset.name.as_str()))
        .collect::<Vec<_>>();
    app.set_preset_names(ModelRc::new(VecModel::from(names)));
    app.set_preset_index(config.current_preset as i32);
}

fn apply_preset_to_app(app: &AppWindow, preset: &config::PresetConfig) {
    app.set_preset_name(SharedString::from(&preset.name));
    app.set_proxy_type_index(match preset.proxy_type.as_str() {
        "SOCKS5" => 1,
        _ => 0,
    });
    app.set_host(SharedString::from(&preset.host));
    app.set_port(SharedString::from(&preset.port));
    app.set_no_proxy(SharedString::from(&preset.no_proxy));
    app.set_test_url(SharedString::from(&preset.test_url));
}

fn save_running_state(config: &Arc<Mutex<config::AppConfig>>, running: bool) {
    let mut config = config.lock().unwrap();
    config.last_running = running;
    if let Err(e) = config::save_config(&config) {
        eprintln!("保存运行状态失败: {}", e);
    }
}

fn menu_signature(running: bool, config: &config::AppConfig) -> String {
    let current = config.current();
    let preset_names = config
        .presets
        .iter()
        .map(|preset| preset.name.as_str())
        .collect::<Vec<_>>()
        .join("|");
    format!(
        "{}:{}:{}:{}:{}:{}",
        running,
        config.current_preset,
        current.proxy_type,
        current.host,
        current.port,
        preset_names
    )
}
