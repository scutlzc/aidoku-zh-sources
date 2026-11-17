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

fn jarray(value: &Value, key: &str) -> Vec<Value> {
	value
		.get(key)
		.and_then(|value| value.as_array())
		.map(|list| list.to_vec())
		.unwrap_or_default()
}

pub fn has_more(data: &Value) -> bool {
	let page = jint(data, "page").unwrap_or_default();
	let pages = jint(data, "pages").unwrap_or_default();

	pages > page
}

pub fn parse_manga_list(list: &[Value]) -> Vec<Manga> {
	list.iter().map(parse_manga).collect()
}

pub fn parse_manga(manga: &Value) -> Manga {
	let manga = manga.get("comic").unwrap_or(manga);
	let key = jstr(manga, "_id");
	let thumb = manga.get("thumb").unwrap_or(&Value::Null);
	let host = jstr(thumb, "fileServer");
	let path = jstr(thumb, "path");
	let cover = format!("{}/static/{}", host, path);
	let title = jstr(manga, "title");
	let authors: Vec<String> = jstr(manga, "author")
		.split('&')
		.map(|author| author.trim().to_string())
		.collect::<Vec<String>>();
	let description = jstr(manga, "description");
	let url = helper::gen_manga_url(key.clone());
	let tags: Vec<String> = jarray(manga, "categories")
		.iter()
		.map(|category| category.as_str().unwrap_or_default().to_string())
		.collect::<Vec<String>>();
	let status = if manga
		.get("finished")
		.and_then(|finished| finished.as_bool())
		.unwrap_or_default()
	{
		MangaStatus::Completed
	} else {
		MangaStatus::Ongoing
	};
	let viewer = if tags.contains(&String::from("WEBTOON")) {
		Viewer::Webtoon
	} else {
		Viewer::RightToLeft
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
		content_rating: ContentRating::NSFW,
		viewer,
		..Default::default()
	}
}

pub fn parse_chapter_list(manga_id: String, list: &[Value]) -> Vec<Chapter> {
	let mut chapters: Vec<Chapter> = Vec::new();

	for item in list.iter() {
		let order = jint(item, "order").unwrap_or_default();
		let key = order.to_string();
		let title = jstr(item, "title");
		let chapter = order as f32;
		let url = helper::gen_chapter_url(manga_id.clone(), key.clone());
		chapters.push(Chapter {
			key,
			title: Some(title),
			chapter_number: Some(chapter),
			url: Some(url),
			..Default::default()
		});
	}

	chapters
}

pub fn parse_page_list(list: &[Value]) -> Vec<Page> {
	let mut pages: Vec<Page> = Vec::new();

	for item in list.iter() {
		let media = item.get("media").unwrap_or(&Value::Null);
		let host = jstr(media, "fileServer");
		let path = jstr(media, "path");
		let url = format!("{}/static/{}", host, path);
		pages.push(Page {
			content: PageContent::url(url),
			..Default::default()
		});
	}

	pages
}
