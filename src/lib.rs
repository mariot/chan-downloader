//! # chan_downloader
//!
//! `chan_downloader` is a collection of utilities to
//! download images/videos from a 4chan thread

use log::info;
use reqwest::{Client, Error, Url};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs::File,
    io::{self, Cursor},
};

/// Represents a 4chan thread
#[derive(Debug)]
pub struct Thread {
    pub board: String,
    pub id:    u32,
}

#[derive(Debug)]
pub struct Link {
    pub url:  String,
    pub name: String,
}

/// Saves the image from the url to the given path.
/// Returns the path on success
///
/// # Examples
///
/// ```
/// use reqwest::Client;
/// use std::{env, fs::remove_file};
/// let client = Client::builder().user_agent("reqwest").build().unwrap();
/// let workpath = env::current_dir().unwrap().join("1745612650141704.png");
/// let url = "https://i.4cdn.org/g/1745612650141704.png";
/// async {
///     let answer = chan_downloader::save_image(url, workpath.to_str().unwrap(), &client)
///         .await
///         .unwrap();
///     assert_eq!(workpath.to_str().unwrap(), answer);
///     remove_file(answer).unwrap();
/// };
/// ```
pub async fn save_image(url: &str, path: &str, client: &Client) -> Result<String, Error> {
    info!(target: "image_events", "Saving image to: {}", path);
    let response = client.get(url).send().await?.error_for_status()?;
    // Receive the whole file before creating it, so a failed transfer doesn't
    // leave a truncated file that later runs would skip
    let mut content = Cursor::new(response.bytes().await?);

    let mut dest = File::create(path).unwrap();
    io::copy(&mut content, &mut dest).unwrap();
    info!("Saved image to: {}", path);
    Ok(String::from(path))
}

/// Returns the page content from the given url.
/// Fails if the server answers with an error status (e.g. 403 or 404)
///
/// # Examples
///
/// ```
/// use reqwest::Client;
/// use std::io;
/// let client = Client::builder().user_agent("reqwest").build().unwrap();
/// let url = "https://raw.githubusercontent.com/mariot/chan-downloader/master/.gitignore";
/// async {
///     let result = chan_downloader::get_page_content(url, &client)
///         .await
///         .unwrap();
///     assert_eq!(result, "/target/\nCargo.lock\n**/*.rs.bk\n");
/// };
/// ```
pub async fn get_page_content(url: &str, client: &Client) -> Result<String, Error> {
    info!(target: "page_events", "Loading page: {}", url);
    let response = client.get(url).send().await?.error_for_status()?;
    let content = response.text().await?;
    info!("Loaded page: {}", url);
    Ok(content)
}

/// Returns the board name and thread id.
///
/// # Examples
///
/// ```
/// let url = "https://boards.4chan.org/wg/thread/6872254";
/// let thread = chan_downloader::get_thread_info(url);
///
/// assert_eq!(thread.board, "wg");
/// assert_eq!(thread.id, 6872254);
/// ```
#[must_use]
pub fn get_thread_info(url: &str) -> Thread {
    info!(target: "thread_events", "Getting thread info from: {}", url);
    let url_vec: Vec<&str> = url.split('/').collect();
    let board_name = url_vec[3];
    let thread_vec: Vec<&str> = url_vec[5].split('#').collect();
    let thread_id = thread_vec[0];
    info!("Got thread info from: {}", url);

    Thread {
        board: board_name.to_owned(),
        id:    thread_id.parse::<u32>().expect("failed to parse thread id"),
    }
}

/// Returns the image/video links from a page, without duplicates and in page
/// order. Supported extensions: jpg, png, gif, webm, mp4 and pdf.
///
/// # Examples
///
/// ```
/// use reqwest::Client;
/// let client = Client::builder().user_agent("reqwest").build().unwrap();
/// let url = "https://boards.4chan.org/wg/thread/6872254";
/// async {
///     match chan_downloader::get_page_content(url, &client).await {
///         Ok(page_string) => {
///             let links_iter = chan_downloader::get_image_links(page_string.as_str());
///
///             for link in links_iter {
///                 println!("{} and {}", link.name, link.url);
///             }
///         },
///         Err(err) => eprintln!("Error: {}", err),
///     }
/// };
/// ```
///
/// Sample image links:
//    - https://i.4pcdn.org/x/1438502659556.jpg
//    - https://img.4plebs.org/boards/x/image/1660/66/1660662319160984.png
//    - https://i.4cdn.org/sp/1661019073822058.jpg
#[must_use]
pub fn get_image_links(page_content: &str) -> Vec<Link> {
    info!(target: "link_events", "Getting image links");
    let reg = regex!(
        r"(//i(?:s|mg)?(?:\d*)?\.(?:4cdn|4chan|4plebs|4pcdn)\.org/(?:\w+/){1,3}(?:\d+/){0,2}(\d+\.(?:jpg|png|gif|webm|mp4|pdf)))"
    );

    // A link appears several times per post: twice for images, three times
    // for videos (extra `<link itemprop="contentUrl">`)
    let mut seen = HashSet::new();
    let links_v: Vec<Link> = reg
        .captures_iter(page_content)
        .filter(|cap| seen.insert(cap[1].to_owned()))
        .map(|cap| Link {
            url:  String::from(&cap[1]),
            name: String::from(&cap[2]),
        })
        .collect();
    info!("Got {} image links from page", links_v.len());
    links_v
}

/// Returns the 4chan API url of a thread, or `None` if the url is not a 4chan
/// thread (e.g. 4plebs)
///
/// # Examples
///
/// ```
/// let url = "https://boards.4chan.org/wg/thread/6872254/sticky-thread";
/// assert_eq!(
///     chan_downloader::get_api_url(url).unwrap(),
///     "https://a.4cdn.org/wg/thread/6872254.json"
/// );
/// assert!(chan_downloader::get_api_url("https://archive.4plebs.org/x/thread/32661196").is_none());
/// ```
#[must_use]
pub fn get_api_url(url: &str) -> Option<String> {
    let host = url.split('/').nth(2)?;
    if host != "boards.4chan.org" && host != "boards.4channel.org" {
        return None;
    }
    let thread = get_thread_info(url);
    Some(format!(
        "https://a.4cdn.org/{}/thread/{}.json",
        thread.board, thread.id
    ))
}

#[derive(Deserialize)]
struct ApiThread {
    posts: Vec<ApiPost>,
}

#[derive(Deserialize)]
struct ApiPost {
    tim:         Option<u64>,
    ext:         Option<String>,
    filename:    Option<String>,
    #[serde(default)]
    filedeleted: u8,
}

/// Returns the image/video links of a thread from its 4chan API json (see
/// [`get_api_url`]), in post order.
///
/// # Examples
///
/// ```
/// let json = r#"{"posts": [{"no": 1, "tim": 1489266570954, "ext": ".jpg"}, {"no": 2}]}"#;
/// let links = chan_downloader::get_api_image_links(json, "wg").unwrap();
///
/// assert_eq!(links.len(), 1);
/// assert_eq!(links[0].url, "//i.4cdn.org/wg/1489266570954.jpg");
/// assert_eq!(links[0].name, "1489266570954.jpg");
/// ```
pub fn get_api_image_links(json: &str, board: &str) -> Result<Vec<Link>, serde_json::Error> {
    info!(target: "link_events", "Getting image links from API");
    let thread: ApiThread = serde_json::from_str(json)?;
    let links_v: Vec<Link> = thread
        .posts
        .into_iter()
        .filter(|post| post.filedeleted == 0)
        .filter_map(|post| {
            let ext = post.ext?;
            // /f/ serves files under their original name
            let name = if board == "f" {
                format!("{}{}", post.filename?, ext)
            } else {
                format!("{}{}", post.tim?, ext)
            };
            let mut url = Url::parse("https://i.4cdn.org").ok()?;
            url.path_segments_mut().ok()?.push(board).push(&name);
            Some(Link {
                url: url.as_str().trim_start_matches("https:").to_owned(),
                name,
            })
        })
        .collect();
    info!("Got {} image links from API", links_v.len());
    Ok(links_v)
}

/// Initialize a [`Regex`] once
#[macro_export]
macro_rules! regex {
    ($re:expr $(,)?) => {{
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;
    use wiremock::{
        Mock,
        MockServer,
        ResponseTemplate,
        matchers::{method, path},
    };

    #[test]
    fn it_gets_4chan_thread_info() {
        let url = "https://boards.4chan.org/wg/thread/6872254";
        let thread = get_thread_info(url);
        assert_eq!(thread.board, "wg");
        assert_eq!(thread.id, 6872254);
    }

    #[test]
    fn it_gets_4plebs_thread_info() {
        let url = "https://archive.4plebs.org/x/thread/32661196";
        let thread = get_thread_info(url);
        assert_eq!(thread.board, "x");
        assert_eq!(thread.id, 32661196);
    }

    #[test]
    fn it_gets_4chan_image_links() {
        let links_iter = get_image_links(
            r#"
            <a href="//i.4cdn.org/wg/1489266570954.jpg" target="_blank">stickyop.jpg</a>
            <a href="//i.4cdn.org/wg/1489266570954.jpg" target="_blank">stickyop.jpg</a>
        "#,
        );
        assert_eq!(links_iter.len(), 1);
        for link in links_iter {
            assert_eq!(link.url, "//i.4cdn.org/wg/1489266570954.jpg");
            assert_eq!(link.name, "1489266570954.jpg");
        }
    }

    #[test]
    fn it_gets_4plebs_image_links() {
        let links_iter = get_image_links(
            r#"
            <a href="https://img.4plebs.org/boards/x/image/1660/66/1660662319160984.png" target="_blank"></a>
            <a href="https://img.4plebs.org/boards/x/image/1660/66/1660662319160984.png" target="_blank"></a>
        "#,
        );
        assert_eq!(links_iter.len(), 1);
        for link in links_iter {
            assert_eq!(
                link.url,
                "//img.4plebs.org/boards/x/image/1660/66/1660662319160984.png"
            );
            assert_eq!(link.name, "1660662319160984.png");
        }
    }

    #[test]
    fn it_gets_thread_info_with_slug() {
        let url = "https://boards.4chan.org/p/thread/4535213/bdp-bird-posting-discussion";
        let thread = get_thread_info(url);
        assert_eq!(thread.board, "p");
        assert_eq!(thread.id, 4535213);
    }

    #[test]
    fn it_gets_thread_info_with_post_anchor() {
        let thread = get_thread_info("https://boards.4chan.org/wg/thread/6872254#p6872300");
        assert_eq!(thread.board, "wg");
        assert_eq!(thread.id, 6872254);
    }

    #[test]
    #[should_panic(expected = "failed to parse thread id")]
    fn it_panics_on_non_numeric_thread_id() {
        let _ = get_thread_info("https://boards.4chan.org/wg/thread/catalog");
    }

    #[test]
    fn it_keeps_one_link_per_image_in_page_order() {
        // 4chan pages contain each full-size image link twice
        let links = get_image_links(
            r#"
            <a href="//i.4cdn.org/wg/1111111111111.png">a.png</a>
            <a href="//i.4cdn.org/wg/1111111111111.png"><img src="//i.4cdn.org/wg/1111111111111s.jpg"></a>
            <a href="//i.4cdn.org/wg/2222222222222.webm">b.webm</a>
            <a href="//i.4cdn.org/wg/2222222222222.webm"><img src="//i.4cdn.org/wg/2222222222222s.jpg"></a>
        "#,
        );
        let names: Vec<&str> = links.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["1111111111111.png", "2222222222222.webm"]);
        assert_eq!(links[1].url, "//i.4cdn.org/wg/2222222222222.webm");
    }

    #[test]
    fn it_matches_all_supported_extensions_and_hosts() {
        let links = get_image_links(
            r#"
            //i.4cdn.org/g/1.jpg //i.4cdn.org/g/1.jpg
            //is2.4chan.org/g/2.gif //is2.4chan.org/g/2.gif
            //img.4plebs.org/boards/x/image/1660/66/3.webm //img.4plebs.org/boards/x/image/1660/66/3.webm
            //i.4cdn.org/3/4.mp4 //i.4cdn.org/3/4.mp4
            //img.4plebs.org/boards/x/image/1660/66/5.mp4 //img.4plebs.org/boards/x/image/1660/66/5.mp4
            //i.4cdn.org/po/6.pdf //i.4cdn.org/po/6.pdf
            //i.4pcdn.org/x/7.png //i.4pcdn.org/x/7s.jpg
        "#,
        );
        let names: Vec<&str> = links.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, [
            "1.jpg", "2.gif", "3.webm", "4.mp4", "5.mp4", "6.pdf", "7.png"
        ]);
    }

    #[test]
    fn it_gets_each_video_once() {
        // Trimmed from a real /3/ thread: videos are linked three times
        let links = get_image_links(
            r#"
            <div class="fileText"><a href="//i.4cdn.org/3/1111111111111.png" target="_blank">a.png</a></div>
            <a class="fileThumb" href="//i.4cdn.org/3/1111111111111.png"><img src="//i.4cdn.org/3/1111111111111s.jpg"></a>
            <link itemprop="contentUrl" href="//i.4cdn.org/3/2222222222222.mp4">
            <div class="fileText"><a href="//i.4cdn.org/3/2222222222222.mp4" target="_blank">b.mp4</a></div>
            <a class="fileThumb" href="//i.4cdn.org/3/2222222222222.mp4"><img src="//i.4cdn.org/3/2222222222222s.jpg"></a>
            <link itemprop="contentUrl" href="//i.4cdn.org/3/3333333333333.webm">
            <div class="fileText"><a href="//i.4cdn.org/3/3333333333333.webm" target="_blank">c.webm</a></div>
            <a class="fileThumb" href="//i.4cdn.org/3/3333333333333.webm"><img src="//i.4cdn.org/3/3333333333333s.jpg"></a>
            <div class="fileText"><a href="//i.4cdn.org/3/4444444444444.jpg" target="_blank">d.jpg</a></div>
            <a class="fileThumb" href="//i.4cdn.org/3/4444444444444.jpg"><img src="//i.4cdn.org/3/4444444444444s.jpg"></a>
        "#,
        );
        let names: Vec<&str> = links.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, [
            "1111111111111.png",
            "2222222222222.mp4",
            "3333333333333.webm",
            "4444444444444.jpg"
        ]);
    }

    #[test]
    fn it_ignores_other_hosts_and_extensions() {
        let links = get_image_links(
            r#"
            //i.example.org/g/1.jpg //i.example.org/g/1.jpg
            //i.4cdn.org/g/2.txt //i.4cdn.org/g/2.txt
        "#,
        );
        assert!(links.is_empty());
    }

    #[test]
    fn it_finds_no_links_in_empty_page() {
        assert!(get_image_links("").is_empty());
    }

    #[tokio::test]
    async fn it_gets_page_content() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wg/thread/1"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>thread</html>"))
            .mount(&server)
            .await;

        let url = format!("{}/wg/thread/1", server.uri());
        let result = get_page_content(&url, &Client::new()).await.unwrap();
        assert_eq!(result, "<html>thread</html>");
    }

    #[test]
    fn it_gets_api_url() {
        for (url, api) in [
            (
                "https://boards.4chan.org/wg/thread/6872254",
                "https://a.4cdn.org/wg/thread/6872254.json",
            ),
            (
                "https://boards.4chan.org/p/thread/4535213/bdp-bird-posting-discussion",
                "https://a.4cdn.org/p/thread/4535213.json",
            ),
            (
                "https://boards.4chan.org/g/thread/105076684#p105076700",
                "https://a.4cdn.org/g/thread/105076684.json",
            ),
            (
                "https://boards.4channel.org/g/thread/105076684",
                "https://a.4cdn.org/g/thread/105076684.json",
            ),
        ] {
            assert_eq!(get_api_url(url).as_deref(), Some(api), "{url}");
        }
    }

    #[test]
    fn it_has_no_api_url_for_other_sites() {
        assert!(get_api_url("https://archive.4plebs.org/x/thread/32661196").is_none());
        assert!(get_api_url("http://127.0.0.1:8080/wg/thread/1").is_none());
        assert!(get_api_url("not a url").is_none());
    }

    #[test]
    fn it_gets_api_image_links() {
        // Trimmed from https://a.4cdn.org/3/thread/1032629.json
        let json = r#"{"posts": [
            {"no": 1032629, "tim": 1789732438865756, "ext": ".png", "filename": "file", "com": "OP"},
            {"no": 1032630, "com": "text only reply"},
            {"no": 1032631, "tim": 1792994168478222, "ext": ".mp4", "filename": "0000-0200-Merged"},
            {"no": 1032632, "tim": 1792994168470000, "ext": ".jpg", "filename": "gone", "filedeleted": 1},
            {"no": 1032633, "tim": 1789749874484566, "ext": ".webm", "filename": "clip"}
        ]}"#;
        let links = get_api_image_links(json, "3").unwrap();
        let got: Vec<(&str, &str)> = links.iter().map(|l| (l.url.as_str(), l.name.as_str())).collect();
        assert_eq!(got, [
            ("//i.4cdn.org/3/1789732438865756.png", "1789732438865756.png"),
            ("//i.4cdn.org/3/1792994168478222.mp4", "1792994168478222.mp4"),
            ("//i.4cdn.org/3/1789749874484566.webm", "1789749874484566.webm"),
        ]);
    }

    #[test]
    fn it_uses_original_filenames_on_f() {
        let json = r#"{"posts": [
            {"no": 3524333, "tim": 1744587482715200, "ext": ".swf", "filename": "jesus on the cross"}
        ]}"#;
        let links = get_api_image_links(json, "f").unwrap();
        assert_eq!(links[0].url, "//i.4cdn.org/f/jesus%20on%20the%20cross.swf");
        assert_eq!(links[0].name, "jesus on the cross.swf");
    }

    #[test]
    fn it_rejects_invalid_api_json() {
        assert!(get_api_image_links("<html>Please wait</html>", "g").is_err());
        assert!(get_api_image_links(r#"{"error": "nope"}"#, "g").is_err());
    }

    #[tokio::test]
    async fn it_fails_to_get_page_content_on_error_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403).set_body_string("<html>Please wait</html>"))
            .mount(&server)
            .await;

        let url = format!("{}/x/thread/1", server.uri());
        let err = get_page_content(&url, &Client::new()).await.unwrap_err();
        assert_eq!(err.status(), Some(reqwest::StatusCode::FORBIDDEN));
    }

    #[tokio::test]
    async fn it_saves_image() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wg/1.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"\x89PNG fake".to_vec()))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("1.png");
        let dest = dest.to_str().unwrap();
        let url = format!("{}/wg/1.png", server.uri());
        let answer = save_image(&url, dest, &Client::new()).await.unwrap();
        assert_eq!(answer, dest);
        assert_eq!(std::fs::read(dest).unwrap(), b"\x89PNG fake");
    }

    #[tokio::test]
    async fn it_fails_to_save_missing_image() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("1.png");
        let url = format!("{}/wg/1.png", server.uri());
        let err = save_image(&url, dest.to_str().unwrap(), &Client::new())
            .await
            .unwrap_err();
        assert_eq!(err.status(), Some(reqwest::StatusCode::NOT_FOUND));
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn it_leaves_no_file_when_transfer_fails() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        // Server announcing 100 bytes but closing after 10
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n0123456789")
                .unwrap();
        });

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("1.png");
        let url = format!("http://{addr}/wg/1.png");
        assert!(
            save_image(&url, dest.to_str().unwrap(), &Client::new())
                .await
                .is_err()
        );
        assert!(!dest.exists());
    }

    // Tests against the live sites. Run with `cargo test -- --ignored`
    #[tokio::test]
    #[ignore = "hits the network"]
    async fn it_saves_live_4chan_image() {
        let client = Client::builder().user_agent("reqwest").build().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("1745612650141704.png");
        let url = "https://i.4cdn.org/g/1745612650141704.png";
        save_image(url, dest.to_str().unwrap(), &client).await.unwrap();
        assert!(dest.metadata().unwrap().len() > 0);
    }

    #[tokio::test]
    #[ignore = "hits the network"]
    async fn it_saves_live_4plebs_image() {
        let client = Client::builder().user_agent("reqwest").build().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("1614942709612.jpg");
        let url = "https://img.4plebs.org/boards/x/image/1614/94/1614942709612.jpg";
        save_image(url, dest.to_str().unwrap(), &client).await.unwrap();
        assert!(dest.metadata().unwrap().len() > 0);
    }
}
