# 迁移规范：旧 aidoku-rs 宏 API → 新版 `Source` trait API

本仓库所有 `src/rust/*` 图源原本基于 aidoku-rs 的**旧宏 API**（`#[get_manga_list]` 等，Aidoku 0.6 时代）。
新版 Aidoku（0.7+，AidokuRunner）只认 **WASM + trait** 形式。本文是逐个源迁移时必须遵守的规范。

已完成的参考实现（直接照着写，不要自创风格）：

- `src/rust/zh.18mh`：纯 HTML 源（列表 + 详情 + 章节 + 图片 + 列表页 + 图片请求头）
- `src/rust/zh.komiic`：JSON/GraphQL 源（serde_json + 设置项 Cookie + 图片请求头）

---

## 0. 构建与工具链

工具链**不在** PATH 里，每次都要先导出：

```sh
export PATH=/Volumes/linzc/rust-1.99.1/bin:$PATH
```

单源构建（`<DIR>` 为该源目录，`<TARGET>` 用自己专属的 target 目录避免互锁）：

```sh
cd /Volumes/linzc/Codes/aidoku-zh-sources/src/rust/<DIR>
rm -f Cargo.lock
CARGO_TARGET_DIR=<TARGET> cargo build --release --target wasm32-unknown-unknown
```

产物：`<TARGET>/wasm32-unknown-unknown/release/aidoku_<name>.wasm`。
**要求 0 warning、0 error 才算完成。**

`Cargo.toml` 的依赖行统一改成（普通源）：

```toml
[dependencies]
aidoku = { git = "https://github.com/Aidoku/aidoku-rs" }
```

JSON 源（需要 `json_owned`）：

```toml
[dependencies]
aidoku = { git = "https://github.com/Aidoku/aidoku-rs", features = ["json"] }
serde_json = { version = "1", default-features = false, features = ["alloc"] }
```

`Cargo.lock` 必须删除（旧 lock 指向需要 nightly 的旧 commit）。

---

## 1. 导入路径对照

| 旧 | 新 |
| --- | --- |
| `use aidoku::{error::Result, ...}` | `use aidoku::{..., Result, ...}`（`Result` **必须**出现在 aidoku 的导入列表里，否则 E0107） |
| `aidoku::std::{net::Request, defaults::defaults_get, String, Vec, ValueRef, ObjectRef, ArrayRef}` | `aidoku::imports::{net::Request, defaults::defaults_get}` + `aidoku::alloc::{String, Vec}` |
| `use alloc::string::ToString;` | `use aidoku::alloc::string::ToString;`（也可写在 `aidoku::alloc::{...}` 里） |
| `vec![...]` 宏 | 必须 `use aidoku::alloc::vec;`（no_std 下 `vec!` 不自动可见） |
| `format!` / `println!` / `bail!` / `error!` | 由 `use aidoku::prelude::*;` 提供（`prelude` **只**有 `format!` 和这些宏，没有 String/Vec/Result） |

`#![no_std]` 保留；`extern crate alloc;` 可以删掉。

---

## 2. 函数 → trait 对照

旧源是 6 个自由函数 + 属性宏；新源是 `struct XxxSource;` 实现若干 trait，最后
`register_source!(XxxSource, ...)` 列出除 `Source` 之外实现了的 trait。

| 旧函数 | 新实现 |
| --- | --- |
| `#[get_manga_list] fn get_manga_list(filters: Vec<Filter>, page: i32)` | `Source::get_search_manga_list(&self, query: Option<String>, page: i32, filters: Vec<FilterValue>)` |
| `#[get_manga_listing] fn get_manga_listing(listing: Listing, page: i32)` | `ListingProvider::get_manga_list(&self, listing: Listing, page: i32)`（同时要有 `res/source.json` 的 `listings`） |
| `#[get_manga_details] fn get_manga_details(id: String)` + `#[get_chapter_list] fn get_chapter_list(id: String)` | 合并为 `Source::get_manga_update(&self, manga: Manga, needs_details: bool, needs_chapters: bool)`：`needs_details` 分支做原来的详情解析，`needs_chapters` 分支做章节解析，最后 `Ok(manga)` |
| `#[get_page_list] fn get_page_list(manga_id: String, chapter_id: String)` | `Source::get_page_list(&self, manga: Manga, chapter: Chapter)`（用 `manga.key` / `chapter.key`） |
| `#[modify_image_request] fn modify_image_request(request: Request)` | `ImageRequestProvider::get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request>`，内部自行 `Request::get(url)?` 后加同样的头 |
| 没有（新增能力） | `DeepLinkHandler`（能直接由 URL 推出 key 时才加）、`Home`、`DynamicFilters` 等一律**不要**自作主张实现 |

`register_source!` 写法示例：`register_source!(Mh18, ListingProvider, ImageRequestProvider, DeepLinkHandler);`
必须与真正 `impl` 的 trait 完全一致，否则链接期报错。

---

## 3. 数据结构对照

旧（`Manga`/`Chapter`/`Page` 字段）→ 新：

| 旧字段 | 新字段 |
| --- | --- |
| `Manga.id: String` | `Manga.key: String`（**值保持不变**，同一个字符串） |
| `Manga.cover: String` | `Manga.cover: Option<String>` |
| `Manga.author: String` | `Manga.authors: Option<Vec<String>>`；旧的 `artist` → `Manga.artists: Option<Vec<String>>` |
| `Manga.description/url/categories` | `description: Option<String>` / `url: Option<String>` / `tags: Option<Vec<String>>` |
| `Manga.nsfw: MangaContentRating` | `content_rating: ContentRating`（`Safe` / `Suggestive` / `NSFW`） |
| `Manga.viewer: MangaViewer` | `viewer: Viewer`（`Scroll`→`Vertical` 用 `Viewer::Webtoon`；`Rtl`→`Viewer::RightToLeft`；`Ltr`→`Viewer::LeftToRight`） |
| `Manga.status: MangaStatus` | 同名 `MangaStatus`（`Ongoing`/`Completed`/`Cancelled`/`Hiatus`/`Unknown`） |
| `MangaPageResult { manga, has_more }` | `MangaPageResult { entries, has_next_page }` |
| `Chapter { id, title: String, chapter: f32, url: String, volume, scanlator }` | `Chapter { key: String, title: Option<String>, chapter_number: Option<f32>, volume_number: Option<f32>, scanlators: Option<Vec<String>>, url: Option<String>, date_uploaded: Option<i64>, ..Default::default() }` |
| `Page { index: i32, url: String }` | `Page { content: PageContent::url(url), ..Default::default() }` |

所有结构体都实现了 `Default`，固定用 `..Default::default()` 收尾。
旧代码里 `chapter: -1.0` / `volume: -1.0` 这种哨兵值改成 `None`。

---

## 4. 过滤器对照（重要）

旧 `filters.json` 用中文 `name` + 下标；新 `filters.json` 用 `id` + `options` + **`ids`**：

```json
[
  {
    "type": "select",
    "id": "category",
    "title": "类型",
    "options": ["全部", "韩漫", "真人写真"],
    "ids": ["", "hanman", "zhenrenxiezhen"]
  },
  {
    "type": "sort",
    "id": "sort",
    "title": "排序",
    "canAscend": false,
    "options": ["更新", "观看数"],
    "default": { "index": 0, "ascending": false }
  }
]
```

规则：

1. **删掉 `{"type":"title"}` 文本过滤器**。新版搜索框已经把关键词作为 `query: Option<String>` 传进来，不需要再声明 text 过滤器。
2. `select` 一律给出 `ids`，`ids[i]` 就是站点真实取值（原来是 `FILTER_XXX[index]` 数组里的那串）。这样 Rust 侧直接 `FilterValue::Select { id, value }` 拿 `value` 用，**不再按下标查数组**；对应的旧 `const FILTER_XXX: [&str; N]` 如果只被过滤器使用，可以删掉；如果别处也用（如 listing 路径），保留。
3. `sort` 没有 `ids`，仍按 `index` 查站点排序值数组（保留 `const FILTER_ORDER_BY`）。
4. Rust 侧读取：

```rust
for filter in filters {
    match filter {
        FilterValue::Select { id, value } => {
            if id == "category" {
                category = value;
            }
        }
        FilterValue::Sort { index, .. } => {
            order_by = FILTER_ORDER_BY.get(index as usize).copied().unwrap_or_default().to_string();
        }
        FilterValue::Text { id, value } => { /* 只有确实需要 text 过滤器时才用 */ }
        _ => continue,
    }
}
```

`FilterValue::Select { id, value }` 里 `id`/`value` 都是 `String`，`id == "category"` 可以直接比较。

---

## 5. 资源文件格式

### res/source.json

```json
{
  "info": {
    "id": "zh.18mh",
    "name": "18漫画",
    "version": 2,
    "url": "https://18mh.org",
    "contentRating": 2,
    "languages": ["zh"]
  },
  "listings": [
    { "id": "hots", "name": "人气推荐" }
  ]
}
```

- `lang` → `languages: ["zh"]`
- `nsfw: 0|2` → `contentRating: 0|2`（0 安全、1 部分、2 主要 NSFW）
- `version` 在原值上 **+1**
- `listings` 每项必须补 `id`（旧格式只有 `name`）。id 用简短的英文/拼音标识，Rust 里按 `listing.id` 分支；`name` 保持旧的中文显示名。
- 不要写 `deprecated`（废弃源已删除）。

### res/filters.json

见上一节。字段名是 camelCase：`canAscend`、`isGenre`、`usesTagStyle`、`hideFromHeader`。

### res/settings.json（只有部分源有）

旧：

```json
[{ "type": "group", "title": "Cookie", "items": [{ "type": "text", "key": "cookie", "placeholder": "...", "default": "" }] }]
```

新（`group` 必须有 `key`；每个 `text` 也必须有 `title`）：

```json
[
  {
    "type": "group",
    "key": "cookie-group",
    "title": "Cookie",
    "items": [
      { "type": "text", "key": "cookie", "title": "Cookie", "placeholder": "...", "default": "" }
    ]
  }
]
```

设置项读取统一改成：

```rust
let cookie = defaults_get::<String>("cookie").unwrap_or_default();
// 需要非空判断时
if let Some(cookie) = defaults_get::<String>("cookie").filter(|c| !c.is_empty()) { ... }
```

旧 `.as_string()?.read()` / `defaults_get("k").and_then(|v| v.as_string())` 一律换成上面的写法。

---

## 6. HTML 解析对照

旧 API 是 lazy `read()` + `.array()`；新 API 全部返回 `Option`：

| 旧 | 新 |
| --- | --- |
| `html.select(".x").array()` | `html.select(".x")` → `Option<ElementList>`，可以直接 `for el in list`（实现 `Iterator<Item = Element>`） |
| `html.select(".x")` 单个元素 | `html.select_first(".x")` → `Option<Element>` |
| `el.attr("href").read()` | `el.attr("href")` → `Option<String>` |
| `el.text().read()` | `el.text()` → `Option<String>` |
| `el.has_attr("data-src")` | 同名可用，返回 `bool` |
| `el.html().read()` | `el.html()` → `Option<String>` |

典型写法：

```rust
let html = Request::get(&url)?.html()?;
if let Some(items) = html.select(".list > a") {
    for item in items {
        let Some(href) = item.attr("href") else { continue };
        let title = item.select_first("h3").and_then(|t| t.text()).unwrap_or_default();
        ...
    }
}
```

**注意**：`html.select(sel).map(|list| list.map(|e| e.text()).collect::<Vec<_>>())` 是错的——
`Option::map` 拿到的是 `ElementList`，必须写
`html.select(sel).map(|list| list.map(|e| e.text().unwrap_or_default()).collect::<Vec<_>>()).unwrap_or_default()`。

属性值可能是相对路径，源里统一用 `abs_url()` 处理（每个源自带这个小函数，参考 18mh）。

---

## 7. 网络与 JSON

`Request`：

```rust
let html = Request::get(url)?.html()?;                       // 旧 Request::new(url, HttpMethod::Get).html()?
let text = Request::get(url)?.string()?;
let bytes = Request::get(url)?.data()?;
let json: serde_json::Value = Request::post(url)?.body(body.as_bytes()).header("Content-Type", "application/json").json_owned()?;
```

- **`header<T: AsRef<str>>(key: T, val: T)` 两个参数必须是同一个类型**。所以第二个参数统一传 `&str`：
  `request.header("Referer", referer.as_str())`、`request.header("Cookie", cookie.as_str())`。
  直接传 `String` 会报 `expected &str, found String`。
- 旧 `.json()` 返回 `ValueRef`；新的一次性解析成 `serde_json::Value`，用
  `value.get("data")`（→`Option<&Value>`）、`value.as_str()`（→`Option<&str>`）、`value.as_array()`（→`Option<&Vec<Value>>`）、`value.as_i64()`、`value.as_f64()`、`value.as_bool()`。
- 递归/嵌套访问用 `.get("a").and_then(|v| v.get("b"))`。
- `ObjectRef`/`ArrayRef`/`ValueRef`/`.as_object()`/`.read()` 全部不存在，逐个改写。
- JSON 源需在 `Cargo.toml` 打开 `aidoku` 的 `json` feature 并加 `serde_json`（见 §0）。

---

## 8. 其余已知坑

1. `aidoku::std::...` 模块不存在了，别沿用旧路径。
2. `Result<T>` 单参数形式来自 `use aidoku::Result`，漏了会 E0107。
3. `vec![]` 需要 `use aidoku::alloc::vec;`。
4. 旧 `helpers::substring::Substring`（`substring_after` / `substring_before`）已删除，在源内自建：

```rust
fn substring_after<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value.find(pattern).map(|index| &value[index + pattern.len()..])
}
fn substring_before<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value.find(pattern).map(|index| &value[..index])
}
```

5. 变量移动：`String` 不可 `Copy`，`.header(..., x.as_str())` 之后还要用时用 `&`/`clone()`。
6. `Chapter.key` / `Manga.key` 的**取值一定要和旧代码的 id 完全一致**（不能加前缀、不能改分隔符），否则用户库里的漫画会失联。
7. 旧代码里 `chapter: index as f32 + 1.0` 这类编号照旧算，只是包成 `Some(...)`。
8. 不要引入新的第三方 crate（除必要时的 `serde_json`）。
9. 保留原有的所有站点常量（`WWW_URL`、`API_URL`、`User-Agent` 等）、选择器、请求参数——只做 API 形态转换，不"优化"逻辑。

---

## 9. 交付要求（子任务）

对每个分配到的源：

1. 读 `src/rust/<id>/src/*.rs`（可能含 `helper.rs`/`parser.rs`）与 `res/*.json`。
2. 按本规范重写（可以合并成单文件 `src/lib.rs`，也可以保留多文件，只要结构清晰、能编译）。
3. 更新 `Cargo.toml`（依赖行 + 删 `Cargo.lock`）、`res/source.json`、`res/filters.json`、`res/settings.json`。
4. `cargo build --release --target wasm32-unknown-unknown` **必须 0 error 0 warning**。
5. 不动 `public/`、不跑 git 提交、不改其他源。
6. 回报：每个源一行结论（编译结果 + 结构体名 + 实现了哪些 trait + 任何语义上不确定/无法一比一迁移的地方）。
