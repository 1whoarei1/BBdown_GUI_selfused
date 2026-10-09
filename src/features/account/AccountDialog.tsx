import { KeyRound, Loader2, LogOut, QrCode, ScanLine, UserRound, X } from "lucide-react";
import { localFileSrc } from "../../lib/api";
import type { AccountInfo, ApiMode, LoginQrEvent } from "../../types";

export type LoginMethod = "scan" | "tv" | "cookie" | "token";

interface AccountDialogProps {
  account: AccountInfo;
  apiMode: ApiMode;
  accessToken: string;
  onApiModeChange: (mode: ApiMode) => void;
  onTokenChange: (value: string) => void;
  onSaveToken: () => void;
  onCancelLogin: () => void;
  busy: boolean;
  cookie: string;
  loginQr: LoginQrEvent | null;
  method: LoginMethod;
  onClose: () => void;
  onCookieChange: (value: string) => void;
  onMethodChange: (method: LoginMethod) => void;
  onLogout: () => void;
  onScanLogin: (mode: "web" | "tv") => void;
  onVerifyCookie: () => void;
}

export function AccountDialog({
  account,
  apiMode,
  accessToken,
  onApiModeChange,
  onTokenChange,
  onSaveToken,
  onCancelLogin,
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
  const qrSrc = loginQr && ((method === "tv" && loginQr.mode === "tv") || (method === "scan" && loginQr.mode === "web"))
    ? `${localFileSrc(loginQr.imagePath)}?v=${encodeURIComponent(loginQr.timestamp)}`
    : undefined;

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="account-dialog">
        <header>
          <div>
            <span className={account.isLoggedIn ? "account-state online" : "account-state"}>
              {account.isLoggedIn ? "WEB 已登录" : account.tokenConfigured ? "Token 已配置" : "未登录"}
            </span>
            <h3>哔哩哔哩账号</h3>
          </div>
          <button className="icon-action" onClick={onClose} title="关闭" type="button"><X size={18} /></button>
        </header>

        {account.isLoggedIn || account.tokenConfigured ? (
          <>
            <div className="account-profile">
              <div className="account-avatar large">
                {account.avatarUrl ? <img alt="账号头像" referrerPolicy="no-referrer" src={account.avatarUrl} /> : <UserRound size={24} />}
              </div>
              <div>
                <strong>{account.name ?? `${apiMode} Token`}</strong>
                <span>{account.mid ? `WEB UID ${account.mid}` : "播放时验证权限"}</span>
              </div>
              {account.vipLabel ? <span className="vip-label">{account.vipLabel}</span> : null}
            </div>
            <button className="secondary-button danger full" disabled={busy} onClick={onLogout} type="button">
              {busy ? <Loader2 className="spin" size={17} /> : <LogOut size={17} />}
              <span>退出登录</span>
            </button>
          </>
        ) : null}

        {account.tokenConfigured ? <p>所选接口的 Token 已配置，播放时校验权限。INTL 需要国际版凭据。</p> : null}
        <div className="segmented-control two login-methods">
          <button className={method === "scan" ? "selected" : ""} onClick={() => onMethodChange("scan")} type="button">
            <QrCode size={16} />WEB 扫码
          </button>
          <button className={method === "tv" ? "selected" : ""} onClick={() => onMethodChange("tv")} type="button">
            <QrCode size={16} />TV / APP 扫码
          </button>
          <button className={method === "token" ? "selected" : ""} onClick={() => onMethodChange("token")} type="button">
            <KeyRound size={16} />Token
          </button>
          <button className={method === "cookie" ? "selected" : ""} onClick={() => onMethodChange("cookie")} type="button">
            <KeyRound size={16} />Cookie
          </button>
        </div>

        {method === "scan" || method === "tv" ? (
          <div className="scan-login">
            <div className="qr-box">
              {qrSrc ? <img alt="登录二维码" src={qrSrc} /> : <ScanLine size={38} />}
            </div>
            <button className="primary-button full" disabled={busy} onClick={() => onScanLogin(method === "tv" ? "tv" : "web")} type="button">
              {busy ? <Loader2 className="spin" size={17} /> : <QrCode size={17} />}
              <span>{qrSrc ? "等待扫码" : "生成二维码"}</span>
            </button>
            {busy && qrSrc ? <button className="secondary-button full" onClick={onCancelLogin} type="button">取消扫码</button> : null}
          </div>
        ) : method === "token" ? (
          <div className="cookie-login">
            <label><span>接口</span><select value={apiMode === "WEB" ? "APP" : apiMode} onChange={(event) => onApiModeChange(event.target.value as ApiMode)}>
              <option value="TV">TV</option><option value="APP">APP</option><option value="INTL">INTL</option>
            </select></label>
            <label><span>Access Token</span><textarea value={accessToken} onChange={(event) => onTokenChange(event.target.value)} spellCheck={false} /></label>
            <button className="primary-button full" disabled={busy || !accessToken.trim()} onClick={onSaveToken} type="button">
              {busy ? <Loader2 className="spin" size={17} /> : <KeyRound size={17} />}<span>保存 Token</span>
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
