#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;

use std::env;
use std::fs::create_dir_all;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::thread;

use clap::App;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;

use chan_downloader::{get_image_links, get_page_content, get_thread_infos, save_image};

fn main() {
    env_logger::init();
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let thread = matches.value_of("thread").unwrap();
    let output = matches.value_of("output").unwrap_or("downloads");
    let reload: bool = matches.is_present("reload");
    let interval: u64 = matches.value_of("interval").unwrap_or("5").parse().unwrap();
    let limit: u64 = matches.value_of("limit").unwrap_or("120").parse().unwrap();

    let mut execution_number: u64 = 0;
    let number_of_execution: u64 = if reload {limit / interval} else { 1 };

    info!(target: "downloader_events", "Downloading images from {} to {}", thread, output);

    let directory = create_directory(thread, &output);

    let wait_time = Duration::from_millis(60000 * interval);
    while execution_number != number_of_execution {
        let start = Instant::now();
        execution_number += 1;
        explore_thread(thread, &directory);
        let runtime = start.elapsed();
        if let Some(remaining) = wait_time.checked_sub(runtime) {
            info!(
                "Schedule slice has time left over; sleeping for {:?}",
                remaining
            );
            thread::sleep(remaining);
        }
        info!(target: "downloader_events", "Downloader executed {} time{}", execution_number, if execution_number == 1 { "" } else {"s"} );
    }
}

fn explore_thread(thread_link: &str, directory: &PathBuf) {
    let client = Client::builder().user_agent("reqwest").build().unwrap();
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
}

fn create_directory(thread_link: &str, output: &str) -> PathBuf {
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
    
    info!("Downloaded: {} in {}", thread_link, output);
    directory
}
