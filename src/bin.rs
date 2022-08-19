use futures::stream::StreamExt;
use std::{
    env,
    fs::create_dir_all,
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Error, Result};
use chan_downloader::{get_image_links, get_page_content, get_thread_infos, save_image};
use clap::{
    crate_authors,
    crate_description,
    crate_version,
    value_parser,
    AppSettings,
    Arg,
    ArgAction,
    ColorChoice,
    Command,
    ValueHint,
};
use indicatif::{ProgressBar, ProgressStyle};
use log::{error, info};
use once_cell::sync::Lazy;
use reqwest::Client;

static DOWNLOADED_FILES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn main() -> Result<()> {
    env_logger::init();
    let matches = build_app().get_matches();

    let thread = matches
        .get_one::<String>("thread")
        .context("failed to get 'thread' value")?;
    let output = matches
        .get_one::<String>("output")
        .map_or_else(|| String::from("downloads"), Clone::clone);
    let reload = matches.contains_id("reload");
    let interval = matches.get_one::<u64>("interval").unwrap_or(&5_u64);
    let limit = matches.get_one::<u64>("limit").unwrap_or(&120_u64);
    let concurrent = matches.get_one::<usize>("concurrent").unwrap_or(&2_usize);

    info!("Downloading images from {} to {}", thread, output);

    let directory = create_directory(thread, &output)?;

    let start = Instant::now();
    let wait_time = Duration::from_secs(60 * interval);
    let limit_time = if reload {
        Duration::from_secs(60 * limit)
    } else {
        Duration::from_secs(0)
    };
    loop {
        let load_start = Instant::now();
        explore_thread(thread, &directory, *concurrent).unwrap();
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

    Ok(())
}

fn mark_as_downloaded(file: &str) -> Result<&str, &str> {
    let mut db = DOWNLOADED_FILES
        .lock()
        .map_err(|_| "Failed to acquire MutexGuard")?;
    db.push(file.to_string());

    Ok(file)
}

#[tokio::main]
async fn explore_thread(thread_link: &str, directory: &Path, concurrent: usize) -> Result<(), Error> {
    let start = Instant::now();
    let client = Client::builder().user_agent("reqwest").build()?;

    match get_page_content(thread_link, &client).await {
        Ok(page_string) => {
            info!("Loaded content from {}", thread_link);

            let links_vec = get_image_links(page_string.as_str());
            let pb = ProgressBar::new(links_vec.len() as u64);

            pb.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "{spinner:.green.bold} [{elapsed_precise}] [{bar:40.cyan.bold/blue}] \
                         {pos}/{len} {msg} ({eta})",
                    )
                    .context("failed to build progress bar")?
                    .progress_chars("#>-"),
            );
            pb.tick();

            let fetches = futures::stream::iter(links_vec.into_iter().map(|link| {
                let client = &client;
                let pb = &pb;
                async move {
                    let img_path = directory.join(link.name);
                    let image_path = img_path.to_str().unwrap();
                    let has_been_downloaded = async {
                        let db = DOWNLOADED_FILES
                            .lock()
                            .map_err(|_| String::from("Failed to acquire MutexGuard"))
                            .unwrap();
                        db.contains(&String::from(image_path))
                    }
                    .await;

                    if has_been_downloaded {
                        info!("Image {} previously downloaded. Skipped", img_path.display());
                    } else if !img_path.exists() {
                        match save_image(format!("https:{}", link.url).as_str(), image_path, client).await
                        {
                            Ok(path) => {
                                info!("Saved image to {}", &path);
                                let result = mark_as_downloaded(&path).unwrap();
                                info!("{} added to downloaded files", result);
                            },
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
            }))
            .buffer_unordered(concurrent)
            .collect::<Vec<()>>();
            fetches.await;

            pb.finish_with_message("Done");
            info!("Done in {:?}", start.elapsed());
        },
        Err(e) => {
            error!("Failed to get content from {}", thread_link);
            eprintln!("Error: {}", e);
            return Err(anyhow!(e));
        },
    }

    Ok(())
}

fn create_directory(thread_link: &str, output: &str) -> Result<PathBuf> {
    let workpath = env::current_dir()?;
    info!("Working from {}", workpath.display());

    let (board_name, thread_id) = get_thread_infos(thread_link);

    let directory = workpath.join(output).join(board_name).join(thread_id);
    if !directory.exists() {
        match create_dir_all(&directory) {
            Ok(_) => {
                info!("Created directory {}", directory.display());
            },
            Err(err) => {
                error!("Failed to create new directory: {}", err);
                eprintln!("Failed to create new directory: {}", err);
                return Err(anyhow!(err));
            },
        }
    }

    info!("Downloaded: {} in {}", thread_link, output);
    Ok(directory)
}

/// Build the command-line application
fn build_app() -> Command<'static> {
    Command::new("chan-downloader")
        .bin_name("chan-downloader")
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .color(if env::var_os("NO_COLOR").is_none() {
            ColorChoice::Auto
        } else {
            ColorChoice::Never
        })
        .setting(AppSettings::DeriveDisplayOrder)
        .infer_long_args(true)
        .dont_collapse_args_in_usage(true)
        .arg(
            Arg::new("thread")
                .short('t')
                .long("thread")
                .required(true)
                .takes_value(true)
                .value_name("URL")
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .help("URL of the thread"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .takes_value(true)
                .value_name("DIRECTORY")
                .value_hint(ValueHint::DirPath)
                .help("Output directory (Default is 'downloads')"),
        )
        .arg(
            Arg::new("preserve_filenames")
                .short('p')
                .long("preserve-filenames")
                .takes_value(false)
                .help("Preserve the filenames that are found on 4chan/4plebs"),
        )
        .arg(
            Arg::new("reload")
                .short('r')
                .long("reload")
                .takes_value(false)
                .help("Reload thread every t minutes to get new images"),
        )
        .arg(
            Arg::new("interval")
                .short('i')
                .long("interval")
                .takes_value(true)
                .value_name("INTERVAL")
                .value_parser(value_parser!(u64))
                .help("Time between each reload (in minutes. Default is 5)"),
        )
        .arg(
            Arg::new("limit")
                .short('l')
                .long("limit")
                .takes_value(true)
                .value_name("LIMIT")
                .value_parser(value_parser!(u64))
                .help("Time limit for execution (in minutes. Default is 120)"),
        )
        .arg(
            Arg::new("concurrent")
                .short('c')
                .long("concurrent")
                .takes_value(true)
                .value_name("NUM-REQUESTS")
                .value_parser(value_parser!(usize))
                .help("Number of concurrent requests (Default is 2)"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .takes_value(false)
                .hide(true)
                .action(ArgAction::Count)
                .help("Display debugging messages"),
        )
}

#[test]
fn verify_app() {
    build_app().debug_assert();
}
