export type ApiMode = "WEB" | "TV" | "APP" | "INTL";

export type TaskStatus =
  | "queued"
  | "parsing"
  | "running"
  | "stopping"
  | "completed"
  | "failed"
  | "canceled";

export type StreamBadge = "8k" | "dolby" | "hdr" | "4k" | "high1080p" | "normal";
export type ContentKind = "singleVideo" | "multiPartVideo" | "bangumiEpisode" | "bangumiMultiEpisode" | "unknown";

export interface ToolPaths {
  bbdownPath: string;
  ffmpegPath?: string;
  mp4boxPath?: string;
  aria2cPath?: string;
}

export interface AuthConfig {
  apiMode: ApiMode;
  cookie?: string;
  accessToken?: string;
  userAgent?: string;
}

export interface DownloadOptions {
  media: {
    video: boolean;
    audio: boolean;
    danmaku: boolean;
    subtitle: boolean;
    cover: boolean;
  };
  pageSelection: string;
  codecPriority: string[];
  dfnPriority: string[];
  mux: boolean;
  skipAiSubtitle: boolean;
  multiThread: boolean;
}

export interface AdvancedOptions {
  forceHttp: boolean;
  useAria2c: boolean;
  aria2cArgs?: string;
  useMp4box: boolean;
  allowPcdn: boolean;
  videoAscending: boolean;
  audioAscending: boolean;
  filePattern?: string;
  multiFilePattern?: string;
  language?: string;
  delayPerPage?: number;
  uposHost?: string;
  forceReplaceHost?: boolean;
  saveArchivesToFile?: boolean;
}

export interface AppConfig {
  tools: ToolPaths;
  workDir: string;
  auth: AuthConfig;
  defaultOptions: DownloadOptions;
  advanced: AdvancedOptions;
}

export interface ParseRequest {
  input: string;
  config: AppConfig;
  showAllParts: boolean;
  useCache: boolean;
}

export interface DownloadRequest {
  input: string;
  config: AppConfig;
  parseId?: string;
}

export interface StreamInfo {
  kind: "video" | "audio";
  rawText: string;
  qualityLabel?: string;
  resolution?: string;
  codec?: string;
  fps?: string;
  bitrate?: string;
  approxSize?: string;
  badges: StreamBadge[];
}

export interface PartInfo {
  pageNumber: number;
  cid?: string;
  title: string;
  duration?: string;
  videoStreams: StreamInfo[];
  audioStreams: StreamInfo[];
}

export interface ParseResult {
  id: string;
  input: string;
  contentKind: ContentKind;
  aidOrEpisodeId?: string;
  bvid?: string;
  title: string;
  ownerName?: string;
  ownerSpaceUrl?: string;
  publishTime?: string;
  duration?: string;
  partCount?: number;
  coverPath?: string;
  savePath?: string;
  parts: PartInfo[];
  rawOutput: string;
  warnings: string[];
  errorMessage?: string;
}

export interface OwnerInfo {
  mid?: number;
  name?: string;
  faceUrl?: string;
  spaceUrl?: string;
}

export interface PartInfoV2 {
  pageNumber: number;
  cid?: number;
  title: string;
  durationSeconds?: number;
  videoStreams: StreamInfo[];
  audioStreams: StreamInfo[];
}

export interface ParseResultV2 {
  schemaVersion: 2;
  id: string;
  input: string;
  contentKind: ContentKind;
  aid?: number;
  bvid?: string;
  title: string;
  description?: string;
  owner?: OwnerInfo;
  publishTime?: string;
  durationSeconds?: number;
  coverUrl?: string;
  parts: PartInfoV2[];
  metadataSource: "bilibiliAndBbdown" | "bbdownOnly";
  warnings: string[];
  errorMessage?: string;
}

export interface AccountInfo {
  isLoggedIn: boolean;
  mid?: number;
  name?: string;
  avatarUrl?: string;
  vipLabel?: string;
  source: "manualCookie" | "bbdownScan" | "none" | string;
}

export interface ToolDetectionResult {
  bbdownFound: boolean;
  ffmpegFound: boolean;
  mp4boxFound: boolean;
  aria2cFound: boolean;
  bbdownVersion?: string;
  messages: string[];
}

export interface CommandPreview {
  executable: string;
  args: string[];
  display: string;
}

export interface CommandRunResult {
  success: boolean;
  exitCode?: number;
  output: string;
}

export interface DownloadTask {
  id: string;
  input: string;
  title: string;
  status: TaskStatus;
  command: CommandPreview;
  createdAt: string;
  output?: string;
}

export type BackendTaskKind = "parse" | "download" | "loginWeb" | "loginTv";
export type BackendTaskStatus = "queued" | "running" | "stopping" | "completed" | "failed" | "canceled";
export type BackendTaskPhase =
  | "preparing"
  | "loggingIn"
  | "loadingCookie"
  | "resolvingAid"
  | "fetchingVideoInfo"
  | "parsingPart"
  | "downloadingPart"
  | "finalizing"
  | "finished";

export interface TaskSnapshot {
  id: string;
  kind: BackendTaskKind;
  status: BackendTaskStatus;
  phase: BackendTaskPhase;
  input: string;
  title?: string;
  currentPartIndex?: number;
  totalParts?: number;
  currentPartTitle?: string;
  latestMessage?: string;
  startedAt?: string;
  finishedAt?: string;
  exitCode?: number;
}

export interface TaskLogEvent {
  taskId: string;
  stream: "stdout" | "stderr" | "system";
  line: string;
  timestamp: string;
}

export interface LoginQrEvent {
  taskId: string;
  mode: "web" | "tv" | string;
  imagePath: string;
  dataPath: string;
  timestamp: string;
}
