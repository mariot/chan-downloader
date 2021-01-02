#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;

use std::env;
use std::fs::create_dir_all;

use clap::App;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Error;
use reqwest::blocking::Client;

use chan_downloader::{get_image_links, get_page_content, get_thread_infos, save_image};

fn main() {
    env_logger::init();
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let thread = matches.value_of("thread").unwrap();
    let output = matches.value_of("output").unwrap_or("downloads");
    info!(target: "downloader_events", "Downloading images from {} to {}", thread, output);
    download_thread(thread, &output).unwrap();
}

fn download_thread(thread_link: &str, output: &str) -> Result<String, Error> {
    let client = Client::builder().user_agent("reqwest").build()?;
    let workpath = env::current_dir().unwrap();
    info!("Working from {}", workpath.display());

    let (board_name, thread_id) = get_thread_infos(thread_link);

    let directory = workpath.join(output).join(board_name).join(thread_id);
    if !directory.exists() {
        match create_dir_all(&directory) {
            Ok(_) => {
                info!("Created directory {}", directory.display());
            }
            Err(err) => {
                error!("Failed to create new directory: {}", err);
                eprintln!("Failed to create new directory: {}", err);
            },
        }
    }

    let page_string = match get_page_content(thread_link, &client) {
        Ok(page_string) => {
            info!("Loaded content from {}", thread_link);
            page_string
        },
        Err(err) => {
            error!("Failed to get content from {}", thread_link);
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
            let image_path = img_path.to_str().unwrap();
            match save_image(
                format!("https:{}", &cap[1]).as_str(),
                image_path,
                &client,
            ) {
                Ok(path) => {
                    info!("Saved image to {}", path);
                }
                Err(err) => {
                    error!("Couldn't save image {}", image_path);
                    eprintln!("Error: {}", err);
                },
            }
        } else {
            info!("Image {} already exists. Skipped", img_path.display());
        }
        pb.set_message(&cap[2].to_string());
        pb.inc(1);
    }
    pb.finish_with_message("Done");
    info!("Download finished");

    Ok(format!("Downloaded: {} in {}", thread_link, output))
}
