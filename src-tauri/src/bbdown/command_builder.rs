use crate::core::{ApiMode, AuthConfig, CommandPreview, DownloadRequest, ParseRequest};

pub fn build_parse_args(request: &ParseRequest) -> Vec<String> {
    let mut args = vec![request.input.clone(), "-info".to_string()];

    if request.show_all_parts {
        args.push("--show-all".to_string());
    }

    args.extend(["-p".to_string(), "ALL".to_string()]);
    append_api_and_auth(&mut args, &request.config.auth);

    args
}

pub fn build_download_args(request: &DownloadRequest) -> Vec<String> {
    let config = &request.config;
    let options = &config.default_options;
    let advanced = &config.advanced;
    let media = &options.media;
    let mut args = vec![
        request.input.clone(),
        "--work-dir".to_string(),
        config.work_dir.clone(),
    ];

    append_optional_arg(
        &mut args,
        "--ffmpeg-path",
        config.tools.ffmpeg_path.as_deref(),
    );
    append_optional_arg(
        &mut args,
        "--mp4box-path",
        config.tools.mp4box_path.as_deref(),
    );
    append_optional_arg(
        &mut args,
        "--aria2c-path",
        config.tools.aria2c_path.as_deref(),
    );

    append_api_and_auth(&mut args, &config.auth);

    if media.video && !media.audio {
        args.push("--video-only".to_string());
    } else if !media.video && media.audio {
        args.push("--audio-only".to_string());
    } else if !media.video && !media.audio {
        if media.danmaku {
            args.push("--danmaku-only".to_string());
        } else if media.subtitle {
            args.push("--sub-only".to_string());
        } else if media.cover {
            args.push("--cover-only".to_string());
        }
    }

    append_optional_arg(&mut args, "-p", Some(options.page_selection.as_str()));

    if media.video || media.audio {
        if !media.subtitle {
            args.push("--skip-subtitle".to_string());
        }
        if !media.cover {
            args.push("--skip-cover".to_string());
        }
        if !options.mux {
            args.push("--skip-mux".to_string());
        }
        if media.danmaku {
            args.push("-dd".to_string());
        }
    }

    if !options.codec_priority.is_empty() {
        args.extend(["-e".to_string(), options.codec_priority.join(",")]);
    }
    if !options.dfn_priority.is_empty() {
        args.extend(["-q".to_string(), options.dfn_priority.join(", ")]);
    }
    if options.multi_thread {
        args.push("-mt".to_string());
    }
    if options.skip_ai_subtitle {
        args.push("--skip-ai".to_string());
    }
    if advanced.force_http {
        args.push("--force-http".to_string());
    }
    if advanced.use_aria2c {
        args.push("--use-aria2c".to_string());
    }
    append_optional_arg(&mut args, "--aria2c-args", advanced.aria2c_args.as_deref());
    if advanced.use_mp4box {
        args.push("--use-mp4box".to_string());
    }
    if advanced.allow_pcdn {
        args.push("--allow-pcdn".to_string());
    }
    if advanced.video_ascending {
        args.push("--video-ascending".to_string());
    }
    if advanced.audio_ascending {
        args.push("--audio-ascending".to_string());
    }
    append_optional_arg(&mut args, "-F", advanced.file_pattern.as_deref());
    append_optional_arg(&mut args, "-M", advanced.multi_file_pattern.as_deref());
    append_optional_arg(&mut args, "--language", advanced.language.as_deref());
    append_optional_arg(&mut args, "--upos-host", advanced.upos_host.as_deref());
    if advanced.force_replace_host.unwrap_or(false) {
        args.push("--force-replace-host".to_string());
    }
    if advanced.save_archives_to_file.unwrap_or(false) {
        args.push("--save-archives-to-file".to_string());
    }
    if let Some(delay) = advanced.delay_per_page {
        if delay > 0 {
            args.extend(["--delay-per-page".to_string(), delay.to_string()]);
        }
    }

    args
}

pub fn make_command_preview(executable: String, args: Vec<String>) -> CommandPreview {
    let display = std::iter::once(quote_for_display(&executable))
        .chain(quote_display_args(&args))
        .collect::<Vec<_>>()
        .join(" ");

    CommandPreview {
        executable,
        args,
        display,
    }
}

pub fn append_api_and_auth(args: &mut Vec<String>, auth: &AuthConfig) {
    append_api_mode(args, &auth.api_mode);
    append_optional_arg(args, "-c", auth.cookie.as_deref());
    append_optional_arg(args, "-token", auth.access_token.as_deref());
    append_optional_arg(args, "-ua", auth.user_agent.as_deref());
}

pub fn append_api_mode(args: &mut Vec<String>, api_mode: &ApiMode) {
    match api_mode {
        ApiMode::WEB => {}
        ApiMode::TV => args.push("-tv".to_string()),
        ApiMode::APP => args.push("-app".to_string()),
        ApiMode::INTL => args.push("-intl".to_string()),
    }
}

pub fn append_optional_arg(args: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        let value = value.trim();
        if !value.is_empty() {
            args.push(flag.to_string());
            args.push(value.to_string());
        }
    }
}

pub fn quote_display_args(args: &[String]) -> Vec<String> {
    let mut rendered = Vec::with_capacity(args.len());
    let mut redact_next = false;

    for arg in args {
        if redact_next {
            rendered.push("\"***\"".to_string());
            redact_next = false;
            continue;
        }

        rendered.push(quote_for_display(arg));
        if matches!(
            arg.as_str(),
            "-c" | "--cookie" | "-token" | "--access-token"
        ) {
            redact_next = true;
        }
    }

    rendered
}

pub fn quote_for_display(value: &str) -> String {
    if value.chars().any(char::is_whitespace) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{default_config, ApiMode};

    fn parse_request() -> ParseRequest {
        ParseRequest {
            input: "BV1GJ411x7h7".to_string(),
            config: default_config(),
            show_all_parts: true,
            use_cache: true,
        }
    }

    fn download_request() -> DownloadRequest {
        DownloadRequest {
            input: "BV1GJ411x7h7".to_string(),
            config: default_config(),
            parse_id: None,
        }
    }

    #[test]
    fn builds_parse_args_with_info_show_all_and_all_parts() {
        let args = build_parse_args(&parse_request());

        assert_eq!(
            args,
            vec![
                "BV1GJ411x7h7".to_string(),
                "-info".to_string(),
                "--show-all".to_string(),
                "-p".to_string(),
                "ALL".to_string(),
            ],
        );
    }

    #[test]
    fn builds_download_args_from_default_options() {
        let args = build_download_args(&download_request());

        assert!(args.starts_with(&[
            "BV1GJ411x7h7".to_string(),
            "--work-dir".to_string(),
            "../download".to_string(),
        ]));
        assert!(contains_pair(&args, "--ffmpeg-path", "../bin/ffmpeg.exe"));
        assert!(contains_pair(&args, "-p", "1"));
        assert!(contains_pair(&args, "-e", "avc,hevc,av1"));
        assert!(contains_pair(
            &args,
            "-q",
            "4K 超清, 1080P 高码率, 1080P 高清"
        ));
        assert!(args.contains(&"-dd".to_string()));
        assert!(args.contains(&"-mt".to_string()));
        assert!(args.contains(&"--skip-ai".to_string()));
    }

    #[test]
    fn builds_audio_only_download_args() {
        let mut request = download_request();
        request.config.default_options.media.video = false;
        request.config.default_options.media.audio = true;

        let args = build_download_args(&request);

        assert!(args.contains(&"--audio-only".to_string()));
        assert!(!args.contains(&"--video-only".to_string()));
    }

    #[test]
    fn builds_danmaku_only_download_args() {
        let mut request = download_request();
        request.config.default_options.media.video = false;
        request.config.default_options.media.audio = false;
        request.config.default_options.media.danmaku = true;
        request.config.default_options.media.subtitle = false;
        request.config.default_options.media.cover = false;

        let args = build_download_args(&request);

        assert!(args.contains(&"--danmaku-only".to_string()));
        assert!(!args.contains(&"-dd".to_string()));
    }

    #[test]
    fn appends_non_web_api_mode_and_auth_args() {
        let mut request = parse_request();
        request.config.auth.api_mode = ApiMode::TV;
        request.config.auth.cookie = Some("SESSDATA=secret".to_string());
        request.config.auth.access_token = Some("token-secret".to_string());
        request.config.auth.user_agent = Some("BBDown Next".to_string());

        let args = build_parse_args(&request);

        assert!(args.contains(&"-tv".to_string()));
        assert!(contains_pair(&args, "-c", "SESSDATA=secret"));
        assert!(contains_pair(&args, "-token", "token-secret"));
        assert!(contains_pair(&args, "-ua", "BBDown Next"));
    }

    #[test]
    fn command_preview_redacts_cookie_and_token() {
        let mut request = parse_request();
        request.config.auth.cookie = Some("SESSDATA=secret".to_string());
        request.config.auth.access_token = Some("token-secret".to_string());
        let args = build_parse_args(&request);

        let preview = make_command_preview("BBDown.exe".to_string(), args);

        assert!(preview.display.contains("\"***\""));
        assert!(!preview.display.contains("SESSDATA=secret"));
        assert!(!preview.display.contains("token-secret"));
    }

    fn contains_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2)
            .any(|window| window[0] == flag && window[1] == value)
    }
}
