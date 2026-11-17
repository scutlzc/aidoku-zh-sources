use aidoku::{
	alloc::{format, string::String, string::ToString, vec, vec::Vec},
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

pub fn parse_manga_list(list: &[Value]) -> Vec<Manga> {
	list.iter().map(parse_manga).collect()
}

pub fn parse_manga(manga: &Value) -> Manga {
	let key = jint(manga, "mangaId")
		.map(|id| id.to_string())
		.unwrap_or_default();
	let cover = manga
		.get("mangaCoverimageUrl")
		.and_then(|cover| cover.as_str())
		.unwrap_or_default();
	let cover = manga
		.get("mangaPicimageUrl")
		.and_then(|cover| cover.as_str())
		.unwrap_or(cover)
		.to_string();
	let title = jstr(manga, "mangaName");
	let author = jstr(manga, "mangaAuthor").trim().to_string();
	let description = jstr(manga, "mangaIntro");
	let url = jstr(manga, "shareUrl");
	let tags = jstr(manga, "mangaTheme")
		.split(' ')
		.map(|category| category.to_string())
		.filter(|category| !category.is_empty())
		.collect::<Vec<String>>();
	let status = match jint(manga, "mangaIsOver").unwrap_or(-1) {
		0 => MangaStatus::Ongoing,
		1 => MangaStatus::Completed,
		_ => MangaStatus::Unknown,
	};

	Manga {
		key,
		cover: if cover.is_empty() { None } else { Some(cover) },
		title,
		authors: if author.is_empty() {
			None
		} else {
			Some(vec![author])
		},
		description: if description.is_empty() {
			None
		} else {
			Some(description)
		},
		url: if url.is_empty() { None } else { Some(url) },
		tags: if tags.is_empty() { None } else { Some(tags) },
		status,
		content_rating: ContentRating::Safe,
		viewer: Viewer::RightToLeft,
		..Default::default()
	}
}

pub fn parse_chapter_list(manga: &Value) -> Vec<Chapter> {
	let mut chapters: Vec<Chapter> = Vec::new();
	chapters.append(&mut parse_chapters(&jarray(manga, "mangaWords")));
	chapters.append(&mut parse_chapters(&jarray(manga, "mangaRolls")));
	chapters.append(&mut parse_chapters(&jarray(manga, "mangaEpisode")));
	chapters.sort_by_key(|a| a.chapter_number.unwrap_or_default().to_bits());
	chapters.reverse();
	chapters
}

pub fn parse_chapters(chapter_list: &[Value]) -> Vec<Chapter> {
	let mut chapters: Vec<Chapter> = Vec::new();

	for item in chapter_list {
		let key = jint(item, "sectionId")
			.map(|id| id.to_string())
			.unwrap_or_default();
		let section_title = jstr(item, "sectionTitle");
		let section_name = jstr(item, "sectionName");
		let is_must_pay = jint(item, "isMustPay").unwrap_or_default();
		let title = if section_title.is_empty() {
			section_name
		} else if is_must_pay == 0 {
			format!("{} {}", section_name, section_title)
		} else {
			format!("{} {} {}", "🔒", section_name, section_title)
		};
		let chapter = item
			.get("sectionSort")
			.and_then(|sort| {
				sort.as_f64()
					.or_else(|| sort.as_i64().map(|sort| sort as f64))
			})
			.unwrap_or_default() as f32;
		let url = helper::gen_chapter_url(key.clone());
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

pub fn parse_page_list(chapter: &Value) -> Vec<Page> {
	let list = jarray(chapter, "mangaSectionImages");
	let host_list = jarray(chapter, "hostList");
	let query = jstr(chapter, "query");
	let mut pages: Vec<Page> = Vec::new();

	if host_list.is_empty() {
		return pages;
	}

	let host = host_list
		.first()
		.and_then(|host| host.as_str())
		.unwrap_or_default();

	for item in list.iter() {
		let item = item.as_str().unwrap_or_default();
		pages.push(Page {
			content: PageContent::url(format!("{}{}{}", host, item, query)),
			..Default::default()
		});
	}

	pages
}
