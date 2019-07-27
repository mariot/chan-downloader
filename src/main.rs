#[macro_use]
extern crate clap;
#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate reqwest;
extern crate tempdir;

use clap::{App, ArgMatches};
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use reqwest::{Client, StatusCode};
use tempdir::TempDir;

use std::env;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::copy;

fn main() {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let thread = matches.value_of("thread").unwrap();
    let client = Client::new();
    download_thread(thread, &matches, &client);
}

fn load(url: &str, client: &Client) -> reqwest::Response {
    client.get(url).send().unwrap()
}

fn save_image(url: &str, name: &str, client: &Client) -> Result<(String), String> {
    let tmp_dir = TempDir::new("inb4404_temp");
    let mut response = client.get(url).send().unwrap();

    let file_name = match response.status() {
        StatusCode::OK => {
            let mut dest = {
                tmp_dir.unwrap().path().join(name);
                File::create(name).unwrap()
            };
            copy(&mut response, &mut dest).unwrap();
            name
        }
        StatusCode::NOT_FOUND => {
            return Err(String::from("File not found"));
        }
        s => return Err(format!("Received response status: {:?}", s)),
    };
    Ok(String::from(file_name))
}

fn download_thread(thread_link: &str, matches: &ArgMatches, client: &Client) {
    let workpath = env::current_dir().unwrap();

    lazy_static! {
        static ref RE: Regex =
            Regex::new(r"(//i(?:s)?\d*\.(?:4cdn|4chan)\.org/\w+/(\d+\.(?:jpg|png|gif|webm)))")
                .unwrap();
    }

    let url_vec: Vec<&str> = thread_link.split('/').collect();
    let board = url_vec[3];
    let thread_vec: Vec<&str> = url_vec[5].split('#').collect();
    let mut thread = thread_vec[0];

    if url_vec.len() > 6 {
        let thread_tmp_vec: Vec<&str> = url_vec[6].split('#').collect();
        let thread_tmp = thread_tmp_vec[0];

        let path = workpath.join("downloads").join(board).join(thread_tmp);

        if matches.is_present("names") || path.exists() {
            thread = thread_tmp;
        }
    }

    let directory = workpath.join("downloads").join(board).join(thread);
    if !directory.exists() {
        match create_dir_all(&directory) {
            Ok(_) => {}
            Err(err) => eprintln!("Failed to create new directory: {}", err),
        }
    }

    let mut thread_page = load(thread_link, client);
    let page_string = thread_page.text().unwrap();
    let links_iter = RE.captures_iter(page_string.as_str());

    let number_of_links = RE.captures_iter(page_string.as_str()).count() / 2;
    let pb = ProgressBar::new(number_of_links as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg} ({eta})")
        .progress_chars("#>-"));

    pb.tick();
    for cap in links_iter.step_by(2) {
        let img_path = directory.join(&cap[2]);
        if !img_path.exists() {
            match save_image(
                format!("{}{}", "https:", &cap[1]).as_str(),
                img_path.to_str().unwrap(),
                client,
            ) {
                Ok(_) => {}
                Err(err) => eprintln!("Error: {}", err),
            }
        }
        pb.set_message(&cap[2].to_string());
        pb.inc(1);
    }
    pb.finish_with_message("Done");
}
