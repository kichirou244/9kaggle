//! Gọi trực tiếp Kaggle API v1 để xác thực credential và lấy quota.
//! Tham khảo:
//! - https://github.com/Kaggle/kaggle-api/blob/main/docs/README.md#authentication
//! - https://github.com/Kaggle/kaggle-sdk-python/blob/main/kagglesdk/kaggle_http_client.py

use crate::config::{Account, AccountType, QuotaInfo};
use serde::Serialize;
use serde_json::{json, Value};

const API_BASE: &str = "https://api.kaggle.com/v1";
const INTROSPECT_ENDPOINT: &str = "security.OAuthService/IntrospectToken";
const QUOTA_ENDPOINT: &str = "kernels.KernelsApiService/GetAcceleratorQuotaStatistics";

#[derive(Debug, Clone, Serialize)]
pub struct Verification {
    pub username: String,
    pub quota: Option<QuotaInfo>,
    /// Thông báo phụ (ví dụ không lấy được quota nhưng credential vẫn đúng).
    pub message: Option<String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn field<'a>(value: &'a Value, camel: &str, snake: &str) -> Option<&'a Value> {
    value.get(camel).or_else(|| value.get(snake))
}

/// Kaggle trả duration dạng protobuf JSON: "18000s", đôi khi là số giây.
fn duration_hours(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::String(s)) => {
            let s = s.trim();
            if let Some(raw) = s.strip_suffix('s') {
                raw.parse::<f64>().unwrap_or(0.0) / 3600.0
            } else if let Some(raw) = s.strip_suffix('h') {
                raw.parse::<f64>().unwrap_or(0.0)
            } else {
                let parts: Vec<&str> = s.split(':').collect();
                if parts.len() == 3 {
                    let h = parts[0].parse::<f64>().unwrap_or(0.0);
                    let m = parts[1].parse::<f64>().unwrap_or(0.0) / 60.0;
                    let sec = parts[2].parse::<f64>().unwrap_or(0.0) / 3600.0;
                    h + m + sec
                } else {
                    s.parse::<f64>().unwrap_or(0.0) / 3600.0
                }
            }
        }
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) / 3600.0,
        Some(Value::Object(_)) => {
            // Dự phòng nếu server trả {seconds, nanos}.
            let seconds = field(value.unwrap(), "seconds", "seconds")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            seconds / 3600.0
        }
        _ => 0.0,
    }
}

fn parse_accelerator(value: Option<&Value>) -> (f64, f64, f64) {
    let Some(quota) = value else {
        return (0.0, 0.0, 0.0);
    };
    (
        duration_hours(field(quota, "timeUsed", "time_used")),
        duration_hours(field(quota, "timeReserved", "time_reserved")),
        duration_hours(field(quota, "totalTimeAllowed", "total_time_allowed")),
    )
}

fn parse_quota(value: &Value) -> Option<QuotaInfo> {
    let (gpu_used, gpu_reserved, gpu_total) =
        parse_accelerator(field(value, "gpuQuota", "gpu_quota"));
    let (tpu_used, tpu_reserved, tpu_total) =
        parse_accelerator(field(value, "tpuQuota", "tpu_quota"));

    if gpu_total <= 0.0 && tpu_total <= 0.0 {
        return None;
    }

    let refresh_at = field(value, "quotaRefreshTime", "quota_refresh_time")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    Some(QuotaInfo {
        gpu_used,
        gpu_total,
        gpu_reserved,
        tpu_used,
        tpu_total,
        tpu_reserved,
        refresh_at,
        checked_at: now_ms(),
    })
}

fn build_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Lỗi khởi tạo HTTP client: {}", e))
}

fn post(endpoint: &str, body: Value, account_type: &AccountType, content: &str) -> Result<Value, String> {
    let client = build_client()?;
    let mut request = client
        .post(format!("{}/{}", API_BASE, endpoint))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", "kaggle-account-switcher/0.1");

    match account_type {
        AccountType::Token => {
            request = request.bearer_auth(content.trim());
        }
        AccountType::Legacy => {
            let parsed: Value = serde_json::from_str(content.trim())
                .map_err(|_| "Nội dung kaggle.json không phải JSON hợp lệ".to_string())?;
            let username = field(&parsed, "username", "username")
                .and_then(Value::as_str)
                .ok_or_else(|| "kaggle.json thiếu trường username".to_string())?;
            let key = field(&parsed, "key", "key")
                .and_then(Value::as_str)
                .ok_or_else(|| "kaggle.json thiếu trường key".to_string())?;
            request = request.basic_auth(username, Some(key));
        }
    }

    let response = request
        .json(&body)
        .send()
        .map_err(|e| format!("Không kết nối được Kaggle: {}", e))?;

    let status = response.status();
    let response_text = response
        .text()
        .map_err(|e| format!("Không đọc được phản hồi Kaggle: {}", e))?;

    let parsed: Result<Value, _> = serde_json::from_str(&response_text);

    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err("Credential không hợp lệ hoặc đã bị thu hồi (401/403)".to_string());
    }
    if !status.is_success() {
        // Kaggle trả lỗi dạng {"error": {"code": 400, "message": "...", "status": "..."}}.
        let message = parsed
            .as_ref()
            .ok()
            .map(|v| {
                let err = field(v, "error", "error").unwrap_or(v);
                field(err, "message", "message")
                    .and_then(Value::as_str)
                    .unwrap_or(&response_text)
                    .to_string()
            })
            .unwrap_or_else(|| response_text.clone());
        return Err(format!("Kaggle trả lỗi {}: {}", status.as_u16(), message));
    }

    let value = parsed.map_err(|_| format!("Phản hồi Kaggle không phải JSON: {}", response_text))?;
    if let Some(error_obj) = field(&value, "error", "error") {
        if let Some(code) = field(error_obj, "code", "code").and_then(Value::as_i64) {
            if code >= 400 {
                let message = field(error_obj, "message", "message")
                    .and_then(Value::as_str)
                    .unwrap_or("Lỗi không xác định");
                return Err(message.to_string());
            }
        }
    }
    if let Some(code) = field(&value, "code", "code").and_then(Value::as_i64) {
        if code >= 400 {
            let message = field(&value, "message", "message")
                .and_then(Value::as_str)
                .unwrap_or("Lỗi không xác định");
            return Err(message.to_string());
        }
    }
    Ok(value)
}

/// Kiểm tra credential với Kaggle và tự động lấy username.
pub fn verify_account(account_type: &AccountType, content: &str) -> Result<Verification, String> {
    let content = content.trim();
    if content.is_empty() {
        return Err("Vui lòng nhập thông tin xác thực".to_string());
    }

    let username = match account_type {
        AccountType::Token => {
            if !content.starts_with("KGAT_") {
                return Err("API Token phải bắt đầu bằng KGAT_...".to_string());
            }
            if content.len() < 20 {
                return Err("API Token KGAT_... không hợp lệ (quá ngắn)".to_string());
            }
            let response = post(
                INTROSPECT_ENDPOINT,
                json!({ "token": content }),
                account_type,
                content,
            )?;
            let active = field(&response, "active", "active")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let user = field(&response, "username", "username")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if !active || user.is_empty() {
                return Err("Token không active hoặc Kaggle không trả username".to_string());
            }
            user
        }
        AccountType::Legacy => {
            let parsed: Value = serde_json::from_str(content)
                .map_err(|_| "Nội dung kaggle.json không phải JSON hợp lệ".to_string())?;
            let user = field(&parsed, "username", "username")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let key = field(&parsed, "key", "key")
                .and_then(Value::as_str)
                .unwrap_or("");
            if user.is_empty() || key.is_empty() {
                return Err("kaggle.json cần có cả username và key".to_string());
            }
            user
        }
    };

    // Lấy luôn quota trong lần kiểm tra. Lỗi xác thực thì báo lỗi, lỗi mạng thì vẫn giữ username.
    let quota_result = fetch_quota(account_type, content);
    let (quota, message) = match quota_result {
        Ok(q) => (Some(q), None),
        Err(e) if e.contains("401/403") || e.contains("không hợp lệ") => {
            return Err(e);
        }
        Err(e) => (None, Some(format!("Đã xác thực username nhưng không lấy được quota: {}", e))),
    };

    Ok(Verification {
        username,
        quota,
        message,
    })
}

pub fn fetch_quota(account_type: &AccountType, content: &str) -> Result<QuotaInfo, String> {
    let response = post(QUOTA_ENDPOINT, json!({}), account_type, content)?;
    parse_quota(&response).ok_or_else(|| "Kaggle không trả thông tin quota".to_string())
}

pub fn fetch_quota_for_account(account: &Account) -> Result<QuotaInfo, String> {
    fetch_quota(&account.account_type, &account.content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kaggle_quota_response() {
        let response = json!({
            "gpuQuota": {
                "timeUsed": "18000s",
                "timeReserved": "3600s",
                "totalTimeAllowed": "108000s"
            },
            "tpuQuota": {
                "timeUsed": "7200s",
                "totalTimeAllowed": "72000s"
            },
            "quotaRefreshTime": "2026-11-30T00:00:00Z"
        });
        let quota = parse_quota(&response).expect("phải parse được quota");
        assert_eq!(quota.gpu_used, 5.0);
        assert_eq!(quota.gpu_reserved, 1.0);
        assert_eq!(quota.gpu_total, 30.0);
        assert_eq!(quota.gpu_remaining(), 25.0);
        assert_eq!(quota.tpu_used, 2.0);
        assert_eq!(quota.tpu_total, 20.0);
        assert_eq!(quota.refresh_at.as_deref(), Some("2026-11-30T00:00:00Z"));
    }

    #[test]
    fn parses_error_envelope() {
        let response = json!({
            "error": {"code": 400, "message": "Token too small", "status": "INVALID_ARGUMENT"}
        });
        let message = field(&response, "error", "error")
            .and_then(|e| field(e, "message", "message"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert_eq!(message, "Token too small");
    }
}
