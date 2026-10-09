//! The subset of BBDown's protobuf schema needed by playback and subtitle RPCs.
//! Unknown fields are ignored, so response schema additions remain compatible.
use super::client::{network_error, BiliClient, ContentPart};
use base64::{engine::general_purpose::STANDARD, Engine};
use prost::Message;
use serde_json::{json, Value};
use std::io::{Read, Write};

const BUILD: i32 = 7320200;
const CHANNEL: &str = "xiaomi_cn_tv.danmaku.bili_zm20200902";
const UA: &str = "Dalvik/2.1.0 (Linux; U; Android 11; M2012K11AC Build/RKQ1.200826.002) 7.32.0 os/android model/M2012K11AC mobi_app/android build/7320200 channel/xiaomi_cn_tv.danmaku.bili_zm20200902 innerVer/7320200 osVer/11 network/2 grpc-java-cronet/1.36.1";
const MAX_MESSAGE: usize = 32 * 1024 * 1024;

#[derive(Clone, PartialEq, Message)]
struct PlayRequest {
    #[prost(int64, tag = "1")]
    id: i64,
    #[prost(int64, tag = "2")]
    cid: i64,
    #[prost(int64, tag = "3")]
    quality: i64,
    #[prost(int32, tag = "5")]
    fnval: i32,
    #[prost(int32, tag = "7")]
    force_host: i32,
    #[prost(bool, tag = "8")]
    fourk: bool,
    #[prost(string, tag = "9")]
    spmid: String,
    #[prost(string, tag = "10")]
    from_spmid: String,
    #[prost(int32, tag = "12")]
    codec: i32,
}
#[derive(Clone, PartialEq, Message)]
struct Metadata {
    #[prost(string, tag = "1")]
    access_key: String,
    #[prost(string, tag = "2")]
    mobi_app: String,
    #[prost(int32, tag = "4")]
    build: i32,
    #[prost(string, tag = "5")]
    channel: String,
    #[prost(string, tag = "7")]
    platform: String,
}
#[derive(Clone, PartialEq, Message)]
struct Device {
    #[prost(int32, tag = "1")]
    app_id: i32,
    #[prost(int32, tag = "2")]
    build: i32,
    #[prost(string, tag = "4")]
    mobi_app: String,
    #[prost(string, tag = "5")]
    platform: String,
    #[prost(string, tag = "7")]
    channel: String,
    #[prost(string, tag = "8")]
    brand: String,
    #[prost(string, tag = "9")]
    model: String,
    #[prost(string, tag = "10")]
    osver: String,
}
#[derive(Clone, PartialEq, Message)]
struct Network {
    #[prost(int32, tag = "1")]
    kind: i32,
    #[prost(string, tag = "3")]
    oid: String,
}
#[derive(Clone, PartialEq, Message)]
struct Fawkes {
    #[prost(string, tag = "1")]
    appkey: String,
    #[prost(string, tag = "2")]
    env: String,
    #[prost(string, tag = "3")]
    session_id: String,
}
#[derive(Clone, PartialEq, Message)]
struct Locale {
    #[prost(message, optional, tag = "1")]
    locale: Option<LocaleIds>,
}
#[derive(Clone, PartialEq, Message)]
struct LocaleIds {
    #[prost(string, tag = "1")]
    language: String,
    #[prost(string, tag = "3")]
    region: String,
}
#[derive(Clone, PartialEq, Message)]
struct PlayReply {
    #[prost(message, optional, tag = "1")]
    video: Option<VideoInfo>,
    #[prost(message, optional, tag = "3")]
    business: Option<Business>,
}
#[derive(Clone, PartialEq, Message)]
struct VideoInfo {
    #[prost(uint64, tag = "3")]
    duration: u64,
    #[prost(message, repeated, tag = "5")]
    streams: Vec<Stream>,
    #[prost(message, repeated, tag = "6")]
    audio: Vec<DashAudio>,
    #[prost(message, optional, tag = "7")]
    dolby: Option<SpecialAudio>,
    #[prost(message, optional, tag = "9")]
    flac: Option<SpecialAudio>,
}
#[derive(Clone, PartialEq, Message)]
struct Business {
    #[prost(bool, tag = "1")]
    preview: bool,
    #[prost(message, repeated, tag = "6")]
    clips: Vec<Clip>,
}
#[derive(Clone, PartialEq, Message)]
struct Clip {
    #[prost(int32, tag = "2")]
    start: i32,
    #[prost(int32, tag = "3")]
    end: i32,
    #[prost(string, tag = "5")]
    title: String,
}
#[derive(Clone, PartialEq, Message)]
struct Stream {
    #[prost(message, optional, tag = "1")]
    info: Option<StreamInfo>,
    #[prost(message, optional, tag = "2")]
    dash: Option<DashVideo>,
    #[prost(message, optional, tag = "3")]
    segments: Option<SegmentVideo>,
}
#[derive(Clone, PartialEq, Message)]
struct StreamInfo {
    #[prost(uint32, tag = "1")]
    quality: u32,
    #[prost(uint32, tag = "4")]
    error: u32,
}
#[derive(Clone, PartialEq, Message)]
struct DashVideo {
    #[prost(string, tag = "1")]
    url: String,
    #[prost(string, repeated, tag = "2")]
    backup: Vec<String>,
    #[prost(uint32, tag = "3")]
    bandwidth: u32,
    #[prost(uint32, tag = "4")]
    codec: u32,
    #[prost(uint64, tag = "6")]
    size: u64,
}
#[derive(Clone, PartialEq, Message)]
struct DashAudio {
    #[prost(uint32, tag = "1")]
    id: u32,
    #[prost(string, tag = "2")]
    url: String,
    #[prost(string, repeated, tag = "3")]
    backup: Vec<String>,
    #[prost(uint32, tag = "4")]
    bandwidth: u32,
    #[prost(uint32, tag = "5")]
    codec: u32,
}
#[derive(Clone, PartialEq, Message)]
struct SpecialAudio {
    #[prost(message, optional, tag = "2")]
    audio: Option<DashAudio>,
}
#[derive(Clone, PartialEq, Message)]
struct SegmentVideo {
    #[prost(message, repeated, tag = "1")]
    segments: Vec<Segment>,
}
#[derive(Clone, PartialEq, Message)]
struct Segment {
    #[prost(uint32, tag = "1")]
    order: u32,
    #[prost(uint64, tag = "2")]
    length: u64,
    #[prost(string, tag = "4")]
    url: String,
    #[prost(string, repeated, tag = "5")]
    backup: Vec<String>,
}
#[derive(Clone, PartialEq, Message)]
struct DmRequest {
    #[prost(int64, tag = "1")]
    aid: i64,
    #[prost(int64, tag = "2")]
    cid: i64,
    #[prost(int32, tag = "3")]
    kind: i32,
    #[prost(string, tag = "4")]
    spmid: String,
}
#[derive(Clone, PartialEq, Message)]
struct DmReply {
    #[prost(message, optional, tag = "3")]
    subtitle: Option<VideoSubtitle>,
}
#[derive(Clone, PartialEq, Message)]
struct VideoSubtitle {
    #[prost(message, repeated, tag = "3")]
    subtitles: Vec<SubtitleItem>,
}
#[derive(Clone, PartialEq, Message)]
struct SubtitleItem {
    #[prost(string, tag = "3")]
    language: String,
    #[prost(string, tag = "5")]
    url: String,
}

pub(crate) fn pack_message(message: &[u8]) -> Result<Vec<u8>, String> {
    if message.len() > MAX_MESSAGE {
        return Err("APP 请求过大".into());
    }
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(message).map_err(|e| e.to_string())?;
    let compressed = encoder.finish().map_err(|e| e.to_string())?;
    let mut frame = vec![1];
    frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    frame.extend(compressed);
    Ok(frame)
}

pub(crate) fn read_message(frame: &[u8]) -> Result<Vec<u8>, String> {
    if frame.len() < 5 {
        return Err("APP gRPC 响应缺少报文，请检查账号和观看权限".into());
    }
    let length = u32::from_be_bytes(frame[1..5].try_into().unwrap()) as usize;
    if length > MAX_MESSAGE || frame.len() != length + 5 {
        return Err("APP gRPC 响应长度无效".into());
    }
    match frame[0] {
        0 => Ok(frame[5..].to_vec()),
        1 => {
            let mut output = Vec::new();
            flate2::read::GzDecoder::new(&frame[5..])
                .take(MAX_MESSAGE as u64 + 1)
                .read_to_end(&mut output)
                .map_err(|e| format!("APP 响应解压失败: {e}"))?;
            if output.len() > MAX_MESSAGE {
                return Err("APP 响应过大".into());
            }
            Ok(output)
        }
        _ => Err("APP gRPC 压缩标识无效".into()),
    }
}

async fn rpc(
    client: &BiliClient,
    endpoint: &str,
    payload: impl Message,
) -> Result<Vec<u8>, String> {
    let metadata = Metadata {
        access_key: client.token.clone().unwrap_or_default(),
        mobi_app: "android".into(),
        build: BUILD,
        channel: CHANNEL.into(),
        platform: "android".into(),
    };
    let device = Device {
        app_id: 1,
        build: BUILD,
        mobi_app: "android".into(),
        platform: "android".into(),
        channel: CHANNEL.into(),
        brand: "M2012K11AC".into(),
        model: "Build/RKQ1.200826.002".into(),
        osver: "11".into(),
    };
    let fawkes = Fawkes {
        appkey: "android64".into(),
        env: "prod".into(),
        session_id: "dedf8669".into(),
    };
    let locale = Locale {
        locale: Some(LocaleIds {
            language: "zh".into(),
            region: "CN".into(),
        }),
    };
    let mut request = client
        .http
        .post(client.api_target(endpoint))
        .timeout(std::time::Duration::from_secs(45))
        // The subtitle gateway also serves gRPC-framed messages over HTTP/1.1.
        // Let TLS negotiate the supported version instead of requiring HTTP/2.
        .header("user-agent", UA)
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .header("grpc-encoding", "gzip")
        .header("grpc-accept-encoding", "identity,gzip")
        .header("grpc-timeout", "30S")
        .header(
            "x-bili-metadata-bin",
            STANDARD.encode(metadata.encode_to_vec()),
        )
        .header("x-bili-device-bin", STANDARD.encode(device.encode_to_vec()))
        .header(
            "x-bili-fawkes-req-bin",
            STANDARD.encode(fawkes.encode_to_vec()),
        )
        .header(
            "x-bili-network-bin",
            STANDARD.encode(
                Network {
                    kind: 1,
                    oid: "46007".into(),
                }
                .encode_to_vec(),
            ),
        )
        .header("x-bili-locale-bin", STANDARD.encode(locale.encode_to_vec()));
    if let Some(token) = &client.token {
        request = request.header("authorization", format!("identify_v1 {token}"));
    }
    let mut response = request
        .body(pack_message(&payload.encode_to_vec())?)
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    if let Some(status) = response
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .filter(|s| *s != "0")
    {
        return Err(format!("APP gRPC 接口返回错误 {status}"));
    }
    if !response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.starts_with("application/grpc"))
    {
        return Err("APP 接口没有返回 gRPC 数据，请检查网络和服务状态".into());
    }
    let mut frame = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if frame.len() + chunk.len() > MAX_MESSAGE + 5 {
            return Err("APP 响应过大".into());
        }
        frame.extend(chunk);
    }
    read_message(&frame)
}

pub async fn play(client: &BiliClient, part: &ContentPart) -> Result<Value, String> {
    if part.cheese {
        return Err("APP 接口不支持课程，请使用 WEB 模式".into());
    }
    let pgc = part.episode_id.is_some();
    if pgc
        && client
            .codecs
            .first()
            .is_some_and(|codec| !codec.eq_ignore_ascii_case("hevc"))
    {
        client.report("APP 番剧接口使用 HEVC 编码")?;
    }
    let endpoint = if pgc {
        "https://app.bilibili.com/bilibili.pgc.gateway.player.v2.PlayURL/PlayView"
    } else {
        "https://grpc.biliapi.net/bilibili.app.playurl.v1.PlayURL/PlayView"
    };
    let codecs: Vec<i32> = if pgc {
        vec![2]
    } else {
        let mut codecs = client
            .codecs
            .iter()
            .filter_map(|s| match s.to_ascii_lowercase().as_str() {
                "avc" => Some(1),
                "hevc" => Some(2),
                "av1" => Some(3),
                _ => None,
            })
            .collect::<Vec<_>>();
        if codecs.is_empty() {
            codecs = vec![1, 2, 3];
        }
        codecs.dedup();
        codecs
    };
    let mut video = Vec::new();
    let mut audio = Vec::new();
    let mut clips = Vec::new();
    let mut progressive = Vec::new();
    let mut duration = part.duration * 1000;
    let mut preview = false;
    let mut last_error = None;
    for codec in codecs {
        let payload = PlayRequest {
            id: part
                .episode_id
                .unwrap_or(part.aid)
                .try_into()
                .map_err(|_| "视频 ID 超出范围")?,
            cid: part.cid.try_into().map_err(|_| "CID 超出范围")?,
            quality: 127,
            fnval: 4048,
            force_host: 2,
            fourk: true,
            spmid: "main.ugc-video-detail.0.0".into(),
            from_spmid: "main.my-history.0.0".into(),
            codec,
        };
        let bytes = match rpc(client, endpoint, payload).await {
            Ok(bytes) => bytes,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };
        let reply = PlayReply::decode(bytes.as_slice())
            .map_err(|e| format!("解析 APP 播放响应失败: {e}"))?;
        let value = match play_reply_json(reply) {
            Ok(value) => value,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };
        let data = &value["data"];
        duration = data["timelength"].as_u64().unwrap_or(duration);
        preview |= data["is_preview"].as_bool() == Some(true);
        if let Some(tracks) = data["dash"]["video"].as_array() {
            video.extend(tracks.iter().cloned());
        }
        if let Some(tracks) = data["dash"]["audio"].as_array() {
            audio.extend(tracks.iter().cloned());
        }
        if let Some(entries) = data["clip_info_list"].as_array() {
            clips.extend(entries.iter().cloned());
        }
        if let Some(entries) = data["durl"].as_array() {
            if progressive.is_empty() {
                progressive = entries.clone();
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    video.retain(|t| seen.insert((t["id"].as_u64(), t["codecid"].as_u64())));
    seen.clear();
    audio.retain(|t| seen.insert((t["id"].as_u64(), t["codecid"].as_u64())));
    if video.is_empty() && audio.is_empty() && progressive.is_empty() {
        return Err(
            last_error.unwrap_or_else(|| "APP 没有返回可播放媒体，请检查账号和观看权限".into())
        );
    }
    Ok(
        json!({"code":0,"data":{"timelength":duration,"is_preview":preview,"dash":{"video":video,"audio":audio},"durl":progressive,"clip_info_list":clips}}),
    )
}

fn audio_json(track: DashAudio) -> Value {
    json!({"id":track.id,"base_url":track.url,"backup_url":track.backup,"bandwidth":track.bandwidth,"codecid":track.codec})
}
fn play_reply_json(reply: PlayReply) -> Result<Value, String> {
    let info = reply.video.ok_or("APP 响应没有视频信息，请检查观看权限")?;
    let mut video = Vec::new();
    let mut progressive = Vec::new();
    for stream in info.streams {
        let description = stream.info.unwrap_or_default();
        if description.error != 0 {
            continue;
        }
        if let Some(dash) = stream.dash.filter(|t| !t.url.is_empty()) {
            let bandwidth = if dash.bandwidth > 0 {
                dash.bandwidth as u64
            } else {
                dash.size
                    .saturating_mul(8000)
                    .checked_div(info.duration)
                    .unwrap_or(0)
            };
            video.push(json!({"id":description.quality,"base_url":dash.url,"backup_url":dash.backup,"bandwidth":bandwidth,"codecid":dash.codec}));
        }
        if progressive.is_empty() {
            if let Some(segments) = stream.segments {
                progressive = segments.segments.into_iter().map(|s| json!({"url":s.url,"backup_url":s.backup,"length":s.length,"order":s.order})).collect();
            }
        }
    }
    let mut audio = info.audio.into_iter().map(audio_json).collect::<Vec<_>>();
    for extra in [info.dolby, info.flac]
        .into_iter()
        .flatten()
        .filter_map(|s| s.audio)
    {
        audio.push(audio_json(extra));
    }
    let business = reply.business.unwrap_or_default();
    let clips = business
        .clips
        .into_iter()
        .map(|c| json!({"start":c.start,"end":c.end,"toastText":c.title}))
        .collect::<Vec<_>>();
    Ok(
        json!({"code":0,"data":{"is_preview":business.preview,"timelength":info.duration,"dash":{"video":video,"audio":audio},"durl":progressive,"clip_info_list":clips}}),
    )
}

pub async fn subtitles(client: &BiliClient, part: &ContentPart) -> Result<Vec<Value>, String> {
    let payload = DmRequest {
        aid: part.aid.try_into().map_err(|_| "视频 ID 超出范围")?,
        cid: part.cid.try_into().map_err(|_| "CID 超出范围")?,
        kind: 1,
        spmid: "main.ugc-video-detail.0.0".into(),
    };
    let bytes = rpc(
        client,
        "https://app.biliapi.net/bilibili.community.service.dm.v1.DM/DmView",
        payload,
    )
    .await?;
    let reply =
        DmReply::decode(bytes.as_slice()).map_err(|e| format!("解析 APP 字幕响应失败: {e}"))?;
    Ok(reply
        .subtitle
        .unwrap_or_default()
        .subtitles
        .into_iter()
        .filter(|s| !s.url.is_empty())
        .map(|s| json!({"lan":s.language,"subtitle_url":s.url}))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn subtitle_rpc_accepts_http1_gateway_and_decodes_grpc_payload() {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response = pack_message(
            &DmReply {
                subtitle: Some(VideoSubtitle {
                    subtitles: vec![SubtitleItem {
                        language: "zh-CN".into(),
                        url: "https://example.com/subtitle.json".into(),
                    }],
                }),
            }
            .encode_to_vec(),
        )
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            let header_end = loop {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            assert!(headers.lines().next().unwrap().ends_with(" HTTP/1.1"));
            let size = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            while bytes.len() < header_end + size {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            let payload = DmRequest::decode(
                read_message(&bytes[header_end..header_end + size])
                    .unwrap()
                    .as_slice(),
            )
            .unwrap();
            assert_eq!((payload.aid, payload.cid), (170001, 279786));
            let headers = format!("HTTP/1.1 200 OK\r\nContent-Type: application/grpc\r\nContent-Length: {}\r\ngrpc-status: 0\r\ngrpc-encoding: gzip\r\nConnection: close\r\n\r\n", response.len());
            socket.write_all(headers.as_bytes()).await.unwrap();
            socket.write_all(&response).await.unwrap();
        });
        let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
        client.http = reqwest::Client::builder().no_proxy().build().unwrap();
        client.api_origin = Some(format!("http://{address}"));
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            subtitles(
                &client,
                &ContentPart {
                    aid: 170001,
                    cid: 279786,
                    ..Default::default()
                },
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result[0]["lan"], "zh-CN");
        assert_eq!(
            result[0]["subtitle_url"],
            "https://example.com/subtitle.json"
        );
        server.await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires the live APP subtitle service; does not use local credentials"]
    async fn live_app_subtitle_gateway_negotiates_supported_http_version() {
        let client = BiliClient::new(&crate::core::default_config()).unwrap();
        let rows = subtitles(
            &client,
            &ContentPart {
                aid: 170001,
                cid: 279786,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(rows
            .iter()
            .all(|row| !super::super::client::string(&row["subtitle_url"]).is_empty()));
    }
    #[test]
    fn framing_rejects_truncation_and_round_trips_gzip() {
        let bytes = b"protobuf fixture";
        assert_eq!(read_message(&pack_message(bytes).unwrap()).unwrap(), bytes);
        assert!(read_message(&[0, 0, 0, 0, 5, 1]).is_err());
        assert!(read_message(&[2, 0, 0, 0, 0]).is_err());
        assert!(read_message(&[]).is_err());
    }
    #[test]
    fn decodes_playback_and_special_audio_without_exposing_urls_in_stream_info() {
        let reply = PlayReply {
            video: Some(VideoInfo {
                duration: 1000,
                streams: vec![Stream {
                    info: Some(StreamInfo {
                        quality: 120,
                        error: 0,
                    }),
                    dash: Some(DashVideo {
                        url: "https://example.com/v".into(),
                        codec: 12,
                        size: 1000,
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                flac: Some(SpecialAudio {
                    audio: Some(DashAudio {
                        id: 30251,
                        url: "https://example.com/a".into(),
                        ..Default::default()
                    }),
                }),
                ..Default::default()
            }),
            business: Some(Business {
                preview: false,
                clips: vec![Clip {
                    start: 0,
                    end: 5,
                    title: "片头".into(),
                }],
            }),
        };
        let wire = reply.encode_to_vec();
        let value = play_reply_json(PlayReply::decode(wire.as_slice()).unwrap()).unwrap();
        let streams = crate::bilibili::streams::parse_play_response(&value, 1).unwrap();
        assert_eq!(streams.video[0].codec, "HEVC");
        assert_eq!(streams.audio[0].codec, "FLAC");
        assert_eq!(value["data"]["clip_info_list"][0]["toastText"], "片头");
    }
}
