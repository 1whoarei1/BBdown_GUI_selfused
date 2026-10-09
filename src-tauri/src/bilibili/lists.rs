use super::{
    client::{check_api, https, string, BiliClient, Content},
    intl::number,
    VideoId,
};
use crate::core::{ContentKind, OwnerInfo};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

const PAGE_SIZE: u64 = 30;
const MAX_PAGES: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListId {
    Favorites { fid: Option<u64>, mid: Option<u64> },
    Space(u64),
    Collection { id: u64, mid: Option<u64> },
    Series { id: u64, mid: Option<u64> },
}

pub fn identify_input(input: &str) -> Result<Option<ListId>, String> {
    let trimmed = input.trim();
    for prefix in [
        "fav:",
        "favId:",
        "space:",
        "mid:",
        "list:",
        "listBizId:",
        "series:",
        "seriesBizId:",
    ] {
        if let Some(value) = trimmed.strip_prefix(prefix) {
            let parts = value.split(':').collect::<Vec<_>>();
            let parse = |s: &str| {
                s.parse::<u64>()
                    .ok()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| "列表 ID 必须是正整数".to_string())
            };
            let first = parts.first().copied().unwrap_or("");
            return Ok(Some(match prefix {
                "fav:" | "favId:" => ListId::Favorites {
                    fid: if first.is_empty() {
                        None
                    } else {
                        Some(parse(first)?)
                    },
                    mid: parts
                        .get(1)
                        .filter(|s| !s.is_empty())
                        .map(|s| parse(s))
                        .transpose()?,
                },
                "space:" | "mid:" => ListId::Space(parse(first)?),
                "list:" | "listBizId:" => ListId::Collection {
                    id: parse(first)?,
                    mid: None,
                },
                _ => ListId::Series {
                    id: parse(first)?,
                    mid: None,
                },
            }));
        }
    }
    let url = trimmed.split_whitespace().find_map(|word| {
        reqwest::Url::parse(word.trim_matches(|c| matches!(c, '，' | '。' | '）' | ')'))).ok()
    });
    let Some(url) = url else {
        return Ok(None);
    };
    let host = url.host_str().unwrap_or("");
    if !(host == "bilibili.com" || host.ends_with(".bilibili.com")) {
        return Ok(None);
    }
    let query = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect::<BTreeMap<_, _>>();
    let integer = |key: &str| {
        query
            .get(key)
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|id| *id > 0)
    };
    let segments = url
        .path_segments()
        .map(|s| s.collect::<Vec<_>>())
        .unwrap_or_default();
    let mid = if host == "space.bilibili.com" {
        segments.first().and_then(|s| s.parse().ok())
    } else {
        segments.iter().rev().find_map(|s| s.parse().ok())
    };
    if url.path().contains("/favlist") {
        return Ok(Some(ListId::Favorites {
            fid: integer("fid"),
            mid,
        }));
    }
    if matches!(segments.first().copied(), Some("list" | "medialist")) {
        if let Some(fid) = integer("fid").or_else(|| integer("media_id")).or_else(|| {
            segments
                .iter()
                .find_map(|s| s.strip_prefix("ml").and_then(|s| s.parse::<u64>().ok()))
        }) {
            return Ok(Some(ListId::Favorites {
                fid: Some(fid),
                mid,
            }));
        }
    }
    if let Some(index) = segments.iter().position(|s| *s == "lists") {
        let id = segments
            .get(index + 1)
            .and_then(|s| s.parse().ok())
            .ok_or("合集链接缺少有效 ID")?;
        return match query.get("type").map(String::as_str) {
            Some("series") => Ok(Some(ListId::Series { id, mid })),
            Some("season") | None => Ok(Some(ListId::Collection { id, mid })),
            _ => Err("无法识别列表类型，请使用 type=season 或 type=series".into()),
        };
    }
    let collection = url.path().contains("collectiondetail")
        || query
            .get("business")
            .is_some_and(|s| s == "space_collection");
    let series = url.path().contains("seriesdetail")
        || query.get("business").is_some_and(|s| s == "space_series");
    if collection || series {
        let id = integer("sid")
            .or_else(|| integer("business_id"))
            .ok_or("列表链接缺少 sid / business_id")?;
        return Ok(Some(if series {
            ListId::Series { id, mid }
        } else {
            ListId::Collection { id, mid }
        }));
    }
    if host == "space.bilibili.com" {
        return mid
            .filter(|id| *id > 0)
            .map(ListId::Space)
            .map(Some)
            .ok_or_else(|| "空间链接缺少 UID".into());
    }
    Ok(None)
}

struct Listing {
    title: String,
    description: String,
    cover: String,
    owner: Option<OwnerInfo>,
    published: Option<String>,
    videos: Vec<Value>,
    warnings: Vec<String>,
}
impl Listing {
    fn new(title: String) -> Self {
        Self {
            title,
            description: String::new(),
            cover: String::new(),
            owner: None,
            published: None,
            videos: vec![],
            warnings: vec![],
        }
    }
}

pub async fn resolve(client: &mut BiliClient, id: ListId) -> Result<Content, String> {
    let listing = match id {
        ListId::Favorites { fid, mid } => favorites(client, fid, mid).await?,
        ListId::Space(mid) => space(client, mid).await?,
        ListId::Collection { id, mid } => match collection(client, id, mid).await {
            Ok(list) => list,
            Err(primary) => {
                client.report("合集新接口暂不可用，尝试旧列表接口")?;
                legacy(client, id, 8, mid)
                    .await
                    .map_err(|fallback| format!("{primary}；旧列表接口: {fallback}"))?
            }
        },
        ListId::Series { id, mid } => match series(client, id, mid).await {
            Ok(list) => list,
            Err(primary) => {
                client.report("系列新接口暂不可用，尝试旧列表接口")?;
                legacy(client, id, 5, mid)
                    .await
                    .map_err(|fallback| format!("{primary}；旧列表接口: {fallback}"))?
            }
        },
    };
    expand(client, listing).await
}

fn owner(value: &Value) -> Option<OwnerInfo> {
    let mid = number(&value["mid"]);
    let name = value["name"]
        .as_str()
        .or_else(|| value["uname"].as_str())
        .map(str::to_string);
    if mid.is_none() && name.is_none() {
        return None;
    }
    Some(OwnerInfo {
        mid,
        name,
        face_url: value["face"].as_str().map(https),
        space_url: mid.map(|id| format!("https://space.bilibili.com/{id}")),
    })
}

async fn favorites(
    client: &BiliClient,
    fid: Option<u64>,
    mid: Option<u64>,
) -> Result<Listing, String> {
    let folders = if let Some(fid) = fid {
        vec![fid]
    } else {
        let mid = mid.ok_or("收藏夹链接需要 fid 或空间 UID")?;
        let value = client
            .api(
                "https://api.bilibili.com/x/v3/fav/folder/created/list-all",
                &[("up_mid", mid.to_string())],
            )
            .await?;
        check_api(&value)?;
        value["data"]["list"]
            .as_array()
            .ok_or("没有可访问的收藏夹")?
            .iter()
            .filter_map(|folder| number(&folder["id"]))
            .collect()
    };
    if folders.is_empty() {
        return Err("没有可访问的收藏夹".into());
    }
    let multiple = folders.len() > 1;
    let mut listing = Listing::new(if multiple {
        format!("UID {} 的收藏夹", mid.unwrap_or(0))
    } else {
        "收藏夹".into()
    });
    for folder in folders {
        let mut pager = Pagination::default();
        for page in 1..=MAX_PAGES {
            client.report(format!("正在读取收藏夹 {folder} 第 {page} 页"))?;
            let value = client
                .api(
                    "https://api.bilibili.com/x/v3/fav/resource/list",
                    &[
                        ("media_id", folder.to_string()),
                        ("pn", page.to_string()),
                        ("ps", PAGE_SIZE.to_string()),
                        ("order", "mtime".into()),
                        ("type", "2".into()),
                        ("tid", "0".into()),
                        ("platform", "web".into()),
                    ],
                )
                .await?;
            check_api(&value)?;
            let data = &value["data"];
            if data.is_null() {
                return Err("收藏夹为空、私密或当前账号无权访问".into());
            }
            if page == 1 {
                if !multiple {
                    listing.title = string(&data["info"]["title"]).into();
                    listing.description = string(&data["info"]["intro"]).into();
                }
                listing.owner = listing.owner.or_else(|| owner(&data["info"]["upper"]));
                listing.cover = https(string(&data["info"]["cover"]));
                listing.published = number(&data["info"]["ctime"]).map(|t| t.to_string());
            }
            let rows = data["medias"].as_array().cloned().unwrap_or_default();
            let more = data["has_more"].as_bool().unwrap_or_else(|| {
                number(&data["info"]["media_count"]).is_some_and(|total| page * PAGE_SIZE < total)
            });
            pager.check(&rows, more)?;
            listing.videos.extend(rows);
            if !more {
                break;
            }
            if page == MAX_PAGES {
                return Err("收藏夹分页超出上限".into());
            }
        }
    }
    Ok(listing)
}

async fn space(client: &mut BiliClient, mid: u64) -> Result<Listing, String> {
    let mut listing = Listing::new(format!("UID {mid} 的投稿"));
    let mut pager = Pagination::default();
    for page in 1..=MAX_PAGES {
        client.report(format!("正在读取空间 UID {mid} 第 {page} 页"))?;
        let value = client
            .signed_api(
                "/x/space/wbi/arc/search",
                &[
                    ("mid", mid.to_string()),
                    ("pn", page.to_string()),
                    ("ps", PAGE_SIZE.to_string()),
                    ("order", "pubdate".into()),
                    ("tid", "0".into()),
                ],
            )
            .await?;
        check_api(&value)?;
        let data = &value["data"];
        let rows = data["list"]["vlist"]
            .as_array()
            .ok_or("空间接口没有返回投稿列表")?;
        if page == 1 {
            if let Some(author) = rows.first().and_then(|row| row["author"].as_str()) {
                listing.title = format!("{author}的投稿");
                listing.owner = Some(OwnerInfo {
                    mid: Some(mid),
                    name: Some(author.into()),
                    face_url: None,
                    space_url: Some(format!("https://space.bilibili.com/{mid}")),
                });
            }
        }
        let total = number(&data["page"]["count"]).ok_or("空间接口缺少投稿总数")?;
        let more = page * PAGE_SIZE < total;
        pager.check(rows, more)?;
        listing.videos.extend(rows.iter().cloned());
        if !more {
            return Ok(listing);
        }
    }
    Err("空间分页超出上限".into())
}

async fn collection(client: &BiliClient, id: u64, mid: Option<u64>) -> Result<Listing, String> {
    let mid = match mid {
        Some(mid) => mid,
        None => {
            let info = client
                .api(
                    "https://api.bilibili.com/x/v1/medialist/info",
                    &[("type", "8".into()), ("biz_id", id.to_string())],
                )
                .await?;
            check_api(&info)?;
            number(&info["data"]["upper"]["mid"]).ok_or("合集接口缺少作者 UID")?
        }
    };
    let mut listing = Listing::new(format!("合集 {id}"));
    let mut pager = Pagination::default();
    for page in 1..=MAX_PAGES {
        client.report(format!("正在读取合集 {id} 第 {page} 页"))?;
        let value = client
            .api(
                "https://api.bilibili.com/x/polymer/web-space/seasons_archives_list",
                &[
                    ("mid", mid.to_string()),
                    ("season_id", id.to_string()),
                    ("page_num", page.to_string()),
                    ("page_size", PAGE_SIZE.to_string()),
                    ("sort_reverse", "false".into()),
                ],
            )
            .await?;
        check_api(&value)?;
        let data = &value["data"];
        if number(&data["meta"]["mid"]).is_some_and(|actual| actual != mid) {
            return Err("合集链接中的 UID 与作者不一致".into());
        }
        if page == 1 {
            listing.title = data["meta"]["name"]
                .as_str()
                .unwrap_or(&listing.title)
                .into();
            listing.description = string(&data["meta"]["description"]).into();
            listing.cover = https(string(&data["meta"]["cover"]));
        }
        let rows = data["archives"]
            .as_array()
            .ok_or("合集接口没有返回视频列表")?;
        let total = number(&data["page"]["total"])
            .or_else(|| number(&data["meta"]["total"]))
            .ok_or("合集接口缺少视频总数")?;
        let more = page * PAGE_SIZE < total;
        pager.check(rows, more)?;
        listing.videos.extend(rows.iter().cloned());
        if !more {
            return Ok(listing);
        }
    }
    Err("合集分页超出上限".into())
}

async fn series(client: &BiliClient, id: u64, mid: Option<u64>) -> Result<Listing, String> {
    let info = client
        .api(
            "https://api.bilibili.com/x/series/series",
            &[("series_id", id.to_string())],
        )
        .await?;
    check_api(&info)?;
    let meta = &info["data"]["meta"];
    let actual_mid = number(&meta["mid"]);
    if mid
        .zip(actual_mid)
        .is_some_and(|(given, actual)| given != actual)
    {
        return Err("系列链接中的 UID 与系列作者不一致".into());
    }
    let mid = actual_mid.or(mid).ok_or("系列接口缺少作者 UID")?;
    let mut listing = Listing::new(string(&meta["name"]).into());
    listing.description = string(&meta["description"]).into();
    let mut pager = Pagination::default();
    for page in 1..=MAX_PAGES {
        client.report(format!("正在读取系列 {id} 第 {page} 页"))?;
        let value = client
            .api(
                "https://api.bilibili.com/x/series/archives",
                &[
                    ("mid", mid.to_string()),
                    ("series_id", id.to_string()),
                    ("pn", page.to_string()),
                    ("ps", PAGE_SIZE.to_string()),
                    ("only_normal", "true".into()),
                    ("sort", "desc".into()),
                ],
            )
            .await?;
        check_api(&value)?;
        let data = &value["data"];
        let rows = data["archives"]
            .as_array()
            .ok_or("系列接口没有返回视频列表")?;
        let total = number(&data["page"]["total"])
            .or_else(|| number(&meta["total"]))
            .ok_or("系列接口缺少视频总数")?;
        let more = page * PAGE_SIZE < total;
        pager.check(rows, more)?;
        listing.videos.extend(rows.iter().cloned());
        if !more {
            return Ok(listing);
        }
    }
    Err("系列分页超出上限".into())
}

async fn legacy(
    client: &BiliClient,
    id: u64,
    kind: u64,
    mid: Option<u64>,
) -> Result<Listing, String> {
    let value = client
        .api(
            "https://api.bilibili.com/x/v1/medialist/info",
            &[
                ("type", kind.to_string()),
                ("biz_id", id.to_string()),
                ("tid", "0".into()),
            ],
        )
        .await?;
    check_api(&value)?;
    let info = &value["data"];
    if !info.is_object() {
        return Err("列表不存在或无权访问".into());
    }
    let list_owner = owner(&info["upper"]);
    if mid
        .zip(list_owner.as_ref().and_then(|o| o.mid))
        .is_some_and(|(given, actual)| given != actual)
    {
        return Err("列表链接中的 UID 与作者不一致".into());
    }
    let mut listing = Listing::new(string(&info["title"]).into());
    listing.description = string(&info["intro"]).into();
    listing.owner = list_owner;
    listing.cover = https(string(&info["cover"]));
    listing.published = number(&info["ctime"]).map(|n| n.to_string());
    let mut cursor = String::new();
    let mut pager = Pagination::default();
    for page in 1..=MAX_PAGES {
        client.report(format!("正在读取列表 {id} 第 {page} 页"))?;
        let value = client
            .api(
                "https://api.bilibili.com/x/v2/medialist/resource/list",
                &[
                    ("type", kind.to_string()),
                    ("oid", cursor.clone()),
                    ("otype", "2".into()),
                    ("biz_id", id.to_string()),
                    ("with_current", "true".into()),
                    ("mobi_app", "web".into()),
                    ("ps", "20".into()),
                    ("direction", "false".into()),
                    ("sort_field", "1".into()),
                    ("tid", "0".into()),
                    ("desc", (kind == 5).to_string()),
                ],
            )
            .await?;
        check_api(&value)?;
        let rows = value["data"]["media_list"]
            .as_array()
            .ok_or("列表接口没有返回视频")?;
        let more = value["data"]["has_more"].as_bool().unwrap_or(false);
        pager.check(rows, more)?;
        // Advance even when the final media is unavailable, otherwise pagination can loop.
        let next = rows
            .last()
            .and_then(|row| number(&row["id"]))
            .map(|id| id.to_string())
            .unwrap_or_default();
        if more && (next.is_empty() || next == cursor) {
            return Err("列表分页游标没有前进".into());
        }
        cursor = next;
        listing.videos.extend(rows.iter().cloned());
        if !more {
            return Ok(listing);
        }
    }
    Err("列表分页超出上限".into())
}

async fn expand(client: &BiliClient, mut listing: Listing) -> Result<Content, String> {
    let mut parts = Vec::new();
    let mut aids = HashSet::new();
    let total = listing.videos.len();
    for (index, item) in listing.videos.into_iter().enumerate() {
        client.report(format!("正在展开列表视频 {} / {total}", index + 1))?;
        if number(&item["type"]).is_some_and(|kind| kind != 2) {
            continue;
        }
        let aid = number(&item["aid"]).or_else(|| number(&item["id"]));
        if number(&item["attr"]).is_some_and(|flags| flags != 0)
            || string(&item["title"]).contains("已失效")
        {
            listing
                .warnings
                .push(format!("已跳过失效条目 AV{}", aid.unwrap_or(0)));
            continue;
        }
        let id = if !string(&item["bvid"]).is_empty() {
            VideoId::Bvid(string(&item["bvid"]).into())
        } else if let Some(aid) = aid {
            VideoId::Aid(aid)
        } else {
            listing.warnings.push("已跳过缺少视频 ID 的条目".into());
            continue;
        };
        let key = match &id {
            VideoId::Aid(id) => format!("av{id}"),
            VideoId::Bvid(id) => id.clone(),
        };
        if !aids.insert(key) {
            continue;
        }
        match client.resolve_video(&id).await {
            Ok(video) => {
                let multi = video.parts.len() > 1;
                for mut part in video.parts {
                    part.title = if multi {
                        format!("{} · P{} {}", video.title, part.number, part.title)
                    } else {
                        video.title.clone()
                    };
                    part.number = parts.len() as u32 + 1;
                    parts.push(part);
                }
            }
            Err(error) => listing
                .warnings
                .push(format!("{}: {error}", string(&item["title"]))),
        }
    }
    if parts.is_empty() {
        return Err(format!(
            "列表中没有可访问的视频。{}",
            listing.warnings.join("；")
        ));
    }
    if listing.cover.is_empty() {
        listing.cover = parts
            .first()
            .and_then(|p| p.source.as_ref())
            .map(|s| s.cover.clone())
            .unwrap_or_default();
    }
    Ok(Content {
        title: listing.title,
        description: listing.description,
        cover: listing.cover,
        owner: listing.owner,
        publish_time: listing.published,
        kind: ContentKind::VideoList,
        parts,
        warnings: listing.warnings,
    })
}

#[derive(Default)]
struct Pagination {
    pages: HashSet<String>,
}
impl Pagination {
    fn check(&mut self, rows: &[Value], more: bool) -> Result<(), String> {
        if more && rows.is_empty() {
            return Err("列表接口声明还有下一页，但没有返回条目".into());
        }
        let identity = rows
            .iter()
            .map(|row| row["aid"].to_string() + &row["id"].to_string() + string(&row["bvid"]))
            .collect::<Vec<_>>()
            .join(",");
        if !rows.is_empty() && !self.pages.insert(identity) {
            return Err("列表接口重复返回同一页，已停止以避免遗漏或循环".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bilibili::test_support::{view_fixture, JsonServer};
    use serde_json::json;
    #[tokio::test]
    #[ignore = "requires public collection and space APIs"]
    async fn live_collection_resolves_all_entries() {
        let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
        let content = client
            .resolve("https://space.bilibili.com/392959666/lists/1560264?type=season")
            .await
            .unwrap();
        assert!(matches!(content.kind, ContentKind::VideoList));
        assert!(
            content.parts.len() > 30,
            "collection should expand beyond the first page"
        );
        assert!(content
            .parts
            .iter()
            .all(|p| p.source.is_some() && p.watch_url.is_some()));
    }
    #[tokio::test]
    #[ignore = "requires the live space API; anonymous requests may require browser validation"]
    async fn live_space_metadata_or_validation_response() {
        let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
        let response = client
            .signed_api(
                "/x/space/wbi/arc/search",
                &[
                    ("mid", "392959666".into()),
                    ("pn", "1".into()),
                    ("ps", "30".into()),
                    ("order", "pubdate".into()),
                    ("tid", "0".into()),
                ],
            )
            .await;
        match response.and_then(|value| {
            check_api(&value)?;
            Ok(value)
        }) {
            Ok(value) => assert!(!value["data"]["list"]["vlist"]
                .as_array()
                .unwrap()
                .is_empty()),
            Err(error) => assert!(
                error.contains("导入 Cookie")
                    && (error.contains("412")
                        || error.contains("429")
                        || error.contains("-352")
                        || error.contains("-799")),
                "{error}"
            ),
        }
    }
    #[tokio::test]
    async fn favorites_page_deduplication_invalid_items_and_multi_parts_keep_source_metadata() {
        let server=JsonServer::new(|url,_,_|{
            let query=url.query_pairs().collect::<BTreeMap<_,_>>();
            match url.path() {
                "/x/v3/fav/resource/list"=>{
                    let page=query["pn"].parse::<u64>().unwrap();
                    let rows=match page {
                        1=>vec![json!({"id":1,"type":2,"attr":0,"title":"first"})],
                        2=>vec![json!({"id":1,"type":2,"attr":0}),json!({"id":2,"type":2,"attr":9}),json!({"id":3,"type":2,"attr":0,"title":"private"})],
                        _=>vec![json!({"id":4,"type":2,"attr":0})],
                    };
                    json!({"code":0,"data":{"info":{"title":"folder","media_count":70,"upper":{"mid":123,"name":"folder owner"}},"has_more":page<3,"medias":rows}})
                },
                "/x/web-interface/view"=>{
                    let aid=query["aid"].parse::<u64>().unwrap();
                    if aid==3 { json!({"code":-404,"message":"unavailable"}) } else { view_fixture(aid) }
                },
                _=>panic!("unexpected fixture endpoint"),
            }
        }).await;
        let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
        client.api_origin = Some(server.url.clone());
        let content = client
            .resolve("https://space.bilibili.com/123/favlist?fid=456")
            .await
            .unwrap();
        assert_eq!(content.parts.len(), 3);
        assert_eq!(
            content.parts.iter().map(|p| p.number).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            content.parts.iter().map(|p| p.cid).collect::<Vec<_>>(),
            vec![10, 11, 40]
        );
        assert_eq!(content.warnings.len(), 2);
        let source = content.parts[2].source_content(&content);
        assert_eq!(source.cover, "https://example.com/4.jpg");
        assert_eq!(source.owner.unwrap().name.as_deref(), Some("author 4"));
        assert_eq!(source.description, "description 4");
    }

    #[tokio::test]
    async fn space_collection_and_series_follow_page_totals_and_sign_space_requests() {
        let server=JsonServer::new(|url,_,_|{
            let query=url.query_pairs().collect::<BTreeMap<_,_>>();
            let rows=|page:&str| if page=="1" { vec![json!({"aid":1,"author":"fixture"})] } else { vec![json!({"aid":4,"author":"fixture"})] };
            match url.path() {
                "/x/web-interface/nav"=>json!({"code":0,"data":{"wbi_img":{"img_url":"https://example.com/7cd084941338484aae1ad9425b84077c.png","sub_url":"https://example.com/4932caff0ff746eab6f01bf08b70ac45.png"}}}),
                "/x/space/wbi/arc/search"=>{assert!(query.contains_key("w_rid"));json!({"code":0,"data":{"page":{"count":31},"list":{"vlist":rows(&query["pn"])}}})},
                "/x/polymer/web-space/seasons_archives_list"=>json!({"code":0,"data":{"meta":{"name":"collection"},"page":{"total":31},"archives":rows(&query["page_num"])}}),
                "/x/series/series"=>json!({"code":0,"data":{"meta":{"name":"series","mid":123,"total":31}}}),
                "/x/series/archives"=>json!({"code":0,"data":{"page":{"total":31},"archives":rows(&query["pn"])}}),
                "/x/web-interface/view"=>view_fixture(query["aid"].parse().unwrap()),
                _=>panic!("unexpected fixture endpoint"),
            }
        }).await;
        for input in [
            "space:123",
            "https://space.bilibili.com/123/lists/456?type=season",
            "https://space.bilibili.com/123/lists/789?type=series",
        ] {
            let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
            client.api_origin = Some(server.url.clone());
            let content = client.resolve(input).await.unwrap();
            assert_eq!(content.parts.len(), 3);
            assert!(matches!(content.kind, ContentKind::VideoList));
            assert_eq!(content.parts[2].aid, 4);
        }
    }

    #[tokio::test]
    async fn legacy_cursor_advances_past_unavailable_final_media() {
        let server=JsonServer::new(|url,_,_|{
            let query=url.query_pairs().collect::<BTreeMap<_,_>>();
            match url.path() {
                "/x/polymer/web-space/seasons_archives_list"=>json!({"code":-404,"message":"legacy list"}),
                "/x/v1/medialist/info"=>json!({"code":0,"data":{"title":"legacy","upper":{"mid":123}}}),
                "/x/v2/medialist/resource/list"=>{
                    if query["oid"].is_empty() { json!({"code":0,"data":{"has_more":true,"media_list":[{"id":1,"attr":0},{"id":2,"attr":1}]}}) }
                    else { assert_eq!(query["oid"],"2");json!({"code":0,"data":{"has_more":false,"media_list":[{"id":4,"attr":0}]}}) }
                },
                "/x/web-interface/view"=>view_fixture(query["aid"].parse().unwrap()),
                _=>panic!("unexpected fixture endpoint"),
            }
        }).await;
        let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
        client.api_origin = Some(server.url.clone());
        let content = client
            .resolve("https://space.bilibili.com/123/lists/456?type=season")
            .await
            .unwrap();
        assert_eq!(content.parts.len(), 3);
        assert_eq!(content.warnings.len(), 1);
    }
    #[test]
    fn recognizes_current_and_legacy_list_links_before_video_query_ids() {
        assert_eq!(
            identify_input("https://space.bilibili.com/123/favlist?fid=456").unwrap(),
            Some(ListId::Favorites {
                fid: Some(456),
                mid: Some(123)
            })
        );
        assert_eq!(
            identify_input("https://space.bilibili.com/123/lists/456?type=series").unwrap(),
            Some(ListId::Series {
                id: 456,
                mid: Some(123)
            })
        );
        assert_eq!(identify_input("https://www.bilibili.com/medialist/play/123?business=space_collection&business_id=456&bvid=BV17x411w7KC").unwrap(),Some(ListId::Collection {id:456,mid:Some(123)}));
        assert_eq!(
            identify_input("space:123").unwrap(),
            Some(ListId::Space(123))
        );
        assert_eq!(
            identify_input("favId::123").unwrap(),
            Some(ListId::Favorites {
                fid: None,
                mid: Some(123)
            })
        );
        assert_eq!(
            identify_input("https://space.bilibili.com/123/video").unwrap(),
            Some(ListId::Space(123))
        );
        assert_eq!(
            identify_input("https://www.bilibili.com/video/BV17x411w7KC").unwrap(),
            None
        );
        assert!(identify_input("fav:0").is_err());
        assert_eq!(
            identify_input("https://www.bilibili.com/video/BV17x411w7KC?fid=456").unwrap(),
            None
        );
        assert_eq!(
            identify_input("https://www.bilibili.com/medialist/play/ml456").unwrap(),
            Some(ListId::Favorites {
                fid: Some(456),
                mid: None
            })
        );
    }
    #[test]
    fn pagination_detects_empty_and_repeated_pages() {
        let mut pager = Pagination::default();
        let rows = vec![serde_json::json!({"aid":1})];
        pager.check(&rows, true).unwrap();
        assert!(pager.check(&rows, true).is_err());
        assert!(Pagination::default().check(&[], true).is_err());
    }
}
