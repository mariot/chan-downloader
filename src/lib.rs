//! # chan_downloader
//!
//! `chan_downloader` is a collection of utilities to
//! download images/webms from a 4chan thread

#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate reqwest;

use std::fs::File;
use std::io::copy;

use regex::{CaptureMatches, Regex};
use reqwest::{Client, Error};

/// Saves the image from the url to the given path.
/// Returns the path on success
///
/// # Examples
///
/// ```
/// use reqwest::Client;
/// use std::env;
/// use std::fs::remove_file;
/// let client = Client::new();
/// let workpath = env::current_dir().unwrap().join("1489266570954.jpg");
/// let url = "https://i.4cdn.org/wg/1489266570954.jpg";
/// let answer = chan_downloader::save_image(url, workpath.to_str().unwrap(), &client).unwrap();
///
/// assert_eq!(workpath.to_str().unwrap(), answer);
/// remove_file(answer).unwrap();
/// ```
pub fn save_image(url: &str, path: &str, client: &Client) -> Result<String, Error> {
    let mut response = client.get(url).send()?;

    if response.status().is_success() {
        let mut dest = File::create(path).unwrap();
        copy(&mut response, &mut dest).unwrap();
    }
    Ok(String::from(path))
}

/// Returns the page content from the given url.
///
/// # Examples
///
/// ```
/// use reqwest::Client;
/// let client = Client::new();
/// let url = "https://boards.4chan.org/wg/thread/6872254";
/// match chan_downloader::get_page_content(url, &client) {
///     Ok(page) => println!("Content: {}", page),
///     Err(err) => eprintln!("Error: {}", err),
/// }
/// ```
pub fn get_page_content(url: &str, client: &Client) -> Result<String, Error> {
    let mut response = client.get(url).send()?;
    Ok(response.text().unwrap())
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
    let url_vec: Vec<&str> = url.split('/').collect();
    let board_name = url_vec[3];
    let thread_vec: Vec<&str> = url_vec[5].split('#').collect();
    let thread_id = thread_vec[0];
    (board_name, thread_id)
}

/// Returns the links and the number of links from a page.
/// Note that the links are doubled
///
/// # Examples
///
/// ```
/// use reqwest::Client;
/// let client = Client::new();
/// let url = "https://boards.4chan.org/wg/thread/6872254";
/// match chan_downloader::get_page_content(url, &client) {
///     Ok(page_string) => {
///         let (links_iter, number_of_links) = chan_downloader::get_image_links(page_string.as_str());

///         assert_eq!(number_of_links, 4);
/// 
///         for cap in links_iter.step_by(2) {
///             println!("{} and {}", &cap[1], &cap[2]);
///         }
///     },
///     Err(err) => eprintln!("Error: {}", err),
/// }
/// ```
pub fn get_image_links(page_content: &str) -> (CaptureMatches, usize) {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r"(//i(?:s)?\d*\.(?:4cdn|4chan)\.org/\w+/(\d+\.(?:jpg|png|gif|webm)))")
                .unwrap();
    }

    let links_iter = RE.captures_iter(page_content);
    let number_of_links = RE.captures_iter(page_content).count() / 2;
    (links_iter, number_of_links)
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
        let (links_iter, number_of_links) = get_image_links("
            <a href=\"//i.4cdn.org/wg/1489266570954.jpg\" target=\"_blank\">stickyop.jpg</a>
            <a href=\"//i.4cdn.org/wg/1489266570954.jpg\" target=\"_blank\">stickyop.jpg</a>
        ");
        assert_eq!(number_of_links, 1);
        for cap in links_iter.step_by(2) {
            let url = &cap[1];
            let filename = &cap[2];
            assert_eq!(url, "//i.4cdn.org/wg/1489266570954.jpg");
            assert_eq!(filename, "1489266570954.jpg");
        }
    }

    #[test]
    fn it_gets_page_content() {
        use reqwest::Client;
        let client = Client::new();
        let url = "https://raw.githubusercontent.com/mariot/chan-downloader/master/.gitignore";
        let result = get_page_content(url, &client).unwrap();
        assert_eq!(result, "/target/\nCargo.lock\n**/*.rs.bk\n");
    }

    #[test]
    fn it_saves_image() {
        use reqwest::Client;
        use std::env;
        use std::fs::remove_file;
        let client = Client::new();
        let workpath = env::current_dir().unwrap().join("1489266570954.jpg");
        let url = "https://i.4cdn.org/wg/1489266570954.jpg";
        let answer = save_image(url, workpath.to_str().unwrap(), &client).unwrap();
        assert_eq!(workpath.to_str().unwrap(), answer);
        remove_file(answer).unwrap();
    }
}
