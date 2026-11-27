use crate::config::{Account, AppConfig, QuotaInfo};
use tauri::{
    menu::{CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Runtime,
};

fn quota_line(quota: Option<&QuotaInfo>) -> String {
    match quota {
        Some(q) => format!(
            "Quota: GPU {:.1}/{:.1}h | TPU {:.1}/{:.1}h",
            q.gpu_remaining(),
            q.gpu_total,
            q.tpu_remaining(),
            q.tpu_total
        ),
        None => "Quota: chưa có dữ liệu".to_string(),
    }
}

fn account_label(acc: &Account) -> String {
    let name = acc.display_name();
    format!("{} [{}]", name, acc.account_type.label())
}

pub fn build_tray_menu<R: Runtime>(app: &AppHandle<R>, config: &AppConfig) -> Result<Menu<R>, tauri::Error> {
    let mut switch_sub = SubmenuBuilder::new(app, "Đổi acc");
    if config.accounts.is_empty() {
        let no_acc = MenuItemBuilder::with_id("no_acc", "(Chưa có tài khoản)")
            .enabled(false)
            .build(app)?;
        switch_sub = switch_sub.item(&no_acc);
    } else {
        for acc in &config.accounts {
            let is_active = config.active_account_id.as_deref() == Some(&acc.id);
            let item = CheckMenuItemBuilder::with_id(format!("acc_{}", acc.id), account_label(acc))
                .checked(is_active)
                .build(app)?;
            switch_sub = switch_sub.item(&item);
        }
    }
    let sub = switch_sub.build()?;

    let title_status = match config.active_account() {
        Some(acc) => format!("Đang dùng: {}", acc.display_name()),
        None => "Chưa chọn tài khoản".to_string(),
    };
    let title_item = MenuItemBuilder::with_id("title", title_status)
        .enabled(false)
        .build(app)?;

    let quota_status = match config.active_account() {
        Some(acc) => quota_line(acc.quota.as_ref()),
        None => "Quota: chưa chọn tài khoản".to_string(),
    };
    let quota_item = MenuItemBuilder::with_id("quota_status", quota_status)
        .enabled(false)
        .build(app)?;

    MenuBuilder::new(app)
        .item(&title_item)
        .item(&quota_item)
        .separator()
        .item(&sub)
        .separator()
        .text("settings", "Cài đặt")
        .text("refresh_quota", "Làm mới quota (acc đang dùng)")
        .text("scan_all_quotas", "Quét quota tất cả acc")
        .text("backup", "Backup dữ liệu")
        .text("open_folder", "Mở thư mục .kaggle")
        .separator()
        .text("exit", "Thoát")
        .build()
}

pub fn update_tray<R: Runtime>(app: &AppHandle<R>, config: &AppConfig) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        if let Ok(menu) = build_tray_menu(app, config) {
            let _ = tray.set_menu(Some(menu));
        }
        let tooltip = match config.active_account() {
            Some(acc) => format!("Kaggle: {}", acc.display_name()),
            None => "Kaggle Switcher".to_string(),
        };
        let _ = tray.set_tooltip(Some(tooltip));
    }
}
