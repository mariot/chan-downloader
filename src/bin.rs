#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;

use std::env;
use std::fs::create_dir_all;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::thread;
use std::sync::Mutex;
use futures::stream::StreamExt;

use clap::App;
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use reqwest::{Client, Error};

use chan_downloader::{get_image_links, get_page_content, get_thread_infos, save_image};

lazy_static! {
    static ref DOWNLOADED_FILES: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

fn main() {
    env_logger::init();
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let thread = matches.value_of("thread").unwrap();
    let output = matches.value_of("output").unwrap_or("downloads");
    let reload: bool = matches.is_present("reload");
    let interval: u64 = matches.value_of("interval").unwrap_or("5").parse().unwrap();
    let limit: u64 = matches.value_of("limit").unwrap_or("120").parse().unwrap();
    let concurrent: usize = matches.value_of("concurrent").unwrap_or("2").parse().unwrap();

    info!("Downloading images from {} to {}", thread, output);

    let directory = create_directory(thread, &output);

    let start = Instant::now();
    let wait_time = Duration::from_secs(60 * interval);
    let limit_time = if reload { Duration::from_secs(60 * limit) } else { Duration::from_secs(0) };
    loop {
        let load_start = Instant::now();
        explore_thread(thread, &directory, concurrent).unwrap();
        let runtime = start.elapsed();
        let load_runtime = load_start.elapsed();
        if runtime > limit_time {
            info!("Runtime exceeded, exiting.");
            break;
        };
        if let Some(remaining) = wait_time.checked_sub(load_runtime) {
            info!("Schedule slice has time left over; sleeping for {:?}", remaining);
            thread::sleep(remaining);
        }
        info!("Downloader executed one more time for {:?}", load_runtime);
    }
}

fn mark_as_downloaded(file: &str) -> Result<&str, &str> {
    let mut db = DOWNLOADED_FILES.lock().map_err(|_| "Failed to acquire MutexGuard")?;
    db.push(file.to_string());
    Ok(file)
}

#[tokio::main]
async fn explore_thread(thread_link: &str, directory: &PathBuf, concurrent: usize) -> Result<(), Error> {
    let start = Instant::now();
    let client = Client::builder().user_agent("reqwest").build()?;
    let page_string = match get_page_content(thread_link, &client).await {
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
    let links_vec = get_image_links(page_string.as_str());
    let pb = ProgressBar::new(links_vec.len() as u64);

    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg} ({eta})")
        .progress_chars("#>-"));
    pb.tick();

    let fetches = futures::stream::iter(
        links_vec.into_iter().map(|link| {
            let client = &client;
            let pb = &pb;
            async move {
                let img_path = directory.join(link.name);
                let image_path = img_path.to_str().unwrap();
                let has_been_downloaded = async {
                    let db = DOWNLOADED_FILES.lock().map_err(|_| String::from("Failed to acquire MutexGuard")).unwrap();
                    db.contains(&String::from(image_path))
                }.await;

                if has_been_downloaded {
                    info!("Image {} previously downloaded. Skipped", img_path.display());
                } else if !img_path.exists() {
                    match save_image(
                        format!("https:{}", link.url).as_str(),
                        image_path,
                        &client,
                    ).await {
                        Ok(path) => {
                            info!("Saved image to {}", &path);
                            let result = mark_as_downloaded(&path).unwrap();
                            info!("{} added to downloaded files", result);
                        }
                        Err(err) => {
                            error!("Couldn't save image {}", image_path);
                            eprintln!("Error: {}", err);
                        },
                    }
                } else {
                    info!("Image {} already exists. Skipped", img_path.display());
                    let result = mark_as_downloaded(image_path).unwrap();
                    info!("{} added to downloaded files", result);
                }
                pb.inc(1);
            }
        })
    ).buffer_unordered(concurrent).collect::<Vec<()>>();
    fetches.await;

    pb.finish_with_message("Done");
    info!("Done in {:?}", start.elapsed());
    Ok(())
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
