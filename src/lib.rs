//! # chan_downloader
//!
//! `chan_downloader` is a collection of utilities to
//! download images/webms from a 4chan thread

use log::info;
use reqwest::{Client, Error};
use std::{
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

    let mut dest = File::create(path).unwrap();
    let mut content = Cursor::new(response.bytes().await?);
    io::copy(&mut content, &mut dest).unwrap();
    info!("Saved image to: {}", path);
    Ok(String::from(path))
}

/// Returns the page content from the given url.
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
    let response = client.get(url).send().await?;
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

/// Returns the links and the number of links from a page.
/// Note that the links are doubled
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
//    - https://img.4plebs.org/boards/x/image/1660/66/1660662319160984.png
//    - https://i.4cdn.org/sp/1661019073822058.jpg
#[must_use]
pub fn get_image_links(page_content: &str) -> Vec<Link> {
    info!(target: "link_events", "Getting image links");
    let reg = regex!(
        r"(//i(?:s|mg)?(?:\d*)?\.(?:4cdn|4chan|4plebs)\.org/(?:\w+/){1,3}(?:\d+/){0,2}(\d+\.(?:jpg|png|gif|webm)))"
    );

    let links_iter = reg.captures_iter(page_content);
    let number_of_links = reg.captures_iter(page_content).count() / 2;
    info!("Got {} image links from page", number_of_links);
    let mut links_v: Vec<Link> = Vec::new();
    for cap in links_iter.step_by(2) {
        links_v.push(Link {
            url:  String::from(&cap[1]),
            name: String::from(&cap[2]),
        });
    }
    links_v
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
        "#,
        );
        let names: Vec<&str> = links.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["1.jpg", "2.gif", "3.webm"]);
    }

    #[test]
    fn it_ignores_other_hosts_and_extensions() {
        let links = get_image_links(
            r#"
            //i.example.org/g/1.jpg //i.example.org/g/1.jpg
            //i.4cdn.org/g/2.pdf //i.4cdn.org/g/2.pdf
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
