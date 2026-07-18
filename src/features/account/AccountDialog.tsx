import { KeyRound, Loader2, LogOut, QrCode, ScanLine, UserRound, X } from "lucide-react";
import { localFileSrc } from "../../lib/api";
import type { AccountInfo, LoginQrEvent } from "../../types";

export type LoginMethod = "scan" | "cookie";

interface AccountDialogProps {
  account: AccountInfo;
  busy: boolean;
  cookie: string;
  loginQr: LoginQrEvent | null;
  method: LoginMethod;
  onClose: () => void;
  onCookieChange: (value: string) => void;
  onMethodChange: (method: LoginMethod) => void;
  onLogout: () => void;
  onScanLogin: () => void;
  onVerifyCookie: () => void;
}

export function AccountDialog({
  account,
  busy,
  cookie,
  loginQr,
  method,
  onClose,
  onCookieChange,
  onMethodChange,
  onLogout,
  onScanLogin,
  onVerifyCookie,
}: AccountDialogProps) {
  const qrSrc = loginQr
    ? `${localFileSrc(loginQr.imagePath)}?v=${encodeURIComponent(loginQr.timestamp)}`
    : undefined;

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="account-dialog">
        <header>
          <div>
            <span className={account.isLoggedIn ? "account-state online" : "account-state"}>
              {account.isLoggedIn ? "已登录" : "未登录"}
            </span>
            <h3>哔哩哔哩账号</h3>
          </div>
          <button className="icon-action" onClick={onClose} title="关闭" type="button"><X size={18} /></button>
        </header>

        {account.isLoggedIn ? (
          <>
            <div className="account-profile">
              <div className="account-avatar large">
                {account.avatarUrl ? <img alt="账号头像" referrerPolicy="no-referrer" src={account.avatarUrl} /> : <UserRound size={24} />}
              </div>
              <div>
                <strong>{account.name}</strong>
                <span>UID {account.mid}</span>
              </div>
              {account.vipLabel ? <span className="vip-label">{account.vipLabel}</span> : null}
            </div>
            <button className="secondary-button danger full" disabled={busy} onClick={onLogout} type="button">
              {busy ? <Loader2 className="spin" size={17} /> : <LogOut size={17} />}
              <span>退出登录</span>
            </button>
          </>
        ) : null}

        <div className="segmented-control two login-methods">
          <button className={method === "scan" ? "selected" : ""} onClick={() => onMethodChange("scan")} type="button">
            <QrCode size={16} />扫码登录
          </button>
          <button className={method === "cookie" ? "selected" : ""} onClick={() => onMethodChange("cookie")} type="button">
            <KeyRound size={16} />Cookie
          </button>
        </div>

        {method === "scan" ? (
          <div className="scan-login">
            <div className="qr-box">
              {qrSrc ? <img alt="登录二维码" src={qrSrc} /> : <ScanLine size={38} />}
            </div>
            <button className="primary-button full" disabled={busy} onClick={onScanLogin} type="button">
              {busy ? <Loader2 className="spin" size={17} /> : <QrCode size={17} />}
              <span>{qrSrc ? "等待扫码" : "生成二维码"}</span>
            </button>
          </div>
        ) : (
          <div className="cookie-login">
            <label>
              <span>Cookie</span>
              <textarea value={cookie} onChange={(event) => onCookieChange(event.target.value)} />
            </label>
            <button className="primary-button full" disabled={busy || !cookie.trim()} onClick={onVerifyCookie} type="button">
              {busy ? <Loader2 className="spin" size={17} /> : <KeyRound size={17} />}
              <span>保存并验证</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
