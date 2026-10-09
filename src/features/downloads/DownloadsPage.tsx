import { useMemo, useState } from "react";
import { ArrowDownToLine, Check, FolderOpen, Pause, Play, Plus, RotateCcw, Search, Square, Trash2, X } from "lucide-react";
import type { DownloadAction, TaskSnapshot } from "../../types";

interface Props {
  tasks: TaskSnapshot[];
  onAction: (ids: string[], action: DownloadAction) => Promise<void>;
  onOpenDirectory: (id: string) => void;
  onAddDownload: () => void;
}
type Filter = "all" | "active" | "paused" | "failed" | "completed";
const working = (task: TaskSnapshot) => ["running", "pausing", "stopping"].includes(task.status);
const resumable = (task: TaskSnapshot) => ["paused", "failed", "canceled"].includes(task.status);

export function DownloadsPage({ tasks, onAction, onOpenDirectory, onAddDownload }: Props) {
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [detail, setDetail] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const downloads = useMemo(() => tasks.filter((task) => task.kind === "download"), [tasks]);
  const counts = {
    all: downloads.length,
    active: downloads.filter((task) => task.status === "queued" || working(task)).length,
    paused: downloads.filter((task) => task.status === "paused").length,
    failed: downloads.filter((task) => task.status === "failed" || task.status === "canceled").length,
    completed: downloads.filter((task) => task.status === "completed").length,
  };
  const visible = downloads.filter((task) => {
    const matches = filter === "all" || (filter === "active" && (working(task) || task.status === "queued"))
      || (filter === "paused" && task.status === "paused")
      || (filter === "failed" && ["failed", "canceled"].includes(task.status))
      || (filter === "completed" && task.status === "completed");
    return matches && `${task.title ?? ""} ${task.input}`.toLowerCase().includes(query.toLowerCase());
  });
  const pageCount = Math.max(1, Math.ceil(visible.length / 40));
  const currentPage = Math.min(page, pageCount);
  const rows = visible.slice((currentPage - 1) * 40, currentPage * 40);
  const actionable = downloads.filter((task) => selected.has(task.id));
  const throughput = downloads.reduce((sum, task) => sum + (task.speedBytesPerSecond ?? 0), 0);
  const currentDetail = downloads.find((task) => task.id === detail);
  const invokeAction = async (targets: TaskSnapshot[], action: DownloadAction) => {
    if (!targets.length || busy) return;
    setBusy(true);
    try { await onAction(targets.map((task) => task.id), action); }
    finally { setBusy(false); if (action === "remove") setSelected(new Set()); }
  };
  const toggle = (id: string) => setSelected((items) => { const next = new Set(items); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  const allSelected = rows.length > 0 && rows.every((task) => selected.has(task.id));

  return (
    <section className="download-manager primary-panel">
      <div className="download-manager-heading">
        <div><h3>下载管理</h3><p>{counts.active} 个进行中 · {counts.paused} 个已暂停</p></div>
        <div className="queue-throughput"><ArrowDownToLine size={17} /><strong>{bytes(throughput)}/s</strong><span>当前速度</span></div>
        <button className="primary-button" onClick={onAddDownload} type="button"><Plus size={17} />添加下载</button>
      </div>
      <div className="download-manager-tools">
        <div className="queue-filters" aria-label="任务状态">
          {([ ["all", "全部"], ["active", "进行中"], ["paused", "已暂停"], ["failed", "失败与取消"], ["completed", "已完成"] ] as const).map(([key, label]) => (
            <button type="button" key={key} className={filter === key ? "selected" : ""} onClick={() => { setFilter(key); setPage(1); }}>{label}<span>{counts[key]}</span></button>
          ))}
        </div>
        <label className="queue-search"><Search size={15} /><input aria-label="搜索下载任务" placeholder="搜索名称或链接" value={query} onChange={(event) => { setQuery(event.target.value); setPage(1); }} /></label>
      </div>
      <div className="queue-batch-bar">
        <label><input type="checkbox" checked={allSelected} disabled={!rows.length} onChange={() => setSelected((items) => { const next = new Set(items); for (const task of rows) { if (allSelected) next.delete(task.id); else next.add(task.id); } return next; })} />本页全选</label>
        <span>{actionable.length ? `已选 ${actionable.length} 项` : "任务按加入顺序排队"}</span>
        <button disabled={busy || !actionable.some((task) => ["running", "queued"].includes(task.status))} onClick={() => void invokeAction(actionable.filter((task) => ["running", "queued"].includes(task.status)), "pause")} type="button"><Pause size={14} />暂停选中</button>
        <button disabled={busy || !actionable.some(resumable)} onClick={() => void invokeAction(actionable.filter(resumable), "resume")} type="button"><Play size={14} />继续选中</button>
        <button disabled={busy || !downloads.some((task) => task.status === "failed")} onClick={() => void invokeAction(downloads.filter((task) => task.status === "failed"), "retry")} type="button"><RotateCcw size={14} />重试失败</button>
        <button disabled={busy || !downloads.some((task) => task.status === "completed")} onClick={() => void invokeAction(downloads.filter((task) => task.status === "completed"), "remove")} type="button"><Trash2 size={14} />清除已完成记录</button>
      </div>
      <div className="queue-table" role="list" aria-label="下载任务">
        <div className="queue-table-header"><span /><span>名称与进度</span><span>传输</span><span>操作</span></div>
        {rows.map((task) => {
          const total = task.currentFileTotal;
          const percent = task.status === "completed" ? 100 : total && total > 0 ? Math.min(100, (task.currentFileDownloaded ?? 0) * 100 / total) : null;
          return (
            <article className={`queue-row ${task.status}`} key={task.id} role="listitem">
              <input type="checkbox" aria-label={`选择 ${task.title ?? task.input}`} checked={selected.has(task.id)} onChange={() => toggle(task.id)} />
              <div className="queue-main">
                <div className="queue-title"><button type="button" onClick={() => setDetail(task.id)}>{task.title ?? task.input}</button><span className={`queue-state ${task.status}`}>{status(task.status)}</span></div>
                <p className={task.status === "failed" ? "queue-message error" : "queue-message"}>{task.errorMessage ?? task.latestMessage ?? "等待下载"}</p>
                <div className="queue-progress"><div className="queue-progress-track" role="progressbar" aria-label="当前文件进度" aria-valuenow={percent ?? undefined} aria-valuemin={0} aria-valuemax={100}><i style={{ width: `${percent ?? 0}%` }} /></div><span>{percent === null ? "准备中" : `${percent.toFixed(1)}%`}</span></div>
                <div className="queue-meta"><span>{task.currentFile ?? (task.status === "completed" ? "已保存文件" : "等待解析")}{task.status !== "completed" && total ? ` ${bytes(task.currentFileDownloaded ?? 0)} / ${bytes(total)}` : ""}</span><span>{task.selectedParts ? `已完成 ${task.completedParts ?? 0} / ${task.selectedParts} 个分 P` : task.input}</span></div>
              </div>
              <div className="queue-transfer"><strong>{task.status === "running" ? `${bytes(task.speedBytesPerSecond ?? 0)}/s` : "—"}</strong><span>累计 {bytes(task.downloadedBytes ?? 0)}</span><span>{task.status === "running" && task.etaSeconds ? `当前文件剩余 ${duration(task.etaSeconds)}` : time(task.createdAt ?? task.startedAt)}</span></div>
              <div className="queue-actions">
                {["queued", "running"].includes(task.status) ? <button disabled={busy} title="暂停" aria-label="暂停任务" onClick={() => void invokeAction([task], "pause")} type="button"><Pause size={17} /></button> : null}
                {resumable(task) ? <button disabled={busy} title={task.status === "paused" ? "继续" : "重试"} aria-label="继续或重试任务" onClick={() => void invokeAction([task], task.status === "paused" ? "resume" : "retry")} type="button">{task.status === "paused" ? <Play size={17} /> : <RotateCcw size={17} />}</button> : null}
                {!(["completed", "canceled", "pausing", "stopping"].includes(task.status)) ? <button disabled={busy} title="取消" aria-label="取消任务" onClick={() => void invokeAction([task], "cancel")} type="button"><Square size={15} /></button> : null}
                <button title="打开下载目录" aria-label="打开下载目录" onClick={() => onOpenDirectory(task.id)} type="button"><FolderOpen size={17} /></button>
                {!working(task) && task.status !== "queued" ? <button disabled={busy} title="移除记录，保留下载文件" aria-label="移除任务记录" onClick={() => void invokeAction([task], "remove")} type="button"><X size={17} /></button> : null}
              </div>
            </article>
          );
        })}
        {!rows.length ? <div className="queue-empty"><ArrowDownToLine size={30} /><h4>{downloads.length ? "没有匹配的任务" : "队列还没有任务"}</h4><p>{downloads.length ? "切换状态或调整搜索内容。" : "添加链接并选择分 P，下载任务会在这里排队。"}</p>{!downloads.length ? <button className="secondary-button" onClick={onAddDownload} type="button">添加下载</button> : null}</div> : null}
      </div>
      <div className="queue-footer"><span>暂停和取消会保留下载数据；移除记录仅清理该任务缓存。</span><button type="button" disabled={currentPage <= 1} onClick={() => setPage(currentPage - 1)}>上一页</button><span>{currentPage} / {pageCount}</span><button type="button" disabled={currentPage >= pageCount} onClick={() => setPage(currentPage + 1)}>下一页</button></div>
      {currentDetail ? <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label="任务详情"><div className="queue-detail modal"><header><h3>任务详情</h3><button className="icon-action" type="button" aria-label="关闭详情" onClick={() => setDetail(null)}><X size={17} /></button></header><h4>{currentDetail.title ?? currentDetail.input}</h4><p>{currentDetail.input}</p><dl><dt>状态</dt><dd>{status(currentDetail.status)}</dd><dt>下载目录</dt><dd>{currentDetail.downloadDir}</dd><dt>添加时间</dt><dd>{time(currentDetail.createdAt)}</dd><dt>已完成分 P</dt><dd>{currentDetail.completedParts ?? 0} / {currentDetail.selectedParts ?? 0}</dd></dl>{currentDetail.errorMessage ? <p className="error">{currentDetail.errorMessage}</p> : null}<h4>已保存文件</h4>{currentDetail.outputFiles?.length ? <ul>{currentDetail.outputFiles.map((file) => <li key={file}>{file}</li>)}</ul> : <p>尚未保存完整文件。</p>}<button className="secondary-button" type="button" onClick={() => onOpenDirectory(currentDetail.id)}><FolderOpen size={16} />打开目录</button></div></div> : null}
    </section>
  );
}
function bytes(value: number) { const units = ["B", "KB", "MB", "GB", "TB"]; let index = 0; while (value >= 1024 && index < units.length - 1) { value /= 1024; index += 1; } return `${value.toFixed(index ? 1 : 0)} ${units[index]}`; }
function duration(seconds: number) { return seconds >= 3600 ? `${Math.floor(seconds / 3600)} 时 ${Math.ceil(seconds % 3600 / 60)} 分` : seconds >= 60 ? `${Math.floor(seconds / 60)} 分 ${Math.ceil(seconds % 60)} 秒` : `${Math.ceil(seconds)} 秒`; }
function time(value?: string) { return value && Number.isFinite(Number(value)) ? new Date(Number(value) * 1000).toLocaleString() : "—"; }
function status(value: TaskSnapshot["status"]) { return { queued: "等待中", running: "下载中", pausing: "暂停中", paused: "已暂停", stopping: "取消中", completed: "已完成", failed: "失败", canceled: "已取消" }[value]; }
