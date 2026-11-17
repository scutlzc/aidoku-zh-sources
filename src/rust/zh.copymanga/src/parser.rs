use aidoku::{
	alloc::{format, string::String, string::ToString, vec::Vec},
	Chapter, ContentRating, Manga, MangaStatus, Page, PageContent, Viewer,
};
use serde_json::Value;

use crate::helper;

fn jstr(value: &Value, key: &str) -> String {
	value
		.get(key)
		.and_then(|value| value.as_str())
		.unwrap_or_default()
		.to_string()
}

fn jint(value: &Value, key: &str) -> Option<i64> {
	value.get(key).and_then(|value| {
		value
			.as_i64()
			.or_else(|| value.as_f64().map(|value| value as i64))
	})
}

fn hex_nibble(byte: u8) -> Option<u64> {
	match byte {
		b'0'..=b'9' => Some((byte - b'0') as u64),
		b'a'..=b'f' => Some((byte - b'a' + 10) as u64),
		b'A'..=b'F' => Some((byte - b'A' + 10) as u64),
		_ => None,
	}
}

fn hex_u64(text: &str) -> Option<u64> {
	if text.is_empty() {
		return None;
	}

	let mut value: u64 = 0;

	for byte in text.as_bytes() {
		value = value.checked_mul(16)? + hex_nibble(*byte)?;
	}

	Some(value)
}

// Unix seconds from a UUID v1 timestamp (100ns ticks since 1582-10-15).
fn uuid_v1_unix(id: &str) -> Option<i64> {
	let mut parts = id.split('-');
	let time_low = hex_u64(parts.next()?)?;
	let time_mid = hex_u64(parts.next()?)?;
	let time_hi = hex_u64(parts.next()?)? & 0x0fff;
	let ticks = (time_hi << 48) | (time_mid << 32) | time_low;
	let unix_ticks = ticks.checked_sub(122_192_928_000_000_000)?;

	Some((unix_ticks / 10_000_000) as i64)
}

pub fn has_more(data: &Value) -> bool {
	let total = jint(data, "total").unwrap_or_default();
	let limit = jint(data, "limit").unwrap_or_default();
	let offset = jint(data, "offset").unwrap_or_default();

	total > limit + offset
}

pub fn parse_manga_list(list: Option<&Value>) -> Vec<Manga> {
	match list.and_then(|list| list.as_array()) {
		Some(list) => list.iter().map(parse_manga).collect(),
		None => Vec::new(),
	}
}

pub fn parse_manga(manga: &Value) -> Manga {
	let manga = manga.get("comic").unwrap_or(manga);
	let key = jstr(manga, "path_word");
	let cover = jstr(manga, "cover");
	let title = jstr(manga, "name");
	let authors: Vec<String> = manga
		.get("author")
		.and_then(|authors| authors.as_array())
		.map(|authors| {
			authors
				.iter()
				.filter_map(|author| {
					author
						.get("name")
						.and_then(|name| name.as_str())
						.map(|name| name.to_string())
				})
				.collect::<Vec<String>>()
		})
		.unwrap_or_default();
	let description = jstr(manga, "brief").trim().to_string();
	let url = helper::gen_manga_url(key.clone());
	let tags: Vec<String> = manga
		.get("theme")
		.and_then(|themes| themes.as_array())
		.map(|themes| {
			themes
				.iter()
				.filter_map(|theme| {
					theme
						.get("name")
						.and_then(|name| name.as_str())
						.map(|name| name.to_string())
				})
				.collect::<Vec<String>>()
		})
		.unwrap_or_default();
	let status = match jint(manga.get("status").unwrap_or(&Value::Null), "value").unwrap_or(-1) {
		0 => MangaStatus::Ongoing,
		1 => MangaStatus::Completed,
		_ => MangaStatus::Unknown,
	};
	let content_rating =
		match jint(manga.get("restrict").unwrap_or(&Value::Null), "value").unwrap_or(-1) {
			0 => ContentRating::Safe,
			1 => ContentRating::Suggestive,
			2 => ContentRating::NSFW,
			3 => ContentRating::NSFW,
			4 => ContentRating::NSFW,
			_ => ContentRating::Safe,
		};

	Manga {
		key,
		cover: if cover.is_empty() { None } else { Some(cover) },
		title,
		authors: if authors.is_empty() {
			None
		} else {
			Some(authors)
		},
		description: if description.is_empty() {
			None
		} else {
			Some(description)
		},
		url: Some(url),
		tags: if tags.is_empty() { None } else { Some(tags) },
		status,
		content_rating,
		viewer: Viewer::RightToLeft,
		..Default::default()
	}
}

pub fn parse_chapter_group(
	manga_id: String,
	group: &Value,
	name: String,
	start: usize,
) -> Vec<Chapter> {
	let list = match group.get("chapters").and_then(|list| list.as_array()) {
		Some(list) => list,
		None => return Vec::new(),
	};
	let mut chapters: Vec<Chapter> = Vec::new();

	for (index, item) in list.iter().enumerate() {
		let id = jstr(item, "id");
		let title = format!("{} - {}", name, jstr(item, "name"));
		let chapter = (index + start + 1) as f32;
		let date_uploaded = uuid_v1_unix(&id);
		let url = helper::gen_chapter_url(manga_id.clone(), id.clone());
		chapters.push(Chapter {
			key: id,
			title: Some(title),
			chapter_number: Some(chapter),
			date_uploaded,
			url: Some(url),
			..Default::default()
		});
	}

	chapters
}

pub fn parse_chapter_list(manga: &Value) -> Vec<Chapter> {
	let empty = Value::Null;
	let build = manga.get("build").unwrap_or(&empty);
	let manga_id = jstr(build, "path_word");
	let groups = manga.get("groups").unwrap_or(&empty);
	let default_group = groups.get("default").unwrap_or(&empty);
	let tankobon_group = groups.get("tankobon").unwrap_or(&empty);
	let other_honyakuchimu_group = groups.get("other_honyakuchimu").unwrap_or(&empty);
	let karapeji_group = groups.get("karapeji").unwrap_or(&empty);
	let default = parse_chapter_group(manga_id.clone(), default_group, String::from("默认"), 0);
	let tankobon = parse_chapter_group(
		manga_id.clone(),
		tankobon_group,
		String::from("单行本"),
		default.len(),
	);
	let other_honyakuchimu = parse_chapter_group(
		manga_id.clone(),
		other_honyakuchimu_group,
		String::from("其它汉化版"),
		default.len() + tankobon.len(),
	);
	let karapeji = parse_chapter_group(
		manga_id.clone(),
		karapeji_group,
		String::from("全彩版"),
		default.len() + tankobon.len() + other_honyakuchimu.len(),
	);
	let mut chapters = [default, tankobon, other_honyakuchimu, karapeji].concat();

	chapters.reverse();
	chapters
}

pub fn parse_page_list(pages: &Value) -> Vec<Page> {
	let list = match pages.as_array() {
		Some(list) => list,
		None => return Vec::new(),
	};
	let mut result: Vec<Page> = Vec::new();

	for item in list.iter() {
		let url = jstr(item, "url");
		result.push(Page {
			content: PageContent::url(url),
			..Default::default()
		});
	}

	result
}
