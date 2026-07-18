import { useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  BadgeCheck,
  ClipboardList,
  Download,
  FolderOpen,
  Search,
  Settings,
  Square,
  Terminal,
  UserRound,
} from "lucide-react";
import { AccountDialog, type LoginMethod } from "./features/account/AccountDialog";
import { WorkspacePage, type ParseSession } from "./features/workspace/WorkspacePage";
import {
  buildDownloadPreview,
  buildPreviewCommand,
  detectTools,
  fallbackConfig,
  getAccountInfo,
  getConfig,
  listenLoginQr,
  listenTaskLog,
  listenTaskStatus,
  listTasks,
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
  AppConfig,
  BackendTaskStatus,
  LoginQrEvent,
  TaskSnapshot,
  ToolDetectionResult,
} from "./types";

interface RuntimeTask {
  id: string;
  kind: TaskSnapshot["kind"];
  input: string;
  title: string;
  status: BackendTaskStatus;
  phase?: TaskSnapshot["phase"];
  latestMessage?: string;
  currentPartIndex?: number;
  totalParts?: number;
  startedAt?: string;
  finishedAt?: string;
  exitCode?: number;
  command: string;
  createdAt: string;
  output?: string;
}

type Section = "workspace" | "tasks" | "settings" | "logs";

const navItems: Array<{ key: Section; label: string; icon: typeof Search }> = [
  { key: "workspace", label: "首页", icon: Search },
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
  const [loginQr, setLoginQr] = useState<LoginQrEvent | null>(null);
  const [tasks, setTasks] = useState<RuntimeTask[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [activeParseTaskId, setActiveParseTaskId] = useState<string | null>(null);
  const pendingTaskRef = useRef<{ input: string; title: string; command: string } | null>(null);

  const appendLog = (entry: string) => {
    const nextLines = formatLogEntry(entry);
    if (nextLines.length) setLogs((items) => [...items, ...nextLines].slice(-260));
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
        if (!cancelled) setTasks(initialTasks.map(taskFromSnapshot));
      })
      .catch((error) => appendLog(`[task-error] ${toErrorMessage(error)}`));

    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void listenTaskStatus((payload) => {
      if (cancelled) return;
      setTasks((items) => upsertTask(items, payload, pendingTaskRef.current));

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
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    void listenTaskLog((payload) => {
      if (!cancelled) appendLog(`[${payload.taskId.slice(0, 8)}] ${payload.line}`);
    }).then((unlisten) => unlisteners.push(unlisten));

    void listenLoginQr((payload) => {
      if (cancelled) return;
      setLoginQr(payload);
      setAccountDialogOpen(true);
      setLoginMethod("scan");
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
      pendingTaskRef.current = { input: result.input, title: result.title, command: command.display };
      setActive("tasks");
      appendLog(`[download] ${command.display}`);
      const output = await runDownload({ input: result.input, config, parseId: result.id });
      appendLog(output.output.trim() || `[download] exit ${output.exitCode ?? "--"}`);
    } catch (error) {
      appendLog(`[download-error] ${toErrorMessage(error)}`);
    }
  };

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

  const handleScanLogin = async () => {
    const scanConfig: AppConfig = {
      ...config,
      auth: { ...config.auth, apiMode: "WEB", cookie: "" },
    };
    setLoginBusy(true);
    setLoginQr(null);
    try {
      const saved = await saveConfig(scanConfig);
      setConfig(saved);
      appendLog("[login] web scan");
      const result = await runLogin(saved, "web");
      appendLog(result.output.trim() || `[login] exit ${result.exitCode ?? "--"}`);
      const info = await refreshAccount(saved);
      if (info.isLoggedIn) setLoginQr(null);
    } catch (error) {
      appendLog(`[login-error] ${toErrorMessage(error)}`);
    } finally {
      setLoginBusy(false);
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
          <span className={toolResult?.bbdownFound ? "tool-state ok" : "tool-state"}>
            <i />{toolResult?.bbdownVersion ?? "BBDown 未检测"}
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
              <strong>{account.isLoggedIn ? account.name : "未登录"}</strong>
              <span>{account.isLoggedIn ? account.vipLabel ?? `UID ${account.mid}` : "点击登录账号"}</span>
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

        {active === "tasks" ? <TasksPage tasks={tasks} onStopTask={(id) => stopTask(id).catch((error) => appendLog(`[stop-error] ${toErrorMessage(error)}`))} /> : null}
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
          busy={loginBusy}
          cookie={config.auth.cookie ?? ""}
          loginQr={loginQr}
          method={loginMethod}
          onClose={() => setAccountDialogOpen(false)}
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

function TasksPage({ tasks, onStopTask }: { tasks: RuntimeTask[]; onStopTask: (id: string) => void }) {
  const downloads = tasks.filter((task) => task.kind === "download");
  const current = downloads.filter((task) => ["queued", "running", "stopping"].includes(task.status));
  const finished = downloads.filter((task) => !["queued", "running", "stopping"].includes(task.status));
  return (
    <section className="primary-panel tasks-panel">
      <TaskGroup title="当前下载" tasks={current} onStopTask={onStopTask} />
      <TaskGroup title="已完成与历史" tasks={finished} onStopTask={onStopTask} />
    </section>
  );
}

function TaskGroup({ title, tasks, onStopTask }: { title: string; tasks: RuntimeTask[]; onStopTask: (id: string) => void }) {
  return (
    <section className="task-group">
      <div className="section-title"><Activity size={18} /><h3>{title}</h3><span>{tasks.length}</span></div>
      {tasks.length ? (
        <div className="task-list">
          {tasks.map((task) => (
            <article className="task-item" key={task.id}>
              <div><h4>{task.title}</h4><span>{task.latestMessage ?? task.createdAt}</span></div>
              <span className={`task-status ${task.status}`}>{statusLabel(task.status)}</span>
              {task.currentPartIndex && task.totalParts ? <span>P {task.currentPartIndex} / {task.totalParts}</span> : null}
              {task.status === "running" || task.status === "stopping" ? (
                <button className="icon-action danger" onClick={() => onStopTask(task.id)} title="停止任务" type="button"><Square size={16} /></button>
              ) : null}
            </article>
          ))}
        </div>
      ) : <div className="group-empty">暂无任务</div>}
    </section>
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
  onOpenToolDownload: (tool: "bbdown" | "ffmpeg") => void;
  onSaveConfig: () => void;
}) {
  return (
    <div className="settings-page">
      <section className="primary-panel settings-panel">
        <div className="section-title"><Settings size={19} /><h3>基础设置</h3></div>
        <PathInput label="BBDown" value={config.tools.bbdownPath} onChange={(value) => setConfig({ ...config, tools: { ...config.tools, bbdownPath: value } })} onDownload={() => onOpenToolDownload("bbdown")} />
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

function LogsPage({ logs }: { logs: string[] }) {
  return (
    <section className="primary-panel terminal-panel">
      <div className="section-title"><Terminal size={19} /><h3>应用日志</h3></div>
      <div className="log-lines">
        {logs.length ? logs.map((line, index) => <code key={`${line}-${index}`}>{line}</code>) : <span>暂无日志</span>}
      </div>
    </section>
  );
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

function taskFromSnapshot(task: TaskSnapshot): RuntimeTask {
  return {
    id: task.id,
    kind: task.kind,
    input: task.input,
    title: task.title ?? task.input,
    status: task.status,
    phase: task.phase,
    latestMessage: task.latestMessage,
    currentPartIndex: task.currentPartIndex,
    totalParts: task.totalParts,
    startedAt: task.startedAt,
    finishedAt: task.finishedAt,
    exitCode: task.exitCode,
    command: task.input,
    createdAt: task.startedAt ?? new Date().toLocaleString(),
  };
}

function upsertTask(
  items: RuntimeTask[],
  payload: TaskSnapshot,
  pending: { input: string; title: string; command: string } | null,
) {
  const index = items.findIndex((item) => item.id === payload.id);
  const previous = index >= 0 ? items[index] : null;
  const next: RuntimeTask = {
    ...taskFromSnapshot(payload),
    title: payload.title ?? previous?.title ?? pending?.title ?? payload.input,
    command: previous?.command ?? pending?.command ?? payload.input,
    createdAt: previous?.createdAt ?? new Date().toLocaleString(),
    output: previous?.output,
  };
  return index >= 0 ? items.map((item, itemIndex) => itemIndex === index ? next : item) : [next, ...items];
}

function compactSessionTitle(value: string) {
  const match = value.match(/(?:BV|bv)[A-Za-z0-9]{10}|(?:av|AV)\d+|(?:ep|EP|ss|SS)\d+/);
  return match?.[0] ?? (value.length > 24 ? `${value.slice(0, 24)}...` : value);
}

function statusLabel(status: BackendTaskStatus) {
  return {
    queued: "等待中",
    running: "下载中",
    stopping: "停止中",
    completed: "已完成",
    failed: "失败",
    canceled: "已取消",
  }[status];
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
