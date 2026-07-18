import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AccountInfo,
  AppConfig,
  CommandPreview,
  CommandRunResult,
  DownloadRequest,
  LoginQrEvent,
  ParseRequest,
  ParseResult,
  ParseResultV2,
  TaskLogEvent,
  TaskSnapshot,
  ToolDetectionResult,
} from "../types";

const tauriReady = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invokeOrFallback<T>(
  command: string,
  args: Record<string, unknown>,
  fallback: T,
): Promise<T> {
  if (!tauriReady()) {
    return fallback;
  }

  return invoke<T>(command, args);
}

export const fallbackConfig: AppConfig = {
  tools: {
    bbdownPath: "../bin/BBDown.exe",
    ffmpegPath: "../bin/ffmpeg.exe",
    mp4boxPath: "",
    aria2cPath: "",
  },
  workDir: "../download",
  auth: {
    apiMode: "WEB",
    cookie: "",
    accessToken: "",
    userAgent: "",
  },
  defaultOptions: {
    media: {
      video: true,
      audio: true,
      danmaku: true,
      subtitle: true,
      cover: true,
    },
    pageSelection: "1",
    codecPriority: ["avc", "hevc", "av1"],
    dfnPriority: ["4K 超清", "1080P 高码率", "1080P 高清"],
    mux: true,
    skipAiSubtitle: true,
    multiThread: true,
  },
  advanced: {
    forceHttp: false,
    useAria2c: false,
    useMp4box: false,
    allowPcdn: false,
    videoAscending: false,
    audioAscending: false,
    filePattern: "",
    multiFilePattern: "",
    language: "",
    delayPerPage: 0,
    aria2cArgs: "",
  },
};

export function getConfig() {
  return invokeOrFallback<AppConfig>("get_config", {}, fallbackConfig);
}

export function saveConfig(config: AppConfig) {
  return invokeOrFallback<AppConfig>("save_config", { config }, config);
}

export function detectTools(config: AppConfig) {
  return invokeOrFallback<ToolDetectionResult>(
    "detect_tools",
    { config },
    {
      bbdownFound: false,
      ffmpegFound: false,
      mp4boxFound: false,
      aria2cFound: false,
      messages: ["Tauri 后端未运行"],
    },
  );
}

export function getAccountInfo(config: AppConfig) {
  return invokeOrFallback<AccountInfo>("get_account_info", { config }, {
    isLoggedIn: false,
    source: "none",
  });
}

export function logoutAccount(config: AppConfig) {
  const loggedOutConfig: AppConfig = {
    ...config,
    auth: { ...config.auth, cookie: "", accessToken: "" },
  };
  return invokeOrFallback<AppConfig>("logout_account", { config }, loggedOutConfig);
}

export function openDownloadDirectory(path: string) {
  return invokeOrFallback<void>("open_download_directory", { path }, undefined);
}

export function openBilibiliLink(url: string) {
  return invokeOrFallback<void>("open_bilibili_link", { url }, undefined);
}

export function openToolDownloadPage(tool: "bbdown" | "ffmpeg") {
  return invokeOrFallback<void>("open_tool_download_page", { tool }, undefined);
}

export function buildPreviewCommand(request: ParseRequest) {
  return invokeOrFallback<CommandPreview>(
    "build_preview_command",
    { request },
    {
      executable: request.config.tools.bbdownPath,
      args: [request.input, "-info", "--show-all", "-p", "ALL"],
      display: `${request.config.tools.bbdownPath} ${request.input} -info --show-all -p ALL`,
    },
  );
}

export function buildDownloadPreview(request: DownloadRequest) {
  return invokeOrFallback<CommandPreview>(
    "build_download_preview",
    { request },
    {
      executable: request.config.tools.bbdownPath,
      args: [request.input, "--work-dir", request.config.workDir],
      display: `${request.config.tools.bbdownPath} ${request.input} --work-dir ${request.config.workDir}`,
    },
  );
}

export function listTasks() {
  return invokeOrFallback<TaskSnapshot[]>("list_tasks", {}, []);
}

export function stopTask(taskId: string) {
  return invokeOrFallback<TaskSnapshot>(
    "stop_task",
    { taskId },
    {
      id: taskId,
      kind: "download",
      status: "stopping",
      phase: "finalizing",
      input: "",
      latestMessage: "正在停止任务...",
    },
  );
}

export function runLogin(config: AppConfig, mode: "web" | "tv") {
  return invokeOrFallback<CommandRunResult>(
    "run_login",
    { config, mode },
    {
      success: false,
      output: "当前运行在浏览器预览模式，未调用 BBDown。",
    },
  );
}

export function runDownload(request: DownloadRequest) {
  return invokeOrFallback<CommandRunResult>(
    "run_download",
    { request },
    {
      success: false,
      output: "当前运行在浏览器预览模式，未调用 BBDown。",
    },
  );
}

export function parseVideo(request: ParseRequest) {
  return invokeOrFallback<ParseResult>("parse_video", { request }, {
    id: crypto.randomUUID(),
    input: request.input,
    contentKind: "unknown",
    title: "等待 Tauri 后端",
    parts: [],
    rawOutput: "",
    warnings: ["当前运行在浏览器预览模式，未调用 BBDown。"],
  });
}

export function parseVideoV2(request: ParseRequest) {
  return invokeOrFallback<ParseResultV2>("parse_video_v2", { request }, {
    schemaVersion: 2,
    id: crypto.randomUUID(),
    input: request.input,
    contentKind: "singleVideo",
    aid: 116928672695184,
    bvid: "BV1btKG6PEKU",
    title: "世 纪 辐 射【上】",
    description: "这里显示视频简介。桌面应用运行时由 B 站元数据接口返回完整内容。",
    owner: {
      mid: 39627524,
      name: "食贫道",
      faceUrl: "https://i2.hdslb.com/bfs/face/eef12b850c10b0d7a7929236abb08604955823c0.jpg",
      spaceUrl: "https://space.bilibili.com/39627524",
    },
    durationSeconds: 7444,
    coverUrl: "https://i1.hdslb.com/bfs/archive/5f9e31ab22f5014f9e51970f0bf857124ecc5735.jpg",
    parts: [{
      pageNumber: 1,
      cid: 40002191561,
      title: "世 纪 辐 射【上】",
      durationSeconds: 7444,
      videoStreams: [{
        kind: "video",
        rawText: "[4K 超清] [3840x2160] [HEVC] [25.000] [3773 kbps] [~3.35 GB]",
        qualityLabel: "4K 超清",
        resolution: "3840x2160",
        codec: "HEVC",
        badges: ["4k"],
      }],
      audioStreams: [{
        kind: "audio",
        rawText: "[M4A] [157 kbps] [~142.66 MB]",
        bitrate: "157 kbps",
        badges: ["normal"],
      }],
    }],
    metadataSource: "bilibiliAndBbdown",
    warnings: [],
  });
}

export function listenTaskStatus(handler: (payload: TaskSnapshot) => void): Promise<UnlistenFn> {
  if (!tauriReady()) {
    return Promise.resolve(() => undefined);
  }

  return listen<TaskSnapshot>("task-status", (event) => {
    handler(event.payload);
  });
}

export function listenTaskLog(handler: (payload: TaskLogEvent) => void): Promise<UnlistenFn> {
  if (!tauriReady()) {
    return Promise.resolve(() => undefined);
  }

  return listen<TaskLogEvent>("task-log", (event) => {
    handler(event.payload);
  });
}

export function listenParseResult(handler: (payload: ParseResult) => void): Promise<UnlistenFn> {
  if (!tauriReady()) {
    return Promise.resolve(() => undefined);
  }

  return listen<ParseResult>("task-parse-result", (event) => {
    handler(event.payload);
  });
}

export function listenLoginQr(handler: (payload: LoginQrEvent) => void): Promise<UnlistenFn> {
  if (!tauriReady()) {
    return Promise.resolve(() => undefined);
  }

  return listen<LoginQrEvent>("login-qr", (event) => {
    handler(event.payload);
  });
}

export function localFileSrc(path?: string) {
  if (!path) {
    return undefined;
  }

  if (!tauriReady()) {
    return path;
  }

  return convertFileSrc(path);
}
