use std::collections::BTreeSet;

pub fn select_pages(selection: &str, count: u32) -> Result<Vec<u32>, String> {
    if count == 0 {
        return Err("没有可下载的分 P".to_string());
    }
    let selection = selection.trim().to_ascii_uppercase();
    if selection == "ALL" {
        return Ok((1..=count).collect());
    }
    let mut pages = BTreeSet::new();
    for item in selection.split(',').map(str::trim) {
        if matches!(item, "LAST" | "LATEST") {
            pages.insert(count);
        } else if let Some((start, end)) = item.split_once('-') {
            let start = number(start, count)?;
            let end = number(end, count)?;
            if start > end {
                return Err(format!("分 P 范围顺序错误: {item}"));
            }
            pages.extend(start..=end);
        } else {
            pages.insert(number(item, count)?);
        }
    }
    Ok(pages.into_iter().collect())
}

fn number(value: &str, count: u32) -> Result<u32, String> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0 && *n <= count)
        .ok_or_else(|| format!("无效分 P: {value}，可选范围为 1-{count}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_and_deduplicates_ranges() {
        assert_eq!(select_pages("3,1-2,2,LATEST", 5).unwrap(), vec![1, 2, 3, 5]);
        assert_eq!(select_pages("all", 3).unwrap(), vec![1, 2, 3]);
        for value in ["", "0", "4", "3-1", "ALL,1", "x"] {
            assert!(select_pages(value, 3).is_err(), "{value}");
        }
    }
}
