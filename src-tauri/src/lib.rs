mod config;
mod kaggle_api;
mod tray;

use config::{
    create_backup, list_backups, load_config, restore_backup, save_config, write_credentials,
    Account, AccountType, AppConfig, BackupFile, QuotaInfo,
};
use kaggle_api::{fetch_quota_for_account, verify_account as api_verify, Verification};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tray::{build_tray_menu, update_tray};

struct AppState(Mutex<AppConfig>);

#[derive(Debug, Clone, Serialize)]
struct CredentialFile {
    account_type: String,
    content: String,
}

fn notify(app: &AppHandle, message: &str) {
    let _ = app.emit("toast", message.to_string());
}

/// Lấy quota của account đang dùng, lưu vào config và cập nhật taskbar.
fn refresh_quota_internal(app: &AppHandle) {
    let account = {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let Ok(conf) = state.0.lock() else {
            return;
        };
        let Some(acc) = conf.active_account() else {
            return;
        };
        acc.clone()
    };

    match fetch_quota_for_account(&account) {
        Ok(quota) => {
            if let Some(state) = app.try_state::<AppState>() {
                if let Ok(mut conf) = state.0.lock() {
                    if let Some(acc) = conf.accounts.iter_mut().find(|a| a.id == account.id) {
                        acc.quota = Some(quota);
                    }
                    let _ = save_config(&conf);
                    update_tray(app, &conf);
                    let _ = app.emit("quota_changed", conf.clone());
                }
            }
        }
        Err(e) => {
            eprintln!("Không lấy được quota: {}", e);
        }
    }
}

/// Quét quota cho TOÀN BỘ tài khoản đang có trong config (không chỉ acc active).
/// Chạy tuần tự để tránh bị Kaggle giới hạn request.
fn scan_all_quotas_internal(app: &AppHandle) -> usize {
    let accounts: Vec<Account> = {
        let Some(state) = app.try_state::<AppState>() else {
            return 0;
        };
        let Ok(conf) = state.0.lock() else {
            return 0;
        };
        conf.accounts.clone()
    };

    let mut results: Vec<(String, QuotaInfo)> = Vec::new();
    for acc in accounts {
        if let Ok(quota) = fetch_quota_for_account(&acc) {
            results.push((acc.id.clone(), quota));
        }
    }

    if results.is_empty() {
        return 0;
    }
    let updated_count = results.len();
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut conf) = state.0.lock() {
            for (id, quota) in results {
                if let Some(acc) = conf.accounts.iter_mut().find(|a| a.id == id) {
                    acc.quota = Some(quota);
                }
            }
            let _ = save_config(&conf);
            update_tray(app, &conf);
            let _ = app.emit("quota_changed", conf.clone());
        }
    }
    updated_count
}

fn activate_account_internal(app: &AppHandle, id: &str) -> Result<AppConfig, String> {
    let account = {
        let state = app.try_state::<AppState>().ok_or_else(|| "App chưa sẵn sàng".to_string())?;
        let conf = state.0.lock().map_err(|e| e.to_string())?;
        conf.find_account(id)
            .cloned()
            .ok_or_else(|| "Không tìm thấy tài khoản".to_string())?
    };

    write_credentials(&account)?;
    let quota = fetch_quota_for_account(&account).ok();

    let state = app.try_state::<AppState>().ok_or_else(|| "App chưa sẵn sàng".to_string())?;
    let mut conf = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(acc) = conf.accounts.iter_mut().find(|a| a.id == id) {
        acc.quota = quota;
    }
    conf.active_account_id = Some(id.to_string());
    save_config(&conf)?;
    update_tray(app, &conf);
    let _ = app.emit("account_changed", conf.clone());
    Ok(conf.clone())
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let conf = state.0.lock().map_err(|e| e.to_string())?;
    Ok(conf.clone())
}

/// Gọi Kaggle API để kiểm tra credential + lấy username (không cần người dùng điền).
#[tauri::command]
fn verify_account(account_type: String, content: String) -> Result<Verification, String> {
    api_verify(&AccountType::from_str(&account_type), &content)
}

#[tauri::command]
fn activate_account(
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppConfig, String> {
    let _ = state;
    activate_account_internal(&app, &id)
}

#[tauri::command]
fn save_account(
    mut account: Account,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppConfig, String> {
    account.content = account.content.trim().to_string();
    if account.content.is_empty() {
        return Err("Vui lòng nhập thông tin xác thực".to_string());
    }

    // Tự kiểm tra với Kaggle và gán username.
    let verified = api_verify(&account.account_type, &account.content);
    match verified {
        Ok(v) => {
            account.username = Some(v.username);
            account.quota = v.quota;
            if let Some(msg) = v.message {
                notify(&app, &msg);
            }
        }
        Err(e) => {
            // Với legacy, username có trong JSON nên vẫn cho lưu nếu Kaggle tạm không gọi được.
            if account.account_type == AccountType::Legacy {
                if let Some(u) = Account::extract_username(&account.content) {
                    account.username = Some(u);
                    notify(&app, &format!("Không kiểm tra được với Kaggle: {}", e));
                } else {
                    return Err(e);
                }
            } else {
                return Err(e);
            }
        }
    }

    let mut conf = state.0.lock().map_err(|e| e.to_string())?;
    if account.id.is_empty() {
        let new_acc = Account::new(
            account.username,
            account.account_type,
            account.content.clone(),
        );
        let mut new_acc = new_acc;
        new_acc.quota = account.quota.clone();
        let new_id = new_acc.id.clone();
        let is_first = conf.accounts.is_empty();
        conf.accounts.push(new_acc);
        if is_first {
            conf.active_account_id = Some(new_id.clone());
            if let Some(acc) = conf.find_account(&new_id).cloned() {
                let _ = write_credentials(&acc);
            }
        }
    } else {
        let was_active = conf.active_account_id.as_deref() == Some(&account.id);
        if let Some(existing) = conf.accounts.iter_mut().find(|a| a.id == account.id) {
            let changed_credentials = existing.content != account.content
                || existing.account_type != account.account_type;
            existing.username = account.username;
            existing.account_type = account.account_type.clone();
            existing.content = account.content;
            existing.quota = account.quota;
            if was_active && changed_credentials {
                let updated = existing.clone();
                let _ = write_credentials(&updated);
            }
        }
    }
    let snapshot = conf.clone();
    drop(conf);

    save_config(&snapshot)?;
    update_tray(&app, &snapshot);
    let _ = app.emit("account_changed", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
fn delete_account(
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppConfig, String> {
    let mut conf = state.0.lock().map_err(|e| e.to_string())?;
    conf.accounts.retain(|a| a.id != id);
    if conf.active_account_id.as_deref() == Some(&id) {
        conf.active_account_id = None;
    }
    save_config(&conf)?;
    update_tray(&app, &conf);
    let _ = app.emit("account_changed", conf.clone());
    Ok(conf.clone())
}

#[tauri::command]
fn read_current_kaggle_file() -> Result<Option<CredentialFile>, String> {
    Ok(config::read_current_credentials().map(|(account_type, content)| CredentialFile {
        account_type: account_type.as_str().to_string(),
        content,
    }))
}

#[tauri::command]
fn open_kaggle_dir() -> Result<(), String> {
    if let Some(dir) = config::get_kaggle_dir() {
        if !dir.exists() {
            let _ = std::fs::create_dir_all(&dir);
        }
        let _ = std::process::Command::new("explorer").arg(dir).spawn();
    }
    Ok(())
}

#[tauri::command]
fn hide_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    Ok(())
}

#[tauri::command]
fn fetch_quota(app: AppHandle) -> Result<AppConfig, String> {
    refresh_quota_internal(&app);
    let state = app.try_state::<AppState>().ok_or_else(|| "App chưa sẵn sàng".to_string())?;
    let conf = state.0.lock().map_err(|e| e.to_string())?;
    Ok(conf.clone())
}

#[tauri::command]
fn check_all_quotas(app: AppHandle) -> Result<AppConfig, String> {
    scan_all_quotas_internal(&app);
    let state = app.try_state::<AppState>().ok_or_else(|| "App chưa sẵn sàng".to_string())?;
    let conf = state.0.lock().map_err(|e| e.to_string())?;
    Ok(conf.clone())
}

#[tauri::command]
fn backup_data(app: AppHandle) -> Result<String, String> {
    let path = create_backup()?;
    notify(&app, "Đã backup dữ liệu");
    Ok(path)
}

#[tauri::command]
fn list_backup_files() -> Result<Vec<BackupFile>, String> {
    list_backups()
}

#[tauri::command]
fn restore_backup_file(path: String, app: AppHandle) -> Result<AppConfig, String> {
    let restored = restore_backup(&path)?;
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut conf) = state.0.lock() {
            *conf = restored.clone();
        }
    }
    if let Some(acc) = restored.active_account() {
        let _ = write_credentials(acc);
        if let Ok(quota) = fetch_quota_for_account(acc) {
            if let Some(state) = app.try_state::<AppState>() {
                if let Ok(mut conf) = state.0.lock() {
                    if let Some(a) = conf.accounts.iter_mut().find(|a| a.id == acc.id) {
                        a.quota = Some(quota);
                    }
                    let _ = save_config(&conf);
                }
            }
        }
    }
    let final_conf = app
        .try_state::<AppState>()
        .and_then(|s| s.0.lock().ok().map(|c| c.clone()))
        .unwrap_or(restored);
    update_tray(&app, &final_conf);
    let _ = app.emit("account_changed", final_conf.clone());
    notify(&app, "Đã khôi phục backup");
    Ok(final_conf)
}

#[tauri::command]
fn open_backup_dir() -> Result<(), String> {
    if let Some(dir) = config::get_backup_dir() {
        if !dir.exists() {
            let _ = std::fs::create_dir_all(&dir);
        }
        let _ = std::process::Command::new("explorer").arg(dir).spawn();
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let initial_config = load_config();
    let has_accounts = !initial_config.accounts.is_empty();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState(Mutex::new(initial_config.clone())))
        .invoke_handler(tauri::generate_handler![
            get_config,
            verify_account,
            activate_account,
            save_account,
            delete_account,
            read_current_kaggle_file,
            open_kaggle_dir,
            hide_window,
            fetch_quota,
            check_all_quotas,
            backup_data,
            list_backup_files,
            restore_backup_file,
            open_backup_dir,
        ])
        .setup(move |app| {
            let handle = app.handle();
            let menu = build_tray_menu(handle, &initial_config)?;

            let tooltip = match initial_config.active_account() {
                Some(acc) => format!("Kaggle: {}", acc.display_name()),
                None => "Kaggle Switcher".to_string(),
            };

            let mut tray_builder = TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .tooltip(tooltip)
                .show_menu_on_left_click(false);
            // Icon taskbar được nhúng riêng (32x32) để không bị trống trên Windows.
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
                .ok()
                .or_else(|| app.default_window_icon().cloned());
            if let Some(icon) = tray_icon {
                tray_builder = tray_builder.icon(icon);
            }

            let _tray = tray_builder
                .on_menu_event(|app, event| {
                    let id_str = event.id().as_ref();
                    match id_str {
                        "exit" => app.exit(0),
                        "settings" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                        "open_folder" => {
                            let _ = open_kaggle_dir();
                        }
                        "refresh_quota" => {
                            refresh_quota_internal(app);
                            notify(app, "Đã làm mới quota");
                        }
                        "scan_all_quotas" => {
                            let count = scan_all_quotas_internal(app);
                            notify(app, &format!("Đã quét quota {} tài khoản", count));
                        }
                        "backup" => {
                            match create_backup() {
                                Ok(_) => notify(app, "Đã backup dữ liệu"),
                                Err(e) => notify(app, &format!("Lỗi backup: {}", e)),
                            }
                        }
                        _ => {
                            if let Some(acc_id) = id_str.strip_prefix("acc_") {
                                match activate_account_internal(app, acc_id) {
                                    Ok(conf) => {
                                        let name = conf
                                            .find_account(acc_id)
                                            .map(|a| a.display_name())
                                            .unwrap_or_else(|| acc_id.to_string());
                                        notify(app, &format!("Đã kích hoạt: {}", name));
                                    }
                                    Err(e) => notify(app, &format!("Lỗi: {}", e)),
                                }
                            }
                        }
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                        | TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        } => {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            // Khi khởi động, quét quota cho mọi tài khoản đã có trong config.
            // App mới cài (chưa có account) sẽ không có dữ liệu nào được tạo ra.
            if !initial_config.accounts.is_empty() {
                let startup_app = app.handle().clone();
                std::thread::spawn(move || {
                    scan_all_quotas_internal(&startup_app);
                });
            }

            // Nếu chưa có account thì hiện cửa sổ để người dùng setup.
            if !has_accounts {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
