#[macro_use]
extern crate clap;

use std::env;
use std::fs::create_dir_all;

use clap::App;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Error;
use reqwest::blocking::Client;

use chan_downloader::{get_image_links, get_page_content, get_thread_infos, save_image};

fn main() {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let thread = matches.value_of("thread").unwrap();
    let output = matches.value_of("output").unwrap_or("downloads");
    download_thread(thread, &output).unwrap();
}

fn download_thread(thread_link: &str, output: &str) -> Result<String, Error> {
    let client = Client::builder().user_agent("reqwest").build()?;
    let workpath = env::current_dir().unwrap();

    let (board_name, thread_id) = get_thread_infos(thread_link);

    let directory = workpath.join(output).join(board_name).join(thread_id);
    if !directory.exists() {
        match create_dir_all(&directory) {
            Ok(_) => {}
            Err(err) => eprintln!("Failed to create new directory: {}", err),
        }
    }

    let page_string = match get_page_content(thread_link, &client) {
        Ok(page_string) => page_string,
        Err(err) => {
            eprintln!("Error: {}", err);
            String::from("")
        },
    };
    let (links_iter, number_of_links) = get_image_links(page_string.as_str());
    let pb = ProgressBar::new(number_of_links as u64);

    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg} ({eta})")
        .progress_chars("#>-"));
    pb.tick();

    for cap in links_iter.step_by(2) {
        let img_path = directory.join(&cap[2]);
        if !img_path.exists() {
            match save_image(
                format!("https:{}", &cap[1]).as_str(),
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

    Ok(format!("Downloaded: {} in {}", thread_link, output))
}
