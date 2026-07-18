import { useEffect, useState } from "react";
import {
  Download,
  ExternalLink,
  FileText,
  ListChecks,
  Loader2,
  Play,
  Search,
  Square,
  UserRound,
  X,
} from "lucide-react";
import type { AppConfig, ParseResultV2, PartInfoV2, StreamBadge } from "../../types";

export interface ParseSession {
  id: string;
  input: string;
  title: string;
  status: "parsing" | "ready" | "error";
  result?: ParseResultV2;
  error?: string;
}

interface WorkspaceProps {
  config: AppConfig;
  input: string;
  sessions: ParseSession[];
  activeSessionId: string | null;
  parsing: boolean;
  setConfig: (config: AppConfig) => void;
  setInput: (value: string) => void;
  onActivateSession: (id: string) => void;
  onCloseSession: (id: string) => void;
  onDownload: () => void;
  onOpenLink: (url: string) => void;
  onParse: () => void;
  onStopParse: () => void;
}

const dfnPresets = [
  "4K 超清",
  "1080P 高码率",
  "1080P 高清",
  "杜比视界",
  "8K 超高清",
  "HDR 真彩",
  "720P 高清",
  "480P 清晰",
  "360P 流畅",
];

export function WorkspacePage({
  config,
  input,
  sessions,
  activeSessionId,
  parsing,
  setConfig,
  setInput,
  onActivateSession,
  onCloseSession,
  onDownload,
  onOpenLink,
  onParse,
  onStopParse,
}: WorkspaceProps) {
  const activeSession = sessions.find((session) => session.id === activeSessionId) ?? null;
  const result = activeSession?.result;
  const [expandedParts, setExpandedParts] = useState<Set<number>>(new Set([1]));
  const [episodePickerOpen, setEpisodePickerOpen] = useState(false);

  useEffect(() => {
    setExpandedParts(new Set([result?.parts[0]?.pageNumber ?? 1]));
  }, [activeSessionId, result?.id]);

  const setPageSelection = (pageSelection: string) => {
    setConfig({
      ...config,
      defaultOptions: { ...config.defaultOptions, pageSelection },
    });
  };

  return (
    <div className="workspace-page">
      <section className="parse-entry">
        <div className="input-wrap">
          <Search size={20} />
          <input
            value={input}
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") onParse();
            }}
            placeholder="输入 BV / AV / EP / SS / B 站视频地址"
          />
        </div>
        <button className="primary-button parse-button" disabled={parsing} onClick={onParse} type="button">
          {parsing ? <Loader2 className="spin" size={19} /> : <Play size={19} />}
          <span>{parsing ? "解析中" : "解析"}</span>
        </button>
        {parsing ? (
          <button className="icon-action danger" onClick={onStopParse} title="停止解析" type="button">
            <Square size={17} />
          </button>
        ) : null}
      </section>

      {sessions.length ? (
        <div className="parse-tabs" role="tablist">
          {sessions.map((session) => (
            <div className={session.id === activeSessionId ? "parse-tab active" : "parse-tab"} key={session.id}>
              <button onClick={() => onActivateSession(session.id)} role="tab" type="button">
                {session.status === "parsing" ? <Loader2 className="spin" size={14} /> : null}
                <span>{session.title}</span>
              </button>
              <button
                aria-label="关闭解析页"
                className="tab-close"
                disabled={session.status === "parsing"}
                onClick={() => onCloseSession(session.id)}
                title="关闭"
                type="button"
              >
                <X size={14} />
              </button>
            </div>
          ))}
        </div>
      ) : null}

      <div className="workspace-grid">
        <section className="primary-panel result-panel">
          {!activeSession ? <WorkspaceEmpty /> : null}
          {activeSession?.status === "parsing" ? <ParsingState input={activeSession.input} /> : null}
          {activeSession?.status === "error" ? (
            <div className="result-error">
              <FileText size={34} />
              <h3>解析失败</h3>
              <p>{activeSession.error}</p>
            </div>
          ) : null}
          {result ? (
            <VideoResult
              result={result}
              expandedParts={expandedParts}
              onOpenLink={onOpenLink}
              onTogglePart={(pageNumber) => {
                setExpandedParts((current) => {
                  const next = new Set(current);
                  if (next.has(pageNumber)) next.delete(pageNumber);
                  else next.add(pageNumber);
                  return next;
                });
              }}
            />
          ) : null}
        </section>

        <DownloadPanel
          config={config}
          result={result}
          setConfig={setConfig}
          onDownload={onDownload}
          onSelectParts={() => setEpisodePickerOpen(true)}
        />
      </div>

      {episodePickerOpen && result ? (
        <EpisodePicker
          current={config.defaultOptions.pageSelection}
          parts={result.parts}
          onClose={() => setEpisodePickerOpen(false)}
          onConfirm={(value) => {
            setPageSelection(value);
            setEpisodePickerOpen(false);
          }}
        />
      ) : null}
    </div>
  );
}

function WorkspaceEmpty() {
  return (
    <div className="workspace-empty">
      <div className="empty-icon"><Search size={34} /></div>
      <h3>输入视频地址开始解析</h3>
      <p>解析记录会保留在上方页签中</p>
    </div>
  );
}

function ParsingState({ input }: { input: string }) {
  return (
    <div className="workspace-empty">
      <Loader2 className="spin" size={36} />
      <h3>正在读取视频信息</h3>
      <p>{input}</p>
    </div>
  );
}

function VideoResult({
  result,
  expandedParts,
  onOpenLink,
  onTogglePart,
}: {
  result: ParseResultV2;
  expandedParts: Set<number>;
  onOpenLink: (url: string) => void;
  onTogglePart: (pageNumber: number) => void;
}) {
  const videoUrl = result.bvid ? `https://www.bilibili.com/video/${result.bvid}` : null;

  return (
    <div className="result-scroll">
      <div className="video-summary">
        <div className="cover-frame">
          {result.coverUrl ? <img alt="视频封面" referrerPolicy="no-referrer" src={result.coverUrl} /> : <FileText size={38} />}
        </div>
        <div className="summary-content">
          <div className="title-row">
            <h2>{result.title}</h2>
            {videoUrl ? (
              <button className="icon-action" onClick={() => onOpenLink(videoUrl)} title="打开视频页面" type="button">
                <ExternalLink size={17} />
              </button>
            ) : null}
          </div>
          <div className="owner-row">
            <div className="owner-avatar">
              {result.owner?.faceUrl ? <img alt="作者头像" referrerPolicy="no-referrer" src={result.owner.faceUrl} /> : <UserRound size={20} />}
            </div>
            <div>
              <strong>{result.owner?.name ?? "未知作者"}</strong>
              <span>{result.owner?.mid ? `UID ${result.owner.mid}` : ""}</span>
            </div>
            {result.owner?.spaceUrl ? (
              <button className="link-button" onClick={() => onOpenLink(result.owner!.spaceUrl!)} type="button">
                作者主页 <ExternalLink size={14} />
              </button>
            ) : null}
          </div>
          <div className="summary-facts">
            <span>{result.bvid ?? `AV${result.aid ?? "--"}`}</span>
            <span>{formatDuration(result.durationSeconds)}</span>
            <span>{result.parts.length} 个分 P</span>
            <span>{result.metadataSource === "bilibiliAndBbdown" ? "完整信息" : "BBDown 信息"}</span>
          </div>
        </div>
      </div>

      {result.description ? (
        <section className="description-block">
          <h3>视频简介</h3>
          <p>{result.description}</p>
        </section>
      ) : null}

      {result.warnings.length ? (
        <div className="warning-strip">{result.warnings.join(" ")}</div>
      ) : null}

      <section className="parts-section">
        <div className="section-title-row">
          <h3>分 P 与流信息</h3>
          <span>{result.parts.length}P</span>
        </div>
        <div className="stream-list">
          {result.parts.map((part) => (
            <article className="stream-part" key={part.pageNumber}>
              <button className="stream-part-title" onClick={() => onTogglePart(part.pageNumber)} type="button">
                <ListChecks size={17} />
                <span>P{part.pageNumber} · {part.title}</span>
                <span className="part-duration">{formatDuration(part.durationSeconds)}</span>
                <span className="part-count">{part.videoStreams.length + part.audioStreams.length} 条流</span>
              </button>
              {expandedParts.has(part.pageNumber) ? (
                <div className="stream-details">
                  <div className="part-meta"><span>CID {part.cid ?? "--"}</span></div>
                  {[...part.videoStreams, ...part.audioStreams].map((stream, index) => (
                    <div className="stream-row" key={`${stream.kind}-${index}`}>
                      <span className={`stream-kind ${stream.kind}`}>{stream.kind === "video" ? "视频" : "音频"}</span>
                      <code>{stream.rawText}</code>
                      <StreamBadges badges={stream.badges} />
                    </div>
                  ))}
                </div>
              ) : null}
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}

function DownloadPanel({
  config,
  result,
  setConfig,
  onDownload,
  onSelectParts,
}: {
  config: AppConfig;
  result?: ParseResultV2;
  setConfig: (config: AppConfig) => void;
  onDownload: () => void;
  onSelectParts: () => void;
}) {
  const updateDfnPriority = (values: string[]) => {
    setConfig({
      ...config,
      defaultOptions: { ...config.defaultOptions, dfnPriority: values },
    });
  };

  return (
    <aside className="tool-panel download-panel">
      <div className="download-primary">
        <div>
          <h3>下载</h3>
          <span>{result ? `${result.parts.length}P · ${config.defaultOptions.pageSelection}` : "解析后即可下载"}</span>
        </div>
        <button className="primary-button download-button" disabled={!result} onClick={onDownload} type="button">
          <Download size={18} />
          <span>下载视频</span>
        </button>
      </div>

      <div className="option-scroll">
        <section className="option-section">
          <div className="option-title"><span>分 P</span></div>
          <div className="inline-control">
            <input
              value={config.defaultOptions.pageSelection}
              onChange={(event) => setConfig({
                ...config,
                defaultOptions: { ...config.defaultOptions, pageSelection: event.target.value },
              })}
            />
            <button className="secondary-button icon-button" disabled={!result} onClick={onSelectParts} type="button">
              <ListChecks size={16} />
              <span>选择</span>
            </button>
          </div>
        </section>

        <section className="option-section">
          <div className="option-title"><span>下载内容</span></div>
          <fieldset className="media-toggle-grid">
            {[
              ["video", "视频"],
              ["audio", "音频"],
              ["danmaku", "弹幕"],
              ["subtitle", "字幕"],
              ["cover", "封面"],
            ].map(([key, label]) => (
              <label key={key}>
                <input
                  checked={config.defaultOptions.media[key as keyof typeof config.defaultOptions.media]}
                  onChange={(event) => setConfig({
                    ...config,
                    defaultOptions: {
                      ...config.defaultOptions,
                      media: { ...config.defaultOptions.media, [key]: event.target.checked },
                    },
                  })}
                  type="checkbox"
                />
                <span>{label}</span>
              </label>
            ))}
          </fieldset>
        </section>

        <section className="option-section">
          <div className="option-title"><span>画质优先级</span></div>
          <div className="preset-grid">
            {dfnPresets.map((preset) => {
              const index = config.defaultOptions.dfnPriority.indexOf(preset);
              return (
                <button
                  className={index >= 0 ? "preset selected" : "preset"}
                  key={preset}
                  onClick={() => updateDfnPriority(
                    index >= 0
                      ? config.defaultOptions.dfnPriority.filter((item) => item !== preset)
                      : [...config.defaultOptions.dfnPriority, preset],
                  )}
                  type="button"
                >
                  <span>{index >= 0 ? index + 1 : ""}</span>{preset}
                </button>
              );
            })}
          </div>
        </section>

        <section className="option-section">
          <div className="option-title"><span>编码优先级</span></div>
          <input
            value={config.defaultOptions.codecPriority.join(",")}
            onChange={(event) => setConfig({
              ...config,
              defaultOptions: { ...config.defaultOptions, codecPriority: splitList(event.target.value) },
            })}
          />
        </section>

        <section className="option-section">
          <div className="option-title"><span>音频优先级</span></div>
          <div className="segmented-control two">
            <button
              className={!config.advanced.audioAscending ? "selected" : ""}
              onClick={() => setConfig({ ...config, advanced: { ...config.advanced, audioAscending: false } })}
              type="button"
            >高码率</button>
            <button
              className={config.advanced.audioAscending ? "selected" : ""}
              onClick={() => setConfig({ ...config, advanced: { ...config.advanced, audioAscending: true } })}
              type="button"
            >低码率</button>
          </div>
        </section>
      </div>
    </aside>
  );
}

function EpisodePicker({
  current,
  parts,
  onClose,
  onConfirm,
}: {
  current: string;
  parts: PartInfoV2[];
  onClose: () => void;
  onConfirm: (value: string) => void;
}) {
  const [selected, setSelected] = useState<Set<number>>(() => parsePageSelection(current, parts));

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal">
        <header>
          <h3>选择分 P</h3>
          <button className="icon-action" onClick={onClose} title="关闭" type="button"><X size={17} /></button>
        </header>
        <div className="episode-toolbar">
          <button onClick={() => setSelected(new Set(parts.map((part) => part.pageNumber)))} type="button">全选</button>
          <button onClick={() => setSelected(new Set())} type="button">清空</button>
          <span>已选 {selected.size} 个</span>
        </div>
        <div className="episode-list">
          {parts.map((part) => (
            <label key={part.pageNumber}>
              <input
                checked={selected.has(part.pageNumber)}
                onChange={(event) => setSelected((currentSet) => {
                  const next = new Set(currentSet);
                  if (event.target.checked) next.add(part.pageNumber);
                  else next.delete(part.pageNumber);
                  return next;
                })}
                type="checkbox"
              />
              <span>P{part.pageNumber}</span>
              <strong>{part.title}</strong>
              <small>{formatDuration(part.durationSeconds)}</small>
            </label>
          ))}
        </div>
        <footer>
          <code>{formatPageSelection([...selected], parts.length)}</code>
          <button className="primary-button" disabled={!selected.size} onClick={() => onConfirm(formatPageSelection([...selected], parts.length))} type="button">
            确认
          </button>
        </footer>
      </div>
    </div>
  );
}

function StreamBadges({ badges }: { badges: StreamBadge[] }) {
  const visible = badges.filter((badge) => badge !== "normal");
  if (!visible.length) return null;
  return (
    <div className="stream-badges">
      {visible.map((badge) => <span className={`badge badge-${badge}`} key={badge}>{badgeLabel(badge)}</span>)}
    </div>
  );
}

function badgeLabel(badge: StreamBadge) {
  return { "8k": "8K", dolby: "Dolby", hdr: "HDR", "4k": "4K", high1080p: "高码率", normal: "" }[badge];
}

function formatDuration(seconds?: number) {
  if (seconds == null) return "--:--";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`
    : `${minutes}:${String(rest).padStart(2, "0")}`;
}

function splitList(value: string) {
  return value.split(",").map((item) => item.trim()).filter(Boolean);
}

function parsePageSelection(value: string, parts: PartInfoV2[]) {
  if (value.trim().toUpperCase() === "ALL") return new Set(parts.map((part) => part.pageNumber));
  const pages = new Set<number>();
  for (const token of value.split(",")) {
    const [startRaw, endRaw] = token.trim().split("-");
    const start = Number(startRaw);
    const end = endRaw ? Number(endRaw) : start;
    if (!Number.isFinite(start) || !Number.isFinite(end)) continue;
    for (let page = start; page <= end; page += 1) {
      if (parts.some((part) => part.pageNumber === page)) pages.add(page);
    }
  }
  return pages.size ? pages : new Set([parts[0]?.pageNumber ?? 1]);
}

function formatPageSelection(values: number[], total: number) {
  const sorted = [...new Set(values)].sort((a, b) => a - b);
  if (sorted.length === total) return "ALL";
  const ranges: string[] = [];
  let start = sorted[0];
  let end = sorted[0];
  for (const value of sorted.slice(1)) {
    if (value === end + 1) end = value;
    else {
      ranges.push(start === end ? `${start}` : `${start}-${end}`);
      start = value;
      end = value;
    }
  }
  if (start != null) ranges.push(start === end ? `${start}` : `${start}-${end}`);
  return ranges.join(",");
}
