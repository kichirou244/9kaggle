use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Kaggle hỗ trợ 2 cách xác thực:
/// - Legacy API Credentials -> file ~/.kaggle/kaggle.json
/// - API Token (KGAT_...)   -> file ~/.kaggle/access_token
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    #[default]
    Legacy,
    Token,
}

impl AccountType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountType::Legacy => "legacy",
            AccountType::Token => "token",
        }
    }

    pub fn from_str(value: &str) -> Self {
        if value == "token" {
            AccountType::Token
        } else {
            AccountType::Legacy
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AccountType::Legacy => "kaggle.json",
            AccountType::Token => "API Token",
        }
    }
}

/// Quota GPU/TPU hàng tuần của Kaggle (đơn vị: giờ).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QuotaInfo {
    #[serde(default)]
    pub gpu_used: f64,
    #[serde(default)]
    pub gpu_total: f64,
    #[serde(default)]
    pub gpu_reserved: f64,
    #[serde(default)]
    pub tpu_used: f64,
    #[serde(default)]
    pub tpu_total: f64,
    #[serde(default)]
    pub tpu_reserved: f64,
    #[serde(default)]
    pub refresh_at: Option<String>,
    /// Thời điểm kiểm tra (unix millis) để frontend hiển thị.
    #[serde(default)]
    pub checked_at: u64,
}

impl QuotaInfo {
    pub fn gpu_remaining(&self) -> f64 {
        (self.gpu_total - self.gpu_used).max(0.0)
    }

    pub fn tpu_remaining(&self) -> f64 {
        (self.tpu_total - self.tpu_used).max(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub id: String,
    /// Username được tự động lấy từ Kaggle, dùng làm tên hiển thị.
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub account_type: AccountType,
    /// Nội dung kaggle.json (legacy) hoặc token KGAT_... (token).
    pub content: String,
    #[serde(default)]
    pub quota: Option<QuotaInfo>,
}

impl Account {
    pub fn new(username: Option<String>, account_type: AccountType, content: String) -> Self {
        let id = format!(
            "{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        Self {
            id,
            username,
            account_type,
            content: content.trim().to_string(),
            quota: None,
        }
    }

    pub fn display_name(&self) -> String {
        match &self.username {
            Some(u) if !u.is_empty() => format!("@{}", u),
            _ => "(chưa xác thực)".to_string(),
        }
    }

    pub fn extract_username(content: &str) -> Option<String> {
        let val = serde_json::from_str::<serde_json::Value>(content).ok()?;
        val.get("username")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn refresh_username(&mut self) {
        if self.account_type == AccountType::Legacy {
            self.username = Self::extract_username(&self.content);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub accounts: Vec<Account>,
    pub active_account_id: Option<String>,
}

impl AppConfig {
    pub fn find_account(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn active_account(&self) -> Option<&Account> {
        self.active_account_id
            .as_deref()
            .and_then(|id| self.find_account(id))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub created_ms: u64,
}

pub fn get_kaggle_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".kaggle"))
}

pub fn get_kaggle_json_path() -> Option<PathBuf> {
    get_kaggle_dir().map(|d| d.join("kaggle.json"))
}

pub fn get_access_token_path() -> Option<PathBuf> {
    get_kaggle_dir().map(|d| d.join("access_token"))
}

pub fn get_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|c| c.join("kaggle-switcher").join("config.json"))
}

pub fn get_backup_dir() -> Option<PathBuf> {
    get_config_path().and_then(|p| p.parent().map(|parent| parent.join("backups")))
}

/// Đọc credential đang được dùng trên máy theo đúng thứ tự ưu tiên của Kaggle:
/// access_token trước, kaggle.json sau.
pub fn read_current_credentials() -> Option<(AccountType, String)> {
    if let Some(path) = get_access_token_path() {
        if let Ok(content) = fs::read_to_string(&path) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Some((AccountType::Token, trimmed.to_string()));
            }
        }
    }
    if let Some(path) = get_kaggle_json_path() {
        if let Ok(content) = fs::read_to_string(&path) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Some((AccountType::Legacy, trimmed.to_string()));
            }
        }
    }
    None
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

/// Ghi credential của account được chọn vào ~/.kaggle.
/// Khi chuyển loại credential thì xoá loại cũ để Kaggle không ưu tiên nhầm.
pub fn write_credentials(account: &Account) -> Result<(), String> {
    let dir = get_kaggle_dir().ok_or_else(|| "Không tìm thấy thư mục người dùng".to_string())?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("Lỗi tạo thư mục .kaggle: {}", e))?;
    }

    let json_path = dir.join("kaggle.json");
    let token_path = dir.join("access_token");
    let token_txt_path = dir.join("access_token.txt");
    let content = account.content.trim();

    match account.account_type {
        AccountType::Token => {
            fs::write(&token_path, content).map_err(|e| format!("Lỗi ghi access_token: {}", e))?;
            restrict_permissions(&token_path);
            let _ = fs::remove_file(&token_txt_path);
            let _ = fs::remove_file(&json_path);
        }
        AccountType::Legacy => {
            fs::write(&json_path, content).map_err(|e| format!("Lỗi ghi kaggle.json: {}", e))?;
            restrict_permissions(&json_path);
            let _ = fs::remove_file(&token_path);
            let _ = fs::remove_file(&token_txt_path);
        }
    }
    Ok(())
}

pub fn load_config() -> AppConfig {
    let mut config = if let Some(path) = get_config_path() {
        if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .and_then(|data| serde_json::from_str::<AppConfig>(&data).ok())
                .unwrap_or_default()
        } else {
            AppConfig::default()
        }
    } else {
        AppConfig::default()
    };

    for acc in &mut config.accounts {
        acc.content = acc.content.trim().to_string();
        if acc.username.is_none() {
            acc.refresh_username();
        }
    }

    // Chỉ giữ active id nếu account tương ứng vẫn tồn tại.
    if let Some(id) = config.active_account_id.clone() {
        if config.find_account(&id).is_none() {
            config.active_account_id = None;
        }
    }

    // Tự nhận diện account đang được cấu hình trên máy (không tạo account "Mặc định").
    if let Some((kind, current)) = read_current_credentials() {
        if let Some(acc) = config
            .accounts
            .iter()
            .find(|a| a.account_type == kind && a.content.trim() == current)
        {
            config.active_account_id = Some(acc.id.clone());
        }
    }

    config
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path().ok_or_else(|| "Không tìm thấy thư mục AppData".to_string())?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("Lỗi tạo thư mục cấu hình: {}", e))?;
        }
    }
    let data = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Lỗi tuần tự hoá JSON: {}", e))?;
    fs::write(path, data).map_err(|e| format!("Lỗi lưu cấu hình: {}", e))?;
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Tạo bản sao config.json (chứa toàn bộ account/credential) vào thư mục backups.
pub fn create_backup() -> Result<String, String> {
    let config_path = get_config_path().ok_or_else(|| "Không tìm thấy thư mục AppData".to_string())?;
    if !config_path.exists() {
        // Lưu cấu hình hiện tại trước khi backup nếu chưa có file.
        save_config(&load_config())?;
    }
    let backup_dir = get_backup_dir().ok_or_else(|| "Không tìm thấy thư mục backup".to_string())?;
    fs::create_dir_all(&backup_dir).map_err(|e| format!("Lỗi tạo thư mục backup: {}", e))?;

    let file_name = format!("config_{}.json", now_ms());
    let target = backup_dir.join(&file_name);
    fs::copy(&config_path, &target).map_err(|e| format!("Lỗi sao lưu: {}", e))?;
    Ok(target.to_string_lossy().to_string())
}

pub fn list_backups() -> Result<Vec<BackupFile>, String> {
    let backup_dir = get_backup_dir().ok_or_else(|| "Không tìm thấy thư mục backup".to_string())?;
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(&backup_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let created_ms = meta
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or_else(|| {
                name.strip_prefix("config_")
                    .and_then(|s| s.strip_suffix(".json"))
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
            });
        items.push(BackupFile {
            name,
            path: path.to_string_lossy().to_string(),
            size: meta.len(),
            created_ms,
        });
    }
    items.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
    Ok(items)
}

pub fn restore_backup(path: &str) -> Result<AppConfig, String> {
    let backup_dir = get_backup_dir().ok_or_else(|| "Không tìm thấy thư mục backup".to_string())?;
    let target = Path::new(path);
    let canonical_dir = fs::canonicalize(&backup_dir).map_err(|e| e.to_string())?;
    let canonical_target = fs::canonicalize(target).map_err(|e| format!("Không tìm thấy file backup: {}", e))?;
    if !canonical_target.starts_with(&canonical_dir) {
        return Err("Đường dẫn backup không hợp lệ".to_string());
    }
    let data = fs::read_to_string(&canonical_target).map_err(|e| e.to_string())?;
    let restored: AppConfig =
        serde_json::from_str(&data).map_err(|e| format!("File backup không hợp lệ: {}", e))?;
    save_config(&restored)?;
    Ok(load_config())
}
