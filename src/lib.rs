//! # chan_downloader
//!
//! `chan_downloader` is a collection of utilities to
//! download images/webms from a 4chan thread

#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate reqwest;

use std::fs::File;
use std::io::{copy, Cursor};

use log::info;
use regex::Regex;
use reqwest::Error;
use reqwest::Client;

pub struct Link {
    pub url: String,
    pub name: String,
}

/// Saves the image from the url to the given path.
/// Returns the path on success
///
/// # Examples
///
/// ```
/// use reqwest::Client;
/// use std::env;
/// use std::fs::remove_file;
/// let client = Client::builder().user_agent("reqwest").build().unwrap();
/// let workpath = env::current_dir().unwrap().join("1489266570954.jpg");
/// let url = "https://i.4cdn.org/wg/1489266570954.jpg";
/// async {
///     let answer = chan_downloader::save_image(url, workpath.to_str().unwrap(), &client).await.unwrap();
///     assert_eq!(workpath.to_str().unwrap(), answer);
///     remove_file(answer).unwrap();
/// };
/// ```
pub async fn save_image(url: &str, path: &str, client: &Client) -> Result<String, Error> {
    info!(target: "image_events", "Saving image to: {}", path);
    let response = client.get(url).send().await?;

    if response.status().is_success() {
        let mut dest = File::create(path).unwrap();
        let mut content =  Cursor::new(response.bytes().await?);
        copy(&mut content, &mut dest).unwrap();
    }
    info!("Saved image to: {}", path);
    Ok(String::from(path))
}

/// Returns the page content from the given url.
///
/// # Examples
///
/// ```
/// use std::io;
/// use reqwest::Client;
/// let client = Client::builder().user_agent("reqwest").build().unwrap();
/// let url = "https://raw.githubusercontent.com/mariot/chan-downloader/master/.gitignore";
/// async {
///     let result = chan_downloader::get_page_content(url, &client).await.unwrap();
///     assert_eq!(result, "/target/\nCargo.lock\n**/*.rs.bk\n");
/// };
/// ```
pub async fn get_page_content(url: &str, client: &Client) -> Result<String, Error> {
    info!(target: "page_events", "Loading page: {}", url);
    let response = client.get(url).send().await?;
    let content =  response.text().await?;
    info!("Loaded page: {}", url);
    Ok(content)
}

/// Returns the board name and thread id.
///
/// # Examples
///
/// ```
/// let url = "https://boards.4chan.org/wg/thread/6872254";
/// let (board_name, thread_id) = chan_downloader::get_thread_infos(url);
///
/// assert_eq!(board_name, "wg");
/// assert_eq!(thread_id, "6872254");
/// ```
pub fn get_thread_infos(url: &str) -> (&str, &str) {
    info!(target: "thread_events", "Getting thread infos from: {}", url);
    let url_vec: Vec<&str> = url.split('/').collect();
    let board_name = url_vec[3];
    let thread_vec: Vec<&str> = url_vec[5].split('#').collect();
    let thread_id = thread_vec[0];
    info!("Got thread infos from: {}", url);
    (board_name, thread_id)
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
pub fn get_image_links(page_content: &str) -> Vec<Link> {
    info!(target: "link_events", "Getting image links");
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r"(//i(?:s)?\d*\.(?:4cdn|4chan)\.org/\w+/(\d+\.(?:jpg|png|gif|webm)))")
                .unwrap();
    }

    let links_iter = RE.captures_iter(page_content);
    let number_of_links = RE.captures_iter(page_content).count() / 2;
    info!("Got {} image links from page", number_of_links);
    let mut links_v: Vec<Link> = Vec::new();
    for cap in links_iter.step_by(2) {
        links_v.push(Link{ url: String::from(&cap[1]), name: String::from(&cap[2]) });
    }
    links_v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_gets_thread_infos() {
        let url = "https://boards.4chan.org/wg/thread/6872254";
        let (board_name, thread_id) = get_thread_infos(url);
        assert_eq!(board_name, "wg");
        assert_eq!(thread_id, "6872254");
    }

    #[test]
    fn it_gets_image_links() {
        let links_iter = get_image_links("
            <a href=\"//i.4cdn.org/wg/1489266570954.jpg\" target=\"_blank\">stickyop.jpg</a>
            <a href=\"//i.4cdn.org/wg/1489266570954.jpg\" target=\"_blank\">stickyop.jpg</a>
        ");
        for link in links_iter {
            assert_eq!(link.url, "//i.4cdn.org/wg/1489266570954.jpg");
            assert_eq!(link.name, "1489266570954.jpg");
        }
    }

    #[tokio::test]
    async fn it_gets_page_content() {
        use reqwest::Client;
        let client = Client::builder().user_agent("reqwest").build().unwrap();
        let url = "https://raw.githubusercontent.com/mariot/chan-downloader/master/.gitignore";
        let result = get_page_content(url, &client).await.unwrap();
        assert_eq!(result, "/target/\nCargo.lock\n**/*.rs.bk\n");
    }

    #[tokio::test]
    async fn it_saves_image() {
        use reqwest::Client;
        use std::env;
        use std::fs::remove_file;
        let client = Client::builder().user_agent("reqwest").build().unwrap();
        let workpath = env::current_dir().unwrap().join("1489266570954.jpg");
        let url = "https://i.4cdn.org/wg/1489266570954.jpg";
        let answer = save_image(url, workpath.to_str().unwrap(), &client).await.unwrap();
        assert_eq!(workpath.to_str().unwrap(), answer);
        remove_file(answer).unwrap();
    }
}
