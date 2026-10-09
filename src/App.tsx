import { useEffect, useMemo, useState } from "react";
import {
  BadgeCheck,
  ClipboardList,
  Download,
  FolderOpen,
  Search,
  Settings,
  Terminal,
  UserRound,
} from "lucide-react";
import { AccountDialog, type LoginMethod } from "./features/account/AccountDialog";
import { DownloadsPage } from "./features/downloads/DownloadsPage";
import { WorkspacePage, type ParseSession } from "./features/workspace/WorkspacePage";
import {
  buildDownloadPreview,
  controlDownloads,
  openTaskDirectory,
  buildPreviewCommand,
  detectTools,
  fallbackConfig,
  getAccountInfo,
  getConfig,
  listenLoginQr,
  listenApplicationLog,
  listenTaskStatus,
  listTasks,
  readApplicationLogs,
  recordApplicationLog,
  logoutAccount,
  openBilibiliLink,
  openDownloadDirectory,
  openToolDownloadPage,
  parseVideoV2,
  runDownload,
  runLogin,
  saveConfig,
  stopTask,
} from "./lib/api";
import type {
  AccountInfo,
  ApplicationLogEntry,
  AppConfig,
  ApiMode,
  DownloadAction,
  LoginQrEvent,
  TaskSnapshot,
  ToolDetectionResult,
} from "./types";

type Section = "workspace" | "tasks" | "settings" | "logs";

const navItems: Array<{ key: Section; label: string; icon: typeof Search }> = [
  { key: "workspace", label: "添加下载", icon: Search },
  { key: "tasks", label: "下载任务", icon: ClipboardList },
  { key: "settings", label: "设置", icon: Settings },
  { key: "logs", label: "日志", icon: Terminal },
];

const loggedOutAccount: AccountInfo = { isLoggedIn: false, source: "none" };

export function App() {
  const [active, setActive] = useState<Section>("workspace");
  const [config, setConfig] = useState<AppConfig>(fallbackConfig);
  const [input, setInput] = useState("");
  const [sessions, setSessions] = useState<ParseSession[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [parsing, setParsing] = useState(false);
  const [toolResult, setToolResult] = useState<ToolDetectionResult | null>(null);
  const [account, setAccount] = useState<AccountInfo>(loggedOutAccount);
  const [accountDialogOpen, setAccountDialogOpen] = useState(false);
  const [loginMethod, setLoginMethod] = useState<LoginMethod>("scan");
  const [loginBusy, setLoginBusy] = useState(false);
  const [loginTaskId, setLoginTaskId] = useState<string | null>(null);
  const [loginQr, setLoginQr] = useState<LoginQrEvent | null>(null);
  const [tasks, setTasks] = useState<TaskSnapshot[]>([]);
  const [logs, setLogs] = useState<ApplicationLogEntry[]>([]);
  const [activeParseTaskId, setActiveParseTaskId] = useState<string | null>(null);

  const appendLog = (entry: string) => {
    const nextLines = formatLogEntry(entry);
    const entries = nextLines.map((line) => ({ id: crypto.randomUUID(), timestamp: String(Math.floor(Date.now() / 1000)), line }));
    if (entries.length) setLogs((items) => mergeLogs(items, entries));
    for (const item of entries) {
      void recordApplicationLog(item)
        .then((saved) => setLogs((items) => mergeLogs(items, [saved])))
        .catch((error) => console.error("保存日志失败", error));
    }
  };

  const refreshAccount = async (nextConfig: AppConfig) => {
    try {
      const info = await getAccountInfo(nextConfig);
      setAccount(info);
      appendLog(info.isLoggedIn ? `[account] ${info.name ?? info.mid}` : "[account] 未登录");
      return info;
    } catch (error) {
      setAccount(loggedOutAccount);
      appendLog(`[account-error] ${toErrorMessage(error)}`);
      return loggedOutAccount;
    }
  };

  useEffect(() => {
    let cancelled = false;
    void readApplicationLogs().then((history) => {
      if (!cancelled) setLogs((items) => mergeLogs(history, items));
    }).catch((error) => appendLog(`[log-error] ${toErrorMessage(error)}`));
    getConfig()
      .then(async (nextConfig) => {
        if (cancelled) return;
        setConfig(nextConfig);
        appendLog("[startup] 配置已加载");
        const [tools] = await Promise.all([
          detectTools(nextConfig),
          refreshAccount(nextConfig),
        ]);
        if (!cancelled) {
          setToolResult(tools);
          tools.messages.forEach((message) => appendLog(`[tool] ${message}`));
        }
      })
      .catch((error) => appendLog(`[startup-error] ${toErrorMessage(error)}`));

    listTasks()
      .then((initialTasks) => {
        if (!cancelled) setTasks(initialTasks);
      })
      .catch((error) => appendLog(`[task-error] ${toErrorMessage(error)}`));

    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void listenTaskStatus((payload) => {
      if (cancelled) return;
      setTasks((items) => upsertTask(items, payload));

      if (payload.kind === "parse") {
        setActiveParseTaskId(payload.id);
        setParsing(["queued", "running", "stopping"].includes(payload.status));
        if (["completed", "failed", "canceled"].includes(payload.status)) {
          setActiveParseTaskId(null);
        }
      }
      if ((payload.kind === "loginWeb" || payload.kind === "loginTv")
        && ["completed", "failed", "canceled"].includes(payload.status)) {
        setLoginBusy(false);
        setLoginTaskId(null);
      }
      if ((payload.kind === "loginWeb" || payload.kind === "loginTv") && ["queued", "running"].includes(payload.status)) setLoginTaskId(payload.id);
    }).then((unlisten) => unlisteners.push(unlisten));

    void listenApplicationLog((payload) => {
      if (!cancelled) setLogs((items) => mergeLogs(items, [payload]));
    }).then((unlisten) => unlisteners.push(unlisten));

    void listenLoginQr((payload) => {
      if (cancelled) return;
      setLoginQr(payload);
      setAccountDialogOpen(true);
      setLoginMethod(payload.mode === "tv" ? "tv" : "scan");
      appendLog(`[login-qr] 二维码已生成: ${payload.imagePath}`);
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      cancelled = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  const selectedNav = useMemo(
    () => navItems.find((item) => item.key === active) ?? navItems[0],
    [active],
  );
  const activeSession = sessions.find((session) => session.id === activeSessionId) ?? null;

  const handleParse = async () => {
    const value = input.trim();
    if (!value || parsing) return;

    const sessionId = crypto.randomUUID();
    const session: ParseSession = {
      id: sessionId,
      input: value,
      title: compactSessionTitle(value),
      status: "parsing",
    };
    setSessions((items) => [...items, session]);
    setActiveSessionId(sessionId);
    setInput("");
    setParsing(true);
    appendLog(`[parse] ${value}`);

    const request = { input: value, config, showAllParts: true, useCache: true };
    try {
      const preview = await buildPreviewCommand(request);
      appendLog(`[command] ${preview.display}`);
      const result = await parseVideoV2(request);
      setSessions((items) => items.map((item) => item.id === sessionId
        ? { ...item, title: result.title, status: "ready", result, error: undefined }
        : item));
      setConfig((current) => ({
        ...current,
        defaultOptions: {
          ...current.defaultOptions,
          pageSelection: result.parts.length > 1 ? "ALL" : "1",
        },
      }));
      result.warnings.forEach((warning) => appendLog(`[warn] ${warning}`));
      appendLog(`[parse-result] ${result.title}`);
    } catch (error) {
      const message = toErrorMessage(error);
      setSessions((items) => items.map((item) => item.id === sessionId
        ? { ...item, status: "error", error: message }
        : item));
      appendLog(`[parse-error] ${message}`);
    } finally {
      setParsing(false);
    }
  };

  const handleCloseSession = (id: string) => {
    const closingIndex = sessions.findIndex((session) => session.id === id);
    const next = sessions.filter((session) => session.id !== id);
    setSessions(next);
    if (activeSessionId === id) {
      setActiveSessionId(next[Math.min(closingIndex, next.length - 1)]?.id ?? null);
    }
  };

  const handleDownload = async () => {
    const result = activeSession?.result;
    if (!result) return;
    try {
      const command = await buildDownloadPreview({ input: result.input, config, parseId: result.id });
      setActive("tasks");
      appendLog(`[download] ${command.display}`);
      const task = await runDownload({ input: result.input, config, parseId: result.id }, result.title);
      setTasks((items) => upsertTask(items, task));
      appendLog(`[download] 已加入队列: ${result.title}`);
    } catch (error) {
      appendLog(`[download-error] ${toErrorMessage(error)}`);
    }
  };

  const handleDownloadAction = async (ids: string[], action: DownloadAction) => {
    try {
      const result = await controlDownloads(ids, action);
      setTasks((items) => result.updated.reduce(upsertTask, items.filter((task) => !result.removed.includes(task.id))));
      result.errors.forEach((error) => appendLog(`[task-error] ${error}`));
    } catch (error) { appendLog(`[task-error] ${toErrorMessage(error)}`); }
  };
  useEffect(() => {
    const interval = window.setInterval(() => { void listTasks().then((snapshots) => setTasks((items) => snapshots.map((snapshot) => { const current = items.find((task) => task.id === snapshot.id); return current && (current.revision ?? 0) > (snapshot.revision ?? 0) ? current : snapshot; }))).catch((error) => appendLog(`[task-error] ${toErrorMessage(error)}`)); }, 1500);
    return () => window.clearInterval(interval);
  }, []);

  const handleDetectTools = async () => {
    try {
      const result = await detectTools(config);
      setToolResult(result);
      result.messages.forEach((message) => appendLog(`[tool] ${message}`));
    } catch (error) {
      appendLog(`[tool-error] ${toErrorMessage(error)}`);
    }
  };

  const handleSaveConfig = async () => {
    try {
      const saved = await saveConfig(config);
      setConfig(saved);
      appendLog("[settings] saved");
    } catch (error) {
      appendLog(`[settings-error] ${toErrorMessage(error)}`);
    }
  };

  const handleScanLogin = async (mode: "web" | "tv") => {
    const scanConfig: AppConfig = {
      ...config,
      auth: { ...config.auth, apiMode: mode === "web" ? "WEB" : "TV", cookie: mode === "web" ? "" : config.auth.cookie, accessToken: mode === "tv" ? "" : config.auth.accessToken },
    };
    setLoginBusy(true);
    setLoginQr(null);
    try {
      const saved = await saveConfig(scanConfig);
      setConfig(saved);
      appendLog(`[login] ${mode} scan`);
      const result = await runLogin(saved, mode);
      appendLog(result.output.trim() || `[login] exit ${result.exitCode ?? "--"}`);
      await refreshAccount(saved);
      if (result.success) setLoginQr(null);
    } catch (error) {
      appendLog(`[login-error] ${toErrorMessage(error)}`);
    } finally {
      setLoginBusy(false);
      setLoginQr(null);
    }
  };

  const handleVerifyCookie = async () => {
    setLoginBusy(true);
    try {
      const nextConfig: AppConfig = { ...config, auth: { ...config.auth, apiMode: "WEB" } };
      const saved = await saveConfig(nextConfig);
      setConfig(saved);
      await refreshAccount(saved);
    } catch (error) {
      appendLog(`[account-error] ${toErrorMessage(error)}`);
    } finally {
      setLoginBusy(false);
    }
  };

  const handleSaveToken = async () => {
    setLoginBusy(true);
    try {
      const nextConfig: AppConfig = { ...config, auth: { ...config.auth, apiMode: config.auth.apiMode === "WEB" ? "APP" : config.auth.apiMode } };
      const saved = await saveConfig(nextConfig);
      setConfig(saved);
      await refreshAccount(saved);
      appendLog("[account] Token 已保存，播放时由所选接口校验");
    } catch (error) {
      appendLog(`[account-error] ${toErrorMessage(error)}`);
    } finally { setLoginBusy(false); }
  };

  const handleLogout = async () => {
    if (!window.confirm("退出登录并删除本机保存的账号信息？")) return;
    setLoginBusy(true);
    try {
      const saved = await logoutAccount(config);
      setConfig(saved);
      setAccount(loggedOutAccount);
      setLoginQr(null);
      setLoginMethod("scan");
      appendLog("[account] 已退出登录，本机账号信息已删除");
    } catch (error) {
      appendLog(`[account-error] ${toErrorMessage(error)}`);
    } finally {
      setLoginBusy(false);
    }
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">BB</div>
          <div><h1>BBDown Next</h1><span>v0.1.0</span></div>
        </div>
        <nav className="nav-list" aria-label="主导航">
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <button className={item.key === active ? "nav-item active" : "nav-item"} key={item.key} onClick={() => setActive(item.key)} type="button">
                <Icon size={18} /><span>{item.label}</span>
              </button>
            );
          })}
        </nav>
        <div className="sidebar-footer">
          <span className={toolResult?.coreAvailable ? "tool-state ok" : "tool-state"}>
            <i />{toolResult?.coreAvailable ? "内置核心已就绪" : "等待桌面后端"}
          </span>
        </div>
      </aside>

      <main className="main">
        <header className="topbar">
          <div><span className="eyebrow">{selectedNav.label}</span><h2>{selectedNav.label}</h2></div>
          <button className={account.isLoggedIn ? "account-chip logged-in" : "account-chip"} onClick={() => setAccountDialogOpen(true)} type="button">
            <div className="account-avatar">
              {account.avatarUrl ? <img alt="账号头像" referrerPolicy="no-referrer" src={account.avatarUrl} /> : <UserRound size={18} />}
            </div>
            <div>
              <strong>{account.isLoggedIn ? account.name : account.tokenConfigured ? "Token 已配置" : "未登录"}</strong>
              <span>{account.isLoggedIn ? `WEB · ${account.vipLabel ?? `UID ${account.mid}`}` : account.tokenConfigured ? `${config.auth.apiMode} · 播放时校验` : "点击登录账号"}</span>
            </div>
          </button>
        </header>

        {active === "workspace" ? (
          <WorkspacePage
            config={config}
            input={input}
            sessions={sessions}
            activeSessionId={activeSessionId}
            parsing={parsing}
            setConfig={setConfig}
            setInput={setInput}
            onActivateSession={setActiveSessionId}
            onCloseSession={handleCloseSession}
            onDownload={handleDownload}
            onOpenLink={(url) => openBilibiliLink(url).catch((error) => appendLog(`[open-error] ${toErrorMessage(error)}`))}
            onParse={handleParse}
            onStopParse={() => {
              if (!activeParseTaskId) return;
              stopTask(activeParseTaskId).catch((error) => appendLog(`[stop-error] ${toErrorMessage(error)}`));
            }}
          />
        ) : null}

        {active === "tasks" ? <DownloadsPage tasks={tasks} onAction={handleDownloadAction} onOpenDirectory={(id) => void openTaskDirectory(id).catch((error) => appendLog(`[open-error] ${toErrorMessage(error)}`))} onAddDownload={() => setActive("workspace")} /> : null}
        {active === "settings" ? (
          <SettingsPage
            config={config}
            setConfig={setConfig}
            toolResult={toolResult}
            onDetectTools={handleDetectTools}
            onOpenDirectory={() => openDownloadDirectory(config.workDir).catch((error) => appendLog(`[open-error] ${toErrorMessage(error)}`))}
            onOpenToolDownload={(tool) => openToolDownloadPage(tool).catch((error) => appendLog(`[open-error] ${toErrorMessage(error)}`))}
            onSaveConfig={handleSaveConfig}
          />
        ) : null}
        {active === "logs" ? <LogsPage logs={logs} /> : null}
      </main>

      {accountDialogOpen ? (
        <AccountDialog
          account={account}
          apiMode={config.auth.apiMode}
          accessToken={config.auth.accessToken ?? ""}
          busy={loginBusy}
          cookie={config.auth.cookie ?? ""}
          loginQr={loginQr}
          method={loginMethod}
          onClose={() => setAccountDialogOpen(false)}
          onApiModeChange={(apiMode) => setConfig({ ...config, auth: { ...config.auth, apiMode } })}
          onTokenChange={(accessToken) => setConfig({ ...config, auth: { ...config.auth, accessToken } })}
          onSaveToken={handleSaveToken}
          onCancelLogin={() => { if (loginTaskId) void stopTask(loginTaskId).catch((error) => appendLog(`[login-error] ${toErrorMessage(error)}`)); }}
          onCookieChange={(cookie) => setConfig({ ...config, auth: { ...config.auth, cookie } })}
          onMethodChange={setLoginMethod}
          onLogout={handleLogout}
          onScanLogin={handleScanLogin}
          onVerifyCookie={handleVerifyCookie}
        />
      ) : null}
    </div>
  );
}

function SettingsPage({
  config,
  setConfig,
  toolResult,
  onDetectTools,
  onOpenDirectory,
  onOpenToolDownload,
  onSaveConfig,
}: {
  config: AppConfig;
  setConfig: (config: AppConfig) => void;
  toolResult: ToolDetectionResult | null;
  onDetectTools: () => void;
  onOpenDirectory: () => void;
  onOpenToolDownload: (tool: "ffmpeg") => void;
  onSaveConfig: () => void;
}) {
  return (
    <div className="settings-page">
      <section className="primary-panel settings-panel">
        <div className="section-title"><Settings size={19} /><h3>基础设置</h3></div>
        <label className="path-input">
          <span>播放接口</span>
          <select value={config.auth.apiMode} onChange={(event) => setConfig({ ...config, auth: { ...config.auth, apiMode: event.target.value as ApiMode } })}>
            <option value="WEB">WEB · Cookie</option><option value="TV">TV · TV 扫码 / Token</option>
            <option value="APP">APP · Token / 公开内容</option><option value="INTL">INTL · 国际版番剧</option>
          </select>
        </label>
        <div className="queue-settings">
          <label><span>同时下载任务数</span><input type="number" min={1} max={8} value={config.downloadManager.maxConcurrentTasks} onChange={(event) => setConfig({ ...config, downloadManager: { ...config.downloadManager, maxConcurrentTasks: Math.min(8, Math.max(1, Number(event.target.value))) } })} /></label>
          <label><span>每个任务的连接数</span><input type="number" min={1} max={16} value={config.downloadManager.connectionsPerTask} onChange={(event) => setConfig({ ...config, downloadManager: { ...config.downloadManager, connectionsPerTask: Math.min(16, Math.max(1, Number(event.target.value))) } })} /></label>
          <label className="queue-auto-resume"><input type="checkbox" checked={config.downloadManager.resumeOnStart} onChange={(event) => setConfig({ ...config, downloadManager: { ...config.downloadManager, resumeOnStart: event.target.checked } })} />启动后自动继续未完成任务</label>
          <p>并发上限控制后续启动的任务，不打断当前下载；连接数用于新任务。</p>
        </div>
        <PathInput label="FFmpeg" value={config.tools.ffmpegPath ?? ""} onChange={(value) => setConfig({ ...config, tools: { ...config.tools, ffmpegPath: value } })} onDownload={() => onOpenToolDownload("ffmpeg")} />
        <label className="path-input">
          <span>下载目录</span>
          <div className="path-with-action">
            <input value={config.workDir} onChange={(event) => setConfig({ ...config, workDir: event.target.value })} />
            <button className="secondary-button" onClick={onOpenDirectory} type="button"><FolderOpen size={16} /><span>打开</span></button>
          </div>
        </label>
        <div className="settings-actions">
          <button className="secondary-button" onClick={onDetectTools} type="button"><Search size={17} /><span>检测工具</span></button>
          <button className="primary-button" onClick={onSaveConfig} type="button"><BadgeCheck size={17} /><span>保存设置</span></button>
        </div>
        {toolResult ? <div className="tool-result">{toolResult.messages.map((message) => <span key={message}>{message}</span>)}</div> : null}
      </section>
    </div>
  );
}

function LogsPage({ logs }: { logs: ApplicationLogEntry[] }) {
  return (
    <section className="primary-panel terminal-panel">
      <div className="section-title"><Terminal size={19} /><h3>应用日志</h3></div>
      <div className="log-lines">
        {logs.length ? logs.map((entry) => <code key={entry.id}>[{new Date(Number(entry.timestamp) * 1000).toLocaleString()}] {entry.line}</code>) : <span>暂无日志</span>}
      </div>
    </section>
  );
}

function mergeLogs(current: ApplicationLogEntry[], incoming: ApplicationLogEntry[]): ApplicationLogEntry[] {
  const unique = new Map(current.map((entry) => [entry.id, entry]));
  for (const entry of incoming) unique.set(entry.id, entry);
  return [...unique.values()].sort((a, b) => Number(a.timestamp) - Number(b.timestamp)).slice(-1000);
}

function PathInput({
  label,
  value,
  onChange,
  onDownload,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onDownload: () => void;
}) {
  return (
    <label className="path-input">
      <span>{label}</span>
      <div className="path-with-action">
        <input value={value} onChange={(event) => onChange(event.target.value)} />
        <button className="secondary-button" onClick={onDownload} type="button"><Download size={16} /><span>下载</span></button>
      </div>
    </label>
  );
}

function upsertTask(items: TaskSnapshot[], payload: TaskSnapshot): TaskSnapshot[] {
  const index = items.findIndex((task) => task.id === payload.id);
  if (index >= 0 && (items[index].revision ?? 0) > (payload.revision ?? 0)) return items;
  return index >= 0 ? items.map((task, i) => i === index ? payload : task) : [payload, ...items];
}

function compactSessionTitle(value: string) {
  const match = value.match(/(?:BV|bv)[A-Za-z0-9]{10}|(?:av|AV)\d+|(?:ep|EP|ss|SS)\d+/);
  return match?.[0] ?? (value.length > 24 ? `${value.slice(0, 24)}...` : value);
}

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function formatLogEntry(entry: string) {
  return entry
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter((line) => line.trim().length > 0)
    .filter((line) => !/^https?:\/\/\S+$/i.test(line.trim()))
    .map((line) => line.length > 260 ? `${line.slice(0, 240)} ...` : line);
}
