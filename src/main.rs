#[macro_use]
extern crate clap;
extern crate regex;
extern crate reqwest;
extern crate tempdir;

use clap::{App, ArgMatches};
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
        s => return Err(String::from(format!("Received response status: {:?}", s))),
    };
    Ok(String::from(file_name))
}

fn download_thread(thread_link: &str, matches: &ArgMatches, client: &Client) {
    let workpath = env::current_dir().unwrap();
    let re =
        Regex::new(r"(//i(?:s)?\d*\.(?:4cdn|4chan)\.org/\w+/(\d+\.(?:jpg|png|gif|webm)))").unwrap();

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
            Ok(_) => println!("Created new directory: {}", directory.display()),
            Err(err) => eprintln!("Failed to create new directory: {}", err),
        }
    } else {
        println!("Using existing directory: {}", directory.display())
    }

    let mut thread_page = load(thread_link, client);
    for cap in re
        .captures_iter(thread_page.text().unwrap().as_str())
        .step_by(2)
    {
        let img_path = directory.join(&cap[2]);
        if !img_path.exists() {
            match save_image(
                format!("{}{}", "https:", &cap[1]).as_str(),
                img_path.to_str().unwrap(),
                client,
            ) {
                Ok(name) => println!("New file: {}", name),
                Err(err) => eprintln!("Error: {}", err),
            }
        }
    }
}
