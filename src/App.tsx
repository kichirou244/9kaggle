import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Plus,
  FolderOpen,
  Check,
  Pencil,
  Trash2,
  X,
  FileCode,
  Sparkles,
  User,
  AlertCircle,
  Database,
  RefreshCw,
  Download,
  RotateCcw,
  KeyRound,
  FileJson,
  Loader2,
  ShieldCheck,
} from "lucide-react";
import "./App.css";

type AccountType = "legacy" | "token";

interface QuotaInfo {
  gpu_used: number;
  gpu_total: number;
  gpu_reserved: number;
  tpu_used: number;
  tpu_total: number;
  tpu_reserved: number;
  refresh_at: string | null;
  checked_at: number;
}

interface Account {
  id: string;
  username: string | null;
  account_type: AccountType;
  content: string;
  quota?: QuotaInfo | null;
}

interface AppConfig {
  accounts: Account[];
  active_account_id: string | null;
}

interface Verification {
  username: string;
  quota: QuotaInfo | null;
  message: string | null;
}

interface CredentialFile {
  account_type: AccountType;
  content: string;
}

interface BackupFile {
  name: string;
  path: string;
  size: number;
  created_ms: number;
}

const fmtHours = (n: number) => n.toFixed(1);

/// Hiển thị mọi mốc thời gian theo giờ Việt Nam (UTC+7).
const formatVnTime = (value?: string | number | null): string => {
  if (value === null || value === undefined || value === "") return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  return date.toLocaleString("vi-VN", {
    timeZone: "Asia/Ho_Chi_Minh",
    hour12: false,
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
};

function QuotaLine({ quota }: { quota?: QuotaInfo | null }) {
  if (!quota) {
    return <span className="quota-empty">Chưa có dữ liệu quota</span>;
  }
  return (
    <span className="quota-line">
      <span className="quota-chip">
        GPU <b>{fmtHours(Math.max(quota.gpu_total - quota.gpu_used, 0))}</b>/{fmtHours(quota.gpu_total)}h
      </span>
      <span className="quota-chip">
        TPU <b>{fmtHours(Math.max(quota.tpu_total - quota.tpu_used, 0))}</b>/{fmtHours(quota.tpu_total)}h
      </span>
    </span>
  );
}

export default function App() {
  const [config, setConfig] = useState<AppConfig>({
    accounts: [],
    active_account_id: null,
  });
  const [modalOpen, setModalOpen] = useState(false);
  const [editingAccount, setEditingAccount] = useState<Account | null>(null);
  const [formType, setFormType] = useState<AccountType>("token");
  const [formContent, setFormContent] = useState("");
  const [formError, setFormError] = useState("");
  const [verifiedUsername, setVerifiedUsername] = useState<string | null>(null);
  const [verifying, setVerifying] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [backupOpen, setBackupOpen] = useState(false);
  const [backups, setBackups] = useState<BackupFile[]>([]);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showToast = (msg: string) => {
    setToast(msg);
    if (toastTimer.current) clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 3200);
  };

  const loadConfig = async () => {
    try {
      const res = await invoke<AppConfig>("get_config");
      setConfig(res);
    } catch (err) {
      console.error(err);
    }
  };

  useEffect(() => {
    loadConfig();

    const unlistenAccount = listen<AppConfig>("account_changed", (event) => {
      setConfig(event.payload);
      showToast("Đã cập nhật tài khoản");
    });
    const unlistenQuota = listen<AppConfig>("quota_changed", (event) => {
      setConfig(event.payload);
    });
    const unlistenToast = listen<string>("toast", (event) => {
      showToast(event.payload);
    });

    return () => {
      unlistenAccount.then((f) => f());
      unlistenQuota.then((f) => f());
      unlistenToast.then((f) => f());
      if (toastTimer.current) clearTimeout(toastTimer.current);
    };
  }, []);

  const handleActivate = async (id: string) => {
    try {
      const updated = await invoke<AppConfig>("activate_account", { id });
      setConfig(updated);
      const acc = updated.accounts.find((a) => a.id === id);
      showToast(`Đã kích hoạt: ${acc?.username ? `@${acc.username}` : "tài khoản"}`);
    } catch (err) {
      showToast(`Lỗi: ${err}`);
    }
  };

  const openAddModal = () => {
    setEditingAccount(null);
    setFormType("token");
    setFormContent("");
    setFormError("");
    setVerifiedUsername(null);
    setModalOpen(true);
  };

  const openEditModal = (acc: Account) => {
    setEditingAccount(acc);
    setFormType(acc.account_type);
    setFormContent(acc.content);
    setFormError("");
    setVerifiedUsername(acc.username);
    setModalOpen(true);
  };

  const switchType = (type: AccountType) => {
    if (type === formType) return;
    setFormType(type);
    setFormContent("");
    setFormError("");
    setVerifiedUsername(null);
  };

  const handleVerify = async () => {
    const content = formContent.trim();
    if (!content) {
      setFormError("Vui lòng nhập thông tin xác thực");
      return;
    }
    setVerifying(true);
    setFormError("");
    try {
      const res = await invoke<Verification>("verify_account", {
        accountType: formType,
        content,
      });
      setVerifiedUsername(res.username);
      if (res.message) showToast(res.message);
      else showToast(`Đã xác thực: @${res.username}`);
    } catch (err) {
      setFormError(String(err));
      setVerifiedUsername(null);
    } finally {
      setVerifying(false);
    }
  };

  const handleSave = async () => {
    const content = formContent.trim();
    if (!content) {
      setFormError(
        formType === "token"
          ? "Vui lòng dán API token KGAT_..."
          : "Vui lòng dán nội dung kaggle.json"
      );
      return;
    }
    setVerifying(true);
    setFormError("");
    try {
      const account: Account = {
        id: editingAccount?.id ?? "",
        username: null,
        account_type: formType,
        content,
      };
      const updated = await invoke<AppConfig>("save_account", { account });
      setConfig(updated);
      setModalOpen(false);
      showToast(editingAccount ? "Đã cập nhật tài khoản" : "Đã thêm tài khoản mới");
    } catch (err) {
      setFormError(String(err));
    } finally {
      setVerifying(false);
    }
  };

  const handleDelete = async (id: string) => {
    const acc = config.accounts.find((a) => a.id === id);
    if (!confirm(`Bạn có chắc muốn xoá tài khoản "${acc?.username ?? ""}"?`)) return;
    try {
      const updated = await invoke<AppConfig>("delete_account", { id });
      setConfig(updated);
      showToast("Đã xoá tài khoản");
    } catch (err) {
      showToast(`Lỗi: ${err}`);
    }
  };

  const handleLoadCurrent = async () => {
    try {
      const current = await invoke<CredentialFile | null>("read_current_kaggle_file");
      if (current?.content) {
        setFormType(current.account_type);
        setFormContent(current.content);
        setVerifiedUsername(null);
        showToast("Đã lấy credential hiện tại trên máy");
      } else {
        setFormError("Không tìm thấy ~/.kaggle/access_token hoặc ~/.kaggle/kaggle.json");
      }
    } catch (err) {
      setFormError(String(err));
    }
  };

  const handleFormatJson = () => {
    try {
      const parsed = JSON.parse(formContent);
      setFormContent(JSON.stringify(parsed, null, 2));
      setFormError("");
    } catch {
      setFormError("Nội dung không phải JSON hợp lệ để format");
    }
  };

  const handleRefreshQuota = async () => {
    setRefreshing(true);
    try {
      const updated = await invoke<AppConfig>("fetch_quota");
      setConfig(updated);
      showToast("Đã làm mới quota");
    } catch (err) {
      showToast(`Lỗi: ${err}`);
    } finally {
      setRefreshing(false);
    }
  };

  const handleScanAll = async () => {
    setScanning(true);
    try {
      const updated = await invoke<AppConfig>("check_all_quotas");
      setConfig(updated);
      const ok = updated.accounts.filter((a) => a.quota).length;
      showToast(`Đã quét quota: ${ok}/${updated.accounts.length} tài khoản`);
    } catch (err) {
      showToast(`Lỗi: ${err}`);
    } finally {
      setScanning(false);
    }
  };

  const loadBackups = async () => {
    try {
      setBackups(await invoke<BackupFile[]>("list_backup_files"));
    } catch (err) {
      showToast(`Lỗi: ${err}`);
    }
  };

  const openBackupModal = () => {
    loadBackups();
    setBackupOpen(true);
  };

  const handleBackup = async () => {
    try {
      const path = await invoke<string>("backup_data");
      await loadBackups();
      showToast(`Đã backup: ${path}`);
    } catch (err) {
      showToast(`Lỗi backup: ${err}`);
    }
  };

  const handleRestore = async (path: string, name: string) => {
    if (!confirm(`Khôi phục backup "${name}"? Các account hiện tại sẽ bị thay thế.`)) return;
    try {
      const updated = await invoke<AppConfig>("restore_backup_file", { path });
      setConfig(updated);
      setBackupOpen(false);
      showToast("Đã khôi phục backup");
    } catch (err) {
      showToast(`Lỗi khôi phục: ${err}`);
    }
  };

  const handleOpenFolder = () => invoke("open_kaggle_dir");
  const handleOpenBackupDir = () => invoke("open_backup_dir");

  const activeAccount = config.accounts.find((a) => a.id === config.active_account_id);

  return (
    <div className="app-container">
      {toast && <div className="toast-notification">{toast}</div>}

      {/* Header */}
      <header className="app-header">
        <div className="brand">
          <div className="brand-icon">K</div>
          <span className="brand-name">Kaggle Switcher</span>
        </div>
        <div className="header-actions">
          <button
            className="icon-btn"
            title="Làm mới quota (acc đang dùng)"
            onClick={handleRefreshQuota}
            disabled={refreshing}
          >
            <RefreshCw size={16} className={refreshing ? "spinning" : ""} />
          </button>
          <button
            className="icon-btn"
            title="Backup / khôi phục dữ liệu"
            onClick={openBackupModal}
          >
            <Database size={17} />
          </button>
          <button
            className="icon-btn"
            title="Mở thư mục .kaggle"
            onClick={handleOpenFolder}
          >
            <FolderOpen size={17} />
          </button>
          <button className="primary-btn" onClick={openAddModal}>
            <Plus size={16} />
            <span>Thêm acc</span>
          </button>
        </div>
      </header>

      {/* Active Status */}
      <div className="status-bar">
        {activeAccount ? (
          <>
            <div className="status-pill active">
              <span className="pulse-dot"></span>
              <span className="status-label">Đang dùng:</span>
              <strong className="status-name">@{activeAccount.username ?? "?"}</strong>
              <span className="type-tag">{activeAccount.account_type === "token" ? "API Token" : "kaggle.json"}</span>
            </div>
            <div className="quota-row">
              <QuotaLine quota={activeAccount.quota} />
              <div className="quota-actions">
                {activeAccount.quota && (
                  <span className="quota-refresh">
                    Đổi lúc: {formatVnTime(activeAccount.quota.refresh_at)}
                    {activeAccount.quota.checked_at
                      ? ` · Cập nhật: ${formatVnTime(activeAccount.quota.checked_at)}`
                      : ""}
                  </span>
                )}
                <button
                  className="tool-btn"
                  onClick={handleScanAll}
                  disabled={scanning}
                  title="Quét quota cho tất cả tài khoản trong config"
                >
                  {scanning ? <Loader2 size={13} className="spinning" /> : <RefreshCw size={13} />}
                  <span>{scanning ? "Đang quét..." : "Quét tất cả"}</span>
                </button>
              </div>
            </div>
          </>
        ) : (
          <div className="status-pill inactive">
            <span className="inactive-dot"></span>
            <span className="status-label">Chưa chọn tài khoản</span>
          </div>
        )}
      </div>

      {/* Accounts List */}
      <main className="accounts-list">
        {config.accounts.length === 0 ? (
          <div className="empty-state">
            <User size={36} className="empty-icon" />
            <p>Chưa có tài khoản nào</p>
            <button className="secondary-btn" onClick={openAddModal}>
              <Plus size={15} /> Thêm tài khoản đầu tiên
            </button>
          </div>
        ) : (
          config.accounts.map((acc) => {
            const isActive = acc.id === config.active_account_id;
            return (
              <div
                key={acc.id}
                className={`account-card ${isActive ? "card-active" : ""}`}
                onClick={() => handleActivate(acc.id)}
              >
                <div className="card-left">
                  <div className={`radio-indicator ${isActive ? "checked" : ""}`}>
                    {isActive && <Check size={13} />}
                  </div>
                  <div className="acc-info">
                    <span className="acc-name">@{acc.username ?? "chưa xác thực"}</span>
                    <div className="acc-meta">
                      <span className="type-tag small">
                        {acc.account_type === "token" ? "API Token" : "kaggle.json"}
                      </span>
                      <span className="acc-quota">
                        {acc.quota
                          ? `GPU ${fmtHours(Math.max(acc.quota.gpu_total - acc.quota.gpu_used, 0))}h · TPU ${fmtHours(Math.max(acc.quota.tpu_total - acc.quota.tpu_used, 0))}h`
                          : "Quota: chưa có dữ liệu"}
                      </span>
                    </div>
                  </div>
                </div>

                <div className="card-actions" onClick={(e) => e.stopPropagation()}>
                  <button
                    className="action-btn"
                    title="Chỉnh sửa"
                    onClick={() => openEditModal(acc)}
                  >
                    <Pencil size={15} />
                  </button>
                  <button
                    className="action-btn danger"
                    title="Xoá"
                    onClick={() => handleDelete(acc.id)}
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              </div>
            );
          })
        )}
      </main>

      {/* Modal Add/Edit */}
      {modalOpen && (
        <div className="modal-overlay" onClick={() => setModalOpen(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3>{editingAccount ? "Sửa tài khoản" : "Thêm tài khoản"}</h3>
              <button className="close-btn" onClick={() => setModalOpen(false)}>
                <X size={18} />
              </button>
            </div>

            <div className="modal-body">
              {formError && (
                <div className="error-banner">
                  <AlertCircle size={15} />
                  <span>{formError}</span>
                </div>
              )}

              <div className="form-group">
                <label>Loại xác thực Kaggle</label>
                <div className="type-options">
                  <button
                    type="button"
                    className={`type-option ${formType === "token" ? "selected" : ""}`}
                    onClick={() => switchType("token")}
                  >
                    <KeyRound size={18} />
                    <span className="type-option-title">API Tokens</span>
                    <span className="type-option-desc">Dán token KGAT_... (khuyến nghị)</span>
                  </button>
                  <button
                    type="button"
                    className={`type-option ${formType === "legacy" ? "selected" : ""}`}
                    onClick={() => switchType("legacy")}
                  >
                    <FileJson size={18} />
                    <span className="type-option-title">Legacy API Credentials</span>
                    <span className="type-option-desc">Dán toàn bộ kaggle.json</span>
                  </button>
                </div>
              </div>

              <div className="form-group">
                <div className="label-row">
                  <label>
                    {formType === "token" ? "API Token (KGAT_...)" : "Nội dung kaggle.json"}
                  </label>
                  <div className="tools-row">
                    <button
                      type="button"
                      className="tool-btn"
                      onClick={handleLoadCurrent}
                      title="Lấy credential hiện tại trên máy"
                    >
                      <FileCode size={13} />
                      <span>Lấy từ máy</span>
                    </button>
                    {formType === "legacy" && (
                      <button
                        type="button"
                        className="tool-btn"
                        onClick={handleFormatJson}
                        title="Căn chỉnh JSON đẹp"
                      >
                        <Sparkles size={13} />
                        <span>Format</span>
                      </button>
                    )}
                  </div>
                </div>
                {formType === "token" ? (
                  <input
                    type="text"
                    className="token-input"
                    placeholder="KGAT_0123456789abcdef..."
                    value={formContent}
                    onChange={(e) => {
                      setFormContent(e.target.value);
                      setVerifiedUsername(null);
                    }}
                    spellCheck={false}
                  />
                ) : (
                  <textarea
                    rows={7}
                    placeholder='{"username": "your_username", "key": "your_api_key"}'
                    value={formContent}
                    onChange={(e) => {
                      setFormContent(e.target.value);
                      setVerifiedUsername(null);
                    }}
                    spellCheck={false}
                  />
                )}
                <span className="field-hint">
                  {formType === "token"
                    ? "Sẽ được ghi vào ~/.kaggle/access_token (chmod 600)."
                    : "Sẽ được ghi vào ~/.kaggle/kaggle.json."}
                </span>
              </div>

              {verifiedUsername && (
                <div className="verified-banner">
                  <ShieldCheck size={16} />
                  <span>
                    Đã xác thực với Kaggle: <b>@{verifiedUsername}</b> — username sẽ được tự động gán.
                  </span>
                </div>
              )}

              <button
                type="button"
                className="secondary-btn verify-btn"
                onClick={handleVerify}
                disabled={verifying}
              >
                {verifying ? <Loader2 size={15} className="spinning" /> : <ShieldCheck size={15} />}
                Kiểm tra với Kaggle
              </button>
            </div>

            <div className="modal-footer">
              <button className="secondary-btn" onClick={() => setModalOpen(false)}>
                Huỷ
              </button>
              <button className="primary-btn" onClick={handleSave} disabled={verifying}>
                {verifying && <Loader2 size={15} className="spinning" />}
                Lưu
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Modal Backup */}
      {backupOpen && (
        <div className="modal-overlay" onClick={() => setBackupOpen(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3>Backup dữ liệu</h3>
              <button className="close-btn" onClick={() => setBackupOpen(false)}>
                <X size={18} />
              </button>
            </div>
            <div className="modal-body">
              <p className="backup-note">
                Backup chứa toàn bộ tài khoản và credential. File được lưu trong thư mục backups của ứng dụng.
              </p>
              <div className="backup-actions">
                <button className="primary-btn" onClick={handleBackup}>
                  <Download size={15} /> Tạo backup
                </button>
                <button className="secondary-btn" onClick={handleOpenBackupDir}>
                  <FolderOpen size={15} /> Mở thư mục
                </button>
              </div>
              <div className="backup-list">
                {backups.length === 0 ? (
                  <p className="backup-empty">Chưa có bản backup nào.</p>
                ) : (
                  backups.map((b) => (
                    <div key={b.path} className="backup-item">
                      <div className="backup-info">
                        <span className="backup-name">{b.name}</span>
                        <span className="backup-date">
                          {formatVnTime(b.created_ms)} · {(b.size / 1024).toFixed(1)} KB
                        </span>
                      </div>
                      <button
                        className="action-btn"
                        title="Khôi phục backup này"
                        onClick={() => handleRestore(b.path, b.name)}
                      >
                        <RotateCcw size={15} />
                      </button>
                    </div>
                  ))
                )}
              </div>
            </div>
            <div className="modal-footer">
              <button className="secondary-btn" onClick={() => setBackupOpen(false)}>
                Đóng
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
