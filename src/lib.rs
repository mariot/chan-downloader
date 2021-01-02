#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate reqwest;

use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use reqwest::{Client, Error};

use std::env;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::copy;

fn load(url: &str, client: &Client) -> Result<String, Error> {
    let mut response = client.get(url).send()?;
    Ok(response.text().unwrap())
}

fn save_image(url: &str, name: &str, client: &Client) -> Result<String, Error> {
    let mut response = client.get(url).send()?;

    if response.status().is_success() {
        let mut dest = File::create(name).unwrap();
        copy(&mut response, &mut dest).unwrap();
    }
    Ok(String::from(name))
}

pub fn download_thread(thread_link: &str, output: &str) {
    let client = Client::new();
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

        let path = workpath.join(output).join(board).join(thread_tmp);

        if path.exists() {
            thread = thread_tmp;
        }
    }

    let directory = workpath.join(output).join(board).join(thread);
    if !directory.exists() {
        match create_dir_all(&directory) {
            Ok(_) => {}
            Err(err) => eprintln!("Failed to create new directory: {}", err),
        }
    }

    match load(thread_link, &client) {
        Ok(page_string) => {
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
                        &client,
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
        Err(err) => eprintln!("Error: {}", err),
    }
}