use super::{
    client::{network_error, BiliClient, Content, ContentPart},
    engine::TaskContext,
    streams::{parse_chapters, parse_play_response, select_audio, select_video, Chapter, Track},
    transfer::{self, StagingFile},
};
use crate::{
    core::{AppConfig, DownloadRequest},
    task::TaskPhase,
    tools::resolve_ffmpeg,
    utils::{decode_output, hide_console_window},
};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::{fs, process::Command};

pub async fn run(request: &DownloadRequest, context: &TaskContext) -> Result<String, String> {
    let config = &request.config;
    let media = &config.default_options.media;
    if ![
        media.video,
        media.audio,
        media.subtitle,
        media.cover,
        media.danmaku,
    ]
    .into_iter()
    .any(|v| v)
    {
        return Err("请至少选择一种下载内容".to_string());
    }
    let mut client = BiliClient::new(config)?;
    client.context = Some(context.clone());
    context.report(
        TaskPhase::FetchingVideoInfo,
        "正在获取视频信息和最新下载地址",
    )?;
    let root = std::path::absolute(&config.work_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let cache_root = context
        .manager
        .cache_root(&context.id)
        .unwrap_or_else(|| root.join(".bbdown-next-cache").join(&context.id));
    let mut session = match super::session::Session::load(
        cache_root.clone(),
        root.clone(),
        context.id.clone(),
    )? {
        Some(session) => session,
        None => super::session::Session::create(
            cache_root,
            root.clone(),
            context.id.clone(),
            client.resolve(&request.input).await?,
            &config.default_options.page_selection,
        )?,
    };
    let content = session.content().clone();
    let selected = session.selection();
    let selected_count = selected.len() as u32;
    context.completed_part(0, selected_count, &[])?;
    for warning in &content.warnings {
        client.report(format!("列表提示: {warning}"))?;
    }
    let mut outputs = vec![];
    let mut completed = 0;
    for (selection_index, number) in selected.into_iter().enumerate() {
        context.checkpoint()?;
        if selection_index > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(
                config.advanced.delay_per_page.unwrap_or(0).into(),
            ))
            .await;
        }
        let part = content
            .parts
            .iter()
            .find(|p| p.number == number)
            .ok_or("分 P 不存在")?;
        context.part(&content, part.number, &part.title)?;
        let key = super::session::Session::key(part.aid, part.cid, part.episode_id);
        if let Some(files) = session.completed(&key) {
            completed += 1;
            context.completed_part(completed, selected_count, &files)?;
            outputs.extend(files);
            continue;
        }
        if let Some(files) = session.ready(&key) {
            let base = session.cache_base(&key)?;
            let committed = session.commit(&key, &base, &files, context)?;
            completed += 1;
            context.completed_part(completed, selected_count, &committed)?;
            outputs.extend(committed);
            continue;
        }
        let source_content = part.source_content(&content);
        let mut streams = if media.video || media.audio {
            context.report(
                TaskPhase::ParsingPart,
                format!("正在获取 P{} 音视频流", part.number),
            )?;
            Some(parse_play_response(
                &client.play(part).await?,
                part.duration,
            )?)
        } else {
            None
        };
        if config.default_options.mux
            && media.video
            && !matches!(client.mode, crate::core::ApiMode::Intl)
        {
            if let Some(streams) = streams
                .as_mut()
                .filter(|streams| streams.chapters.is_empty())
            {
                let params = [("aid", part.aid.to_string()), ("cid", part.cid.to_string())];
                if let Ok(value) = client.signed_api("/x/player/wbi/v2", &params).await {
                    if value["code"].as_i64() == Some(0) {
                        streams.chapters = parse_chapters(&value["data"], part.duration);
                    }
                }
            }
        }
        let video = streams
            .as_ref()
            .and_then(|s| select_video(&s.video, config));
        let audio = streams
            .as_ref()
            .and_then(|s| select_audio(&s.audio, config));
        if let Some(streams) = &streams {
            let needs_ffmpeg = config.default_options.mux
                || !(streams.progressive.is_empty() || media.video && media.audio);
            if needs_ffmpeg && resolve_ffmpeg(&config.tools).is_none() {
                return Err("未找到 FFmpeg，请在设置中配置路径".into());
            }
            if streams.progressive.is_empty() {
                if media.video && video.is_none() {
                    return Err("当前分 P 没有可用视频流".into());
                }
                if media.audio && audio.is_none() {
                    return Err("当前分 P 没有可用音频流，请取消音频选择后重试".into());
                }
            }
        }
        let requested_base = root.join(render_pattern(&content, part, config, video, audio)?);
        let destination = session.allocate(&key, &requested_base)?;
        let base = session.cache_base(&key)?;
        if let Some(parent) = base.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("创建输出目录失败: {e}"))?;
        }
        let archive_key = format!("{}:{}:{}", part.aid, part.cid, archive_signature(config));
        if config.advanced.save_archives_to_file == Some(true)
            && archive_contains(&root, &archive_key).await?
        {
            context.report(
                TaskPhase::DownloadingPart,
                format!("P{} 已下载，跳过", part.number),
            )?;
            completed += 1;
            context.completed_part(completed, selected_count, &[])?;
            continue;
        }
        let _destination = destination;
        context.report(
            TaskPhase::DownloadingPart,
            format!("开始下载 P{}: {}", part.number, part.title),
        )?;
        let output_start = outputs.len();
        let subtitles = if media.subtitle {
            download_subtitles(&mut client, part, &base, config, context).await?
        } else {
            vec![]
        };
        outputs.extend(subtitles.iter().map(|(path, _)| path.clone()));
        if media.cover && !source_content.cover.is_empty() {
            context.report(TaskPhase::DownloadingPart, "正在下载封面")?;
            let extension = image_extension(&source_content.cover);
            let path = suffix(&base, &format!(".{extension}"));
            transfer::download(
                &client.http,
                std::slice::from_ref(&source_content.cover),
                &path,
                false,
                context,
            )
            .await?;
            outputs.push(path);
        }
        if media.danmaku && part.cid > 0 {
            context.report(TaskPhase::DownloadingPart, "正在下载弹幕")?;
            let path = suffix(&base, ".danmaku.xml");
            download_danmaku(&client, part.cid, &path, context).await?;
            outputs.push(path);
        }
        if let Some(streams) = &streams {
            if !streams.progressive.is_empty() {
                if !config.default_options.mux && media.video && media.audio {
                    for (index, urls) in streams.progressive.iter().enumerate() {
                        let path = suffix(&base, &format!(".segment{}.flv", index + 1));
                        transfer::download(
                            &client.http,
                            &media_urls(urls, config)?,
                            &path,
                            config.default_options.multi_thread,
                            context,
                        )
                        .await?;
                        outputs.push(path);
                    }
                } else {
                    let ffmpeg = resolve_ffmpeg(&config.tools)
                        .ok_or("需要 FFmpeg 来处理合流视频，请在设置中配置路径")?;
                    let mut segments = vec![];
                    for (index, urls) in streams.progressive.iter().enumerate() {
                        let path = suffix(&base, &format!(".segment{}.flv", index + 1));
                        transfer::download(
                            &client.http,
                            &media_urls(urls, config)?,
                            &path,
                            config.default_options.multi_thread,
                            context,
                        )
                        .await?;
                        segments.push(path);
                    }
                    let output = suffix(&base, if media.video { ".mp4" } else { ".m4a" });
                    mux(
                        &ffmpeg,
                        config,
                        &source_content,
                        &segments,
                        None,
                        &subtitles,
                        &streams.chapters,
                        &output,
                        true,
                        context,
                    )
                    .await?;
                    outputs.push(output);
                }
            } else {
                if media.video && video.is_none() {
                    return Err("当前分 P 没有可用视频流".to_string());
                }
                if media.audio && audio.is_none() {
                    return Err("当前分 P 没有可用音频流，请取消音频选择后重试".to_string());
                }
                let ffmpeg = if config.default_options.mux {
                    Some(resolve_ffmpeg(&config.tools).ok_or("未找到 FFmpeg，请在设置中配置路径")?)
                } else {
                    None
                };
                let mut video_paths = vec![];
                let mut audio_path = None;
                if media.video {
                    let track = video.unwrap();
                    let path = suffix(&base, ".video.m4s");
                    transfer::download(
                        &client.http,
                        &media_urls(&track.urls, config)?,
                        &path,
                        config.default_options.multi_thread,
                        context,
                    )
                    .await?;
                    video_paths.push(path);
                }
                if media.audio {
                    let track = audio.unwrap();
                    let path = suffix(&base, ".audio.m4s");
                    transfer::download(
                        &client.http,
                        &media_urls(&track.urls, config)?,
                        &path,
                        config.default_options.multi_thread,
                        context,
                    )
                    .await?;
                    audio_path = Some(path);
                }
                if let Some(ffmpeg) = ffmpeg {
                    let output = suffix(
                        &base,
                        if media.video {
                            ".mp4"
                        } else if audio.is_some_and(|t| t.codec == "FLAC") {
                            ".flac"
                        } else {
                            ".m4a"
                        },
                    );
                    mux(
                        &ffmpeg,
                        config,
                        &source_content,
                        &video_paths,
                        audio_path.as_deref(),
                        &subtitles,
                        &streams.chapters,
                        &output,
                        false,
                        context,
                    )
                    .await?;
                    outputs.push(output);
                } else {
                    outputs.extend(video_paths);
                    outputs.extend(audio_path);
                }
            }
        }
        session.prepare(&key, &outputs[output_start..])?;
        context.checkpoint()?;
        let files = session.commit(&key, &base, &outputs[output_start..], context)?;
        outputs.truncate(output_start);
        outputs.extend(files.clone());
        completed += 1;
        context.completed_part(completed, selected_count, &files)?;
        if config.advanced.save_archives_to_file == Some(true) {
            save_archive(&root, archive_key, &outputs[output_start..]).await?;
        }
    }
    Ok(format!(
        "下载完成，共完成 {completed} 个分 P，文件保存在 {}",
        root.display()
    ))
}

#[allow(clippy::too_many_arguments)]
async fn mux(
    ffmpeg: &Path,
    config: &AppConfig,
    content: &Content,
    video: &[PathBuf],
    audio: Option<&Path>,
    subtitles: &[(PathBuf, String)],
    chapters: &[Chapter],
    output: &Path,
    progressive: bool,
    context: &TaskContext,
) -> Result<(), String> {
    if output.exists() {
        return Ok(());
    }
    context.report(TaskPhase::Finalizing, "正在使用 FFmpeg 合并媒体")?;
    let media = &config.default_options.media;
    let stage = StagingFile(suffix(output, &format!(".{}.part", context.id)));
    for attempt in 0..10 {
        match std::fs::remove_file(&stage.0) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) if attempt < 9 => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await
            }
            Err(e) => return Err(format!("FFmpeg 临时文件尚未释放，请稍后重试: {e}")),
        }
    }
    let mut args = vec![
        "-hide_banner".to_string(),
        "-nostdin".to_string(),
        "-n".to_string(),
    ];
    let mut inputs = 0;
    let concat_path = StagingFile(suffix(output, &format!(".{}.concat", context.id)));
    if progressive && video.len() > 1 {
        // Concat names are generated local files; escape FFmpeg's list grammar.
        let entries = video
            .iter()
            .map(|path| {
                let absolute =
                    std::fs::canonicalize(path).map_err(|e| format!("读取媒体路径失败: {e}"))?;
                let name = absolute
                    .to_string_lossy()
                    .trim_start_matches(r"\\?\")
                    .replace('\\', "/")
                    .replace('\'', "'\\''");
                Ok(format!("file '{name}'\n"))
            })
            .collect::<Result<Vec<_>, String>>()?
            .join("");
        fs::write(&concat_path.0, entries)
            .await
            .map_err(|e| format!("写入合并列表失败: {e}"))?;
        args.extend([
            "-f".into(),
            "concat".into(),
            "-safe".into(),
            "0".into(),
            "-i".into(),
            concat_path.0.to_string_lossy().into(),
        ]);
        inputs += 1;
    } else if let Some(path) = video.first() {
        args.extend(["-i".into(), path.to_string_lossy().into()]);
        inputs += 1;
    }
    let video_index = 0;
    let audio_index = if let Some(path) = audio {
        let index = inputs;
        args.extend(["-i".into(), path.to_string_lossy().into()]);
        inputs += 1;
        index
    } else {
        0
    };
    // FLAC has no subtitle tracks; SRT sidecars are still kept.
    let include_subtitles = output.extension().and_then(|s| s.to_str()) != Some("flac");
    if include_subtitles {
        for (path, _) in subtitles {
            args.extend(["-i".into(), path.to_string_lossy().into()]);
        }
    }
    let chapter_path = StagingFile(suffix(output, &format!(".{}.chapters", context.id)));
    if !chapters.is_empty() && media.video {
        fs::write(&chapter_path.0, chapter_metadata(chapters))
            .await
            .map_err(|e| format!("保存章节信息失败: {e}"))?;
        let chapter_index = inputs
            + if include_subtitles {
                subtitles.len()
            } else {
                0
            };
        args.extend([
            "-f".into(),
            "ffmetadata".into(),
            "-i".into(),
            chapter_path.0.to_string_lossy().into(),
            "-map_chapters".into(),
            chapter_index.to_string(),
        ]);
    }
    if media.video {
        args.extend(["-map".into(), format!("{video_index}:v:0")]);
    }
    if media.audio {
        args.extend(["-map".into(), format!("{audio_index}:a:0")]);
    }
    if include_subtitles {
        for (index, (_, language)) in subtitles.iter().enumerate() {
            args.extend([
                "-map".into(),
                format!("{}:0", inputs + index),
                format!("-metadata:s:s:{index}"),
                format!("language={language}"),
            ]);
        }
    }
    args.extend(["-c".into(), "copy".into()]);
    if include_subtitles && !subtitles.is_empty() {
        args.extend(["-c:s".into(), "mov_text".into()]);
    }
    if let Some(language) = config
        .advanced
        .language
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        args.extend(["-metadata:s:a:0".into(), format!("language={language}")]);
    }
    args.extend([
        "-metadata".into(),
        format!("title={}", content.title),
        "-metadata".into(),
        format!("comment={}", content.description),
    ]);
    if let Some(owner) = &content.owner {
        args.extend([
            "-metadata".into(),
            format!("artist={}", owner.name.as_deref().unwrap_or("")),
        ]);
    }
    let format = if output.extension().and_then(|s| s.to_str()) == Some("flac") {
        "flac"
    } else {
        "mp4"
    };
    if format == "mp4" {
        args.extend([
            "-movflags".into(),
            "+faststart".into(),
            "-strict".into(),
            "-2".into(),
        ]);
    }
    args.extend(["-f".into(), format.into(), stage.0.to_string_lossy().into()]);
    let mut command = Command::new(ffmpeg);
    command
        .args(&args)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    hide_console_window(&mut command);
    let result = command
        .output()
        .await
        .map_err(|e| format!("启动 FFmpeg 失败: {e}"))?;
    context.checkpoint()?;
    if !result.status.success() {
        return Err(format!(
            "FFmpeg 合并失败，原始媒体已保留: {}",
            decode_output(&[], &result.stderr)
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    transfer::publish(&stage.0, output).map_err(|e| format!("保存合并文件失败: {e}"))?;
    Ok(())
}

async fn download_danmaku(
    client: &BiliClient,
    cid: u64,
    path: &Path,
    context: &TaskContext,
) -> Result<(), String> {
    let response = client
        .http
        .get(format!("https://comment.bilibili.com/{cid}.xml"))
        .header(reqwest::header::REFERER, "https://www.bilibili.com/")
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    let encoding = response
        .headers()
        .get(reqwest::header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let raw = response.bytes().await.map_err(network_error)?;
    let xml = decode_danmaku(&raw, &encoding)?;
    write_new(path, &xml, context).await
}

fn chapter_metadata(chapters: &[Chapter]) -> String {
    let escape = |value: &str| {
        value
            .replace('\\', "\\\\")
            .replace('=', "\\=")
            .replace(';', "\\;")
            .replace('#', "\\#")
            .replace('\r', "")
            .replace('\n', "\\\n")
    };
    let mut output = ";FFMETADATA1\n".to_string();
    for chapter in chapters {
        output.push_str(&format!(
            "[CHAPTER]\nTIMEBASE=1/1000\nSTART={}\nEND={}\ntitle={}\n",
            chapter.start_ms,
            chapter.end_ms,
            escape(&chapter.title)
        ));
    }
    output
}

fn decode_danmaku(raw: &[u8], encoding: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut decoded = Vec::new();
    if encoding.eq_ignore_ascii_case("deflate") {
        // comment.bilibili.com uses RFC 1951 raw DEFLATE; some hosts use zlib.
        if flate2::read::DeflateDecoder::new(raw)
            .read_to_end(&mut decoded)
            .is_err()
        {
            decoded.clear();
            flate2::read::ZlibDecoder::new(raw)
                .read_to_end(&mut decoded)
                .map_err(|e| format!("解压弹幕失败: {e}"))?;
        }
    } else {
        decoded.extend_from_slice(raw);
    }
    let text = std::str::from_utf8(&decoded).map_err(|e| format!("弹幕不是 UTF-8 XML: {e}"))?;
    if !(text.trim_start().starts_with("<?xml") || text.trim_start().starts_with("<i>")) {
        return Err("弹幕接口没有返回 XML 内容".into());
    }
    Ok(decoded)
}

async fn download_subtitles(
    client: &mut BiliClient,
    part: &ContentPart,
    base: &Path,
    config: &AppConfig,
    context: &TaskContext,
) -> Result<Vec<(PathBuf, String)>, String> {
    context.report(TaskPhase::DownloadingPart, "正在获取字幕列表")?;
    if let Some(parent) = base.parent() {
        let prefix = format!(
            "{}.",
            base.file_name().unwrap_or_default().to_string_lossy()
        );
        let mut cached = std::fs::read_dir(parent)
            .map_err(|e| e.to_string())?
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let name = path.file_name()?.to_string_lossy();
                if !name.starts_with(&prefix)
                    || !matches!(path.extension()?.to_str()?, "srt" | "ass")
                {
                    return None;
                }
                let language = name.strip_prefix(&prefix)?.split('.').next()?.to_string();
                Some((path, language))
            })
            .collect::<Vec<_>>();
        cached.sort_by(|a, b| a.0.cmp(&b.0));
        if !cached.is_empty() {
            return Ok(cached);
        }
    }
    let sources = super::subtitles::lookup(client, part).await?;
    let mut saved = vec![];
    for (index, subtitle) in sources.into_iter().enumerate() {
        if config.default_options.skip_ai_subtitle && subtitle.ai {
            continue;
        }
        let extension = reqwest::Url::parse(&subtitle.url)
            .ok()
            .map(|url| {
                if url.path().ends_with(".ass") {
                    "ass"
                } else if url.path().ends_with(".srt") {
                    "srt"
                } else {
                    "json"
                }
            })
            .unwrap_or("json");
        let response = client
            .http
            .get(&subtitle.url)
            .send()
            .await
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?;
        let bytes = response.bytes().await.map_err(network_error)?;
        let (contents, saved_extension) = if extension == "json" {
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|e| format!("解析字幕失败: {e}"))?;
            (subtitle_to_srt(&value)?, "srt")
        } else {
            (
                String::from_utf8(bytes.to_vec()).map_err(|e| format!("字幕不是 UTF-8: {e}"))?,
                extension,
            )
        };
        let path = suffix(
            base,
            &format!(
                ".{}.{}.{saved_extension}",
                sanitize_segment(&subtitle.language),
                index + 1
            ),
        );
        write_new(&path, contents.as_bytes(), context).await?;
        saved.push((path, subtitle.language));
    }
    if saved.is_empty() {
        context.report(TaskPhase::DownloadingPart, "所选接口没有可用字幕")?;
    }
    Ok(saved)
}

pub(crate) fn subtitle_to_srt(value: &Value) -> Result<String, String> {
    let body = value["body"].as_array().ok_or("字幕响应缺少 body")?;
    let mut result = String::new();
    for (index, cue) in body.iter().enumerate() {
        let start = cue["from"]
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or("字幕开始时间无效")?;
        let end = cue["to"]
            .as_f64()
            .filter(|n| n.is_finite() && *n >= start)
            .ok_or("字幕结束时间无效")?;
        let text = cue["content"].as_str().ok_or("字幕内容无效")?;
        result.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            index + 1,
            srt_time(start),
            srt_time(end),
            text.replace('\r', "").trim()
        ));
    }
    Ok(result)
}
fn srt_time(seconds: f64) -> String {
    let millis = (seconds * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        millis / 3600000,
        millis / 60000 % 60,
        millis / 1000 % 60,
        millis % 1000
    )
}

pub fn suffix(base: &Path, ending: &str) -> PathBuf {
    PathBuf::from(format!("{}{ending}", base.display()))
}

#[cfg(test)]
async fn reserve_output(requested: &Path) -> Result<(PathBuf, StagingFile), String> {
    let parent = requested.parent().ok_or("输出路径无效")?;
    let stem = requested
        .file_name()
        .ok_or("输出文件名无效")?
        .to_string_lossy();
    for index in 1..1000 {
        let name = if index == 1 {
            stem.to_string()
        } else {
            format!("{stem} ({index})")
        };
        let lock_path = parent.join(format!(".{name}.lock"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .await
        {
            Ok(file) => drop(file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("创建输出文件锁失败: {e}")),
        }
        let lock = StagingFile(lock_path);
        let mut entries = fs::read_dir(parent)
            .await
            .map_err(|e| format!("读取输出目录失败: {e}"))?;
        let mut exists = false;
        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let entry_name = entry.file_name().to_string_lossy().into_owned();
            if entry_name == name || entry_name.starts_with(&format!("{name}.")) {
                exists = true;
                break;
            }
        }
        if !exists {
            return Ok((parent.join(name), lock));
        }
    }
    Err("同名输出过多，请修改文件名模板".into())
}
fn image_extension(url: &str) -> &str {
    let path = url.split('?').next().unwrap_or(url);
    match path.rsplit('.').next().unwrap_or("") {
        "png" => "png",
        "webp" => "webp",
        _ => "jpg",
    }
}

pub fn sanitize_segment(value: &str) -> String {
    let value = value
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                ch
            }
        })
        .take(100)
        .collect::<String>();
    let value = value.trim().trim_matches('.');
    let reserved = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    if value.is_empty() {
        "untitled".into()
    } else if matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (reserved.len() == 4
            && (reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && reserved.as_bytes()[3].is_ascii_digit())
    {
        format!("_{value}")
    } else {
        value.to_string()
    }
}

fn render_pattern(
    content: &Content,
    part: &ContentPart,
    config: &AppConfig,
    video: Option<&Track>,
    audio: Option<&Track>,
) -> Result<PathBuf, String> {
    let multi = content.parts.len() > 1;
    let default = if multi {
        "<videoTitle>/[P<pageNumberWithZero>]<pageTitle>"
    } else {
        "<videoTitle>"
    };
    let pattern = if multi {
        config.advanced.multi_file_pattern.as_deref()
    } else {
        config.advanced.file_pattern.as_deref()
    }
    .filter(|s| !s.trim().is_empty())
    .unwrap_or(default);
    let owner = part
        .source
        .as_ref()
        .and_then(|source| source.owner.as_ref())
        .or(content.owner.as_ref());
    let tokens = [
        ("videoTitle", content.title.clone()),
        ("pageTitle", part.title.clone()),
        ("pageNumber", part.number.to_string()),
        (
            "pageNumberWithZero",
            format!(
                "{:0width$}",
                part.number,
                width = content.parts.len().to_string().len()
            ),
        ),
        ("bvid", part.bvid.clone()),
        ("aid", part.aid.to_string()),
        ("cid", part.cid.to_string()),
        (
            "dfn",
            video
                .and_then(|t| t.info.quality_label.clone())
                .unwrap_or_default(),
        ),
        (
            "res",
            video
                .and_then(|t| t.info.resolution.clone())
                .unwrap_or_default(),
        ),
        (
            "fps",
            video.and_then(|t| t.info.fps.clone()).unwrap_or_default(),
        ),
        (
            "videoCodecs",
            video.map(|t| t.codec.clone()).unwrap_or_default(),
        ),
        (
            "audioCodecs",
            audio.map(|t| t.codec.clone()).unwrap_or_default(),
        ),
        (
            "videoBandwidth",
            video
                .map(|t| (t.bandwidth / 1000).to_string())
                .unwrap_or_default(),
        ),
        (
            "audioBandwidth",
            audio
                .map(|t| (t.bandwidth / 1000).to_string())
                .unwrap_or_default(),
        ),
        (
            "ownerName",
            owner.and_then(|o| o.name.clone()).unwrap_or_default(),
        ),
        (
            "ownerMid",
            owner
                .and_then(|o| o.mid)
                .map(|id| id.to_string())
                .unwrap_or_default(),
        ),
        (
            "publishDate",
            part.source
                .as_ref()
                .and_then(|source| source.publish_time.clone())
                .or_else(|| content.publish_time.clone())
                .unwrap_or_default(),
        ),
        (
            "videoDate",
            part.source
                .as_ref()
                .and_then(|source| source.publish_time.clone())
                .or_else(|| content.publish_time.clone())
                .unwrap_or_default(),
        ),
        (
            "apiType",
            format!("{:?}", config.auth.api_mode).to_ascii_uppercase(),
        ),
    ];
    let mut rendered = pattern.replace('\\', "/");
    if rendered.starts_with('/')
        || rendered.contains(':')
        || rendered.split('/').any(|s| matches!(s.trim(), "." | ".."))
    {
        return Err("文件名模板必须是下载目录内的相对路径，不能包含 .. 或盘符".into());
    }
    for (token, value) in tokens {
        rendered = rendered.replace(&format!("<{token}>"), &sanitize_segment(&value));
    }
    if rendered.contains('<') || rendered.contains('>') {
        return Err("文件名模板包含未知变量".into());
    }
    Ok(rendered.split('/').map(sanitize_segment).collect())
}

fn media_urls(urls: &[String], config: &AppConfig) -> Result<Vec<String>, String> {
    let mut urls = urls.to_vec();
    if !config.advanced.allow_pcdn {
        urls.sort_by_key(|url| url.contains("mcdn.bilivideo") || url.contains("szbdyd.com"));
    }
    urls.iter()
        .map(|url| {
            let mut url = reqwest::Url::parse(url).map_err(|_| "无效媒体下载地址".to_string())?;
            if config.advanced.force_replace_host == Some(true) {
                if let Some(host) = config
                    .advanced
                    .upos_host
                    .as_deref()
                    .filter(|s| !s.is_empty())
                {
                    url.set_host(Some(host))
                        .map_err(|_| "无效 UPOS 主机名".to_string())?;
                }
            }
            if config.advanced.force_http {
                let _ = url.set_scheme("http");
            }
            Ok(url.to_string())
        })
        .collect()
}

async fn write_new(path: &Path, contents: &[u8], context: &TaskContext) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    let stage = StagingFile(suffix(path, &format!(".{}.part", context.id)));
    fs::write(&stage.0, contents)
        .await
        .map_err(|e| format!("写入文件失败: {e}"))?;
    context.checkpoint()?;
    transfer::publish(&stage.0, path)
        .map_err(|e| format!("保存文件失败 {}: {e}", path.display()))?;
    Ok(())
}

fn archive_signature(config: &AppConfig) -> String {
    let options = &config.default_options;
    let output_options = (
        &options.media,
        &options.codec_priority,
        &options.dfn_priority,
        options.mux,
        options.skip_ai_subtitle,
        &config.advanced,
        &config.auth.api_mode,
    );
    format!(
        "{:x}",
        md5::compute(serde_json::to_string(&output_options).unwrap_or_default())
    )
}

type Archives = HashMap<String, Vec<PathBuf>>;
async fn read_archives(root: &Path) -> Result<Archives, String> {
    match fs::read(root.join("download-archives.json")).await {
        Ok(raw) => serde_json::from_slice(&raw).map_err(|e| format!("读取下载记录失败: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(format!("读取下载记录失败: {e}")),
    }
}
async fn archive_contains(root: &Path, key: &str) -> Result<bool, String> {
    Ok(read_archives(root)
        .await?
        .get(key)
        .is_some_and(|paths| !paths.is_empty() && paths.iter().all(|p| p.is_file())))
}
async fn save_archive(root: &Path, key: String, paths: &[PathBuf]) -> Result<(), String> {
    // In-process serialization prevents concurrent downloads losing each other's records.
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;
    let mut archives = read_archives(root).await?;
    archives.insert(key, paths.to_vec());
    let raw = serde_json::to_vec_pretty(&archives).map_err(|e| e.to_string())?;
    let temp = StagingFile(root.join(format!(".archives-{}.part", uuid::Uuid::new_v4())));
    fs::write(&temp.0, raw)
        .await
        .map_err(|e| format!("保存下载记录失败: {e}"))?;
    fs::rename(&temp.0, root.join("download-archives.json"))
        .await
        .map_err(|e| format!("保存下载记录失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_identity_is_stable_when_list_selection_or_thread_count_changes() {
        let mut config = crate::core::default_config();
        let original = archive_signature(&config);
        config.default_options.page_selection = "3-5".into();
        config.default_options.multi_thread = false;
        assert_eq!(archive_signature(&config), original);
        config.auth.api_mode = crate::core::ApiMode::Tv;
        assert_ne!(archive_signature(&config), original);
    }

    #[test]
    fn decodes_raw_deflate_and_zlib_danmaku() {
        use std::io::Write;
        let xml = b"<?xml version=\"1.0\"?><i><d>test</d></i>";
        let mut raw =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        raw.write_all(xml).unwrap();
        assert_eq!(
            decode_danmaku(&raw.finish().unwrap(), "deflate").unwrap(),
            xml
        );
        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zlib.write_all(xml).unwrap();
        assert_eq!(
            decode_danmaku(&zlib.finish().unwrap(), "deflate").unwrap(),
            xml
        );
        assert!(decode_danmaku(b"<html>error</html>", "").is_err());
    }
    #[tokio::test]
    async fn output_reservations_avoid_existing_files_and_concurrent_writers() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let requested = directory.0.join("video");
        std::fs::write(suffix(&requested, ".mp4"), "keep").unwrap();
        let (first, lock) = reserve_output(&requested).await.unwrap();
        let (second, _lock) = reserve_output(&requested).await.unwrap();
        assert_eq!(first, directory.0.join("video (2)"));
        assert_eq!(second, directory.0.join("video (3)"));
        drop(lock);
        assert_eq!(
            std::fs::read_to_string(suffix(&requested, ".mp4")).unwrap(),
            "keep"
        );
    }

    #[tokio::test]
    #[ignore = "requires a local FFmpeg executable"]
    async fn ffmpeg_muxes_downloaded_streams_and_subtitles() {
        use crate::bilibili::test_support::{context, TestDirectory, TestServer};
        use std::sync::Arc;
        let directory = TestDirectory::new();
        let config = crate::core::default_config();
        let ffmpeg = resolve_ffmpeg(&config.tools).expect("FFmpeg must be available on PATH");
        let source = directory.0.join("source.mp4");
        let result = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:d=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&source)
            .output()
            .await
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            decode_output(&[], &result.stderr)
        );
        let data = Arc::new(fs::read(&source).await.unwrap());
        let server = TestServer::new(data).await;
        let raw = directory.0.join("下载 ' 测试.mp4");
        let task = context();
        transfer::download(
            &reqwest::Client::builder().no_proxy().build().unwrap(),
            &[format!("{}/data", server.url)],
            &raw,
            false,
            &task,
        )
        .await
        .unwrap();
        let subtitle = directory.0.join("字幕.srt");
        fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:00,900\n测试字幕\n")
            .await
            .unwrap();
        let content = Content {
            title: "测试 ' 标题".into(),
            description: "合并验证".into(),
            cover: String::new(),
            owner: None,
            publish_time: None,
            kind: crate::core::ContentKind::SingleVideo,
            parts: vec![],
            warnings: vec![],
        };
        let output = directory.0.join("合并 ' 文件.mp4");
        mux(
            &ffmpeg,
            &config,
            &content,
            &[raw],
            None,
            &[(subtitle, "zho".into())],
            &[Chapter {
                start_ms: 0,
                end_ms: 900,
                title: "fixture chapter;#".into(),
            }],
            &output,
            true,
            &task,
        )
        .await
        .unwrap();
        let result = Command::new(&ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args(["-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"])
            .output()
            .await
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            decode_output(&[], &result.stderr)
        );
        let result = Command::new(&ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args(["-map", "0:s:0", "-f", "srt", "-"])
            .output()
            .await
            .unwrap();
        assert!(result.status.success());
        assert!(String::from_utf8_lossy(&result.stdout).contains("测试字幕"));
        let info = Command::new(&ffmpeg)
            .args(["-hide_banner", "-i"])
            .arg(&output)
            .args(["-f", "null", "-"])
            .output()
            .await
            .unwrap();
        let info = decode_output(&[], &info.stderr);
        assert!(info.contains("Chapter") && info.contains("fixture chapter;#"));
    }

    #[tokio::test]
    #[ignore = "requires Bilibili network access and FFmpeg"]
    async fn live_download_smoke() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let mut config = crate::core::default_config();
        config.work_dir = directory.0.to_string_lossy().to_string();
        config.default_options.dfn_priority = vec!["360P 流畅".into()];
        let ffmpeg = resolve_ffmpeg(&config.tools).unwrap();
        let request = DownloadRequest {
            input: "BV17x411w7KC".into(),
            config,
            parse_id: None,
        };
        let manager = crate::task::TaskManager::new();
        let task = manager
            .enqueue(request.clone(), Some("live fixture".into()))
            .unwrap();
        manager.take_next().unwrap();
        let context = TaskContext {
            app: None,
            manager: manager.clone(),
            id: task.id.clone(),
        };
        let result = run(&request, &context).await;
        assert!(
            result.is_ok(),
            "{:?}; phase={:?}",
            result,
            context
                .manager
                .get_task(&context.id)
                .map(|t| t.latest_message)
        );
        assert!(manager.get_task(&task.id).unwrap().downloaded_bytes > 0);
        let prior = manager.get_task(&task.id).unwrap().output_files;
        run(&request, &context).await.unwrap();
        assert_eq!(manager.get_task(&task.id).unwrap().output_files, prior);
        let mut stack = vec![directory.0.clone()];
        let mut outputs = Vec::new();
        while let Some(folder) = stack.pop() {
            for entry in std::fs::read_dir(folder).unwrap().flatten() {
                if entry.path().is_dir() {
                    stack.push(entry.path());
                } else {
                    outputs.push(entry.path());
                }
            }
        }
        let output = outputs
            .iter()
            .find(|p| p.extension().is_some_and(|s| s == "mp4"))
            .expect("native download should publish MP4");
        assert!(outputs
            .iter()
            .any(|p| p.extension().is_some_and(|s| s == "xml")));
        assert!(outputs.iter().any(|p| p
            .extension()
            .is_some_and(|s| s == "jpg" || s == "png" || s == "webp")));
        let result = Command::new(&ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(output)
            .args(["-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"])
            .output()
            .await
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            decode_output(&[], &result.stderr)
        );
    }
    #[test]
    fn converts_subtitle_timestamps_and_multiline_text() {
        let value =
            serde_json::json!({"body":[{"from":0.25,"to":61.123,"content":"第一行\n第二行"}]});
        assert_eq!(
            subtitle_to_srt(&value).unwrap(),
            "1\n00:00:00,250 --> 00:01:01,123\n第一行\n第二行\n\n"
        );
        assert!(subtitle_to_srt(&serde_json::json!({"body":[{"from":2,"to":1}]})).is_err());
        assert_eq!(sanitize_segment("../CON"), "_CON");
        assert_eq!(sanitize_segment("NUL.txt"), "_NUL.txt");
    }
    #[test]
    fn template_cannot_escape_output_directory() {
        let mut config = crate::core::default_config();
        let part = ContentPart {
            number: 1,
            aid: 1,
            bvid: "BV1btKG6PEKU".into(),
            cid: 2,
            episode_id: None,
            title: "../hello".into(),
            duration: 1,
            cheese: false,
            source: None,
            watch_url: None,
        };
        let content = Content {
            title: "测试/视频".into(),
            description: String::new(),
            cover: String::new(),
            owner: None,
            publish_time: None,
            kind: crate::core::ContentKind::SingleVideo,
            parts: vec![part.clone()],
            warnings: vec![],
        };
        assert_eq!(
            render_pattern(&content, &part, &config, None, None).unwrap(),
            PathBuf::from("测试_视频")
        );
        for template in [
            "../<videoTitle>",
            "C:/<videoTitle>",
            "/<videoTitle>",
            "<unknown>",
        ] {
            config.advanced.file_pattern = Some(template.into());
            assert!(render_pattern(&content, &part, &config, None, None).is_err());
        }
    }
}
