// TODO: Implement --preserve-filenames
//       This would preserve the filenames that are given to the files on the
//       given website. It can be accomplished, by using their API.
//       Example API URLs:
// 4plebs: https://archive.4plebs.org/_/api/chan/thread?board=x&num=32661196
//  4chan: https://a.4cdn.org/po/thread/570368.json

use futures::stream::StreamExt;
use std::{
    env,
    fs::{create_dir_all, remove_dir},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        LazyLock,
        Mutex,
        Once,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Error, Result, anyhow, bail};
use chan_downloader::{
    get_api_image_links,
    get_api_url,
    get_image_links,
    get_page_content,
    get_thread_info,
    save_image,
};
use clap::{
    Arg,
    ArgAction,
    ColorChoice,
    Command,
    ValueHint,
    crate_authors,
    crate_description,
    crate_version,
    value_parser,
};
use env_logger::fmt::style::{Ansi256Color, AnsiColor, Style};
use indicatif::{ProgressBar, ProgressStyle};
use log::{LevelFilter, error, info};
use reqwest::Client;

static DOWNLOADED_FILES: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Run `initialize_logging` one time
///
/// The place where this is used should only be ran once,
/// but this is a precaution
static ONCE: Once = Once::new();

fn main() -> Result<()> {
    let matches = build_app().get_matches();
    let verbosity = matches.get_one::<u8>("verbose").expect("Count always defaulted");

    initialize_logging(*verbosity);

    let thread = matches
        .get_one::<String>("thread")
        .context("failed to get 'thread' value")?;
    let output = matches
        .get_one::<String>("output")
        .map_or_else(|| String::from("downloads"), Clone::clone);
    let reload = matches.get_flag("reload");
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
        if let Err(err) = explore_thread(thread, &directory, *concurrent) {
            // Don't leave an empty directory behind (fails if it has files)
            let _ = remove_dir(&directory);
            return Err(err);
        }
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

/// Initialize logging for this crate
fn initialize_logging(verbosity: u8) {
    ONCE.call_once(|| {
        env_logger::Builder::new()
            .format_timestamp(None)
            .format(|buf, record| {
                let level_color = match record.level() {
                    log::Level::Warn => AnsiColor::Yellow,
                    log::Level::Info => AnsiColor::Green,
                    log::Level::Debug => AnsiColor::Magenta,
                    log::Level::Trace => AnsiColor::Cyan,
                    log::Level::Error => AnsiColor::Red,
                };
                let level_style = Style::new().fg_color(Some(level_color.into()));
                let target_style = Style::new().fg_color(Some(Ansi256Color(14).into()));

                writeln!(
                    buf,
                    " {level_style}{}{level_style:#}: {target_style}{}{target_style:#} {}",
                    record.level(),
                    record.target(),
                    record.args()
                )
            })
            .filter(None, match &verbosity {
                1 => LevelFilter::Warn,
                2 => LevelFilter::Info,
                3 => LevelFilter::Debug,
                4 => LevelFilter::Trace,
                _ => LevelFilter::Off,
            })
            .init();
    });
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
    let client = Client::builder()
        .user_agent(concat!("chan-downloader/", env!("CARGO_PKG_VERSION")))
        .build()?;

    match get_links(thread_link, &client).await {
        Ok(links_vec) => {
            info!("Loaded content from {}", thread_link);

            let total = links_vec.len();
            let failed = AtomicUsize::new(0);
            let pb = ProgressBar::new(total as u64);

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
                let failed = &failed;
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
                                info!("Saved image to {}", path);
                                let result = mark_as_downloaded(&path).unwrap();
                                info!("{} added to downloaded files", result);
                            },
                            Err(err) => {
                                failed.fetch_add(1, Ordering::Relaxed);
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

            let failed = failed.into_inner();
            if failed == total {
                bail!(
                    "could not download any of the {total} files of {thread_link}: the site may be \
                     blocking the downloader (e.g. a Cloudflare check)"
                );
            }
            if failed > 0 {
                eprintln!("Warning: {failed} of {total} files could not be downloaded");
            }
        },
        Err(e) => {
            error!("Failed to get content from {}", thread_link);
            return Err(e);
        },
    }

    Ok(())
}

/// Returns the image links of a thread: from the API for 4chan, from the page
/// for other sites. Fails if there are none, so that a blocked or changed page
/// is not silently treated as an empty thread.
async fn get_links(thread_link: &str, client: &Client) -> Result<Vec<chan_downloader::Link>> {
    let links = if let Some(api_url) = get_api_url(thread_link) {
        let board = get_thread_info(thread_link).board;
        let json = get_page_content(&api_url, client)
            .await
            .with_context(|| format!("failed to load thread {thread_link} from {api_url}"))?;
        let links = get_api_image_links(&json, &board)
            .with_context(|| format!("unexpected response from {api_url}"))?;
        if links.is_empty() {
            bail!("no images found in {thread_link}");
        }
        links
    } else {
        let page = get_page_content(thread_link, client)
            .await
            .with_context(|| format!("failed to load {thread_link}"))?;
        let links = get_image_links(&page);
        if links.is_empty() {
            bail!(
                "no images found in {thread_link}: the site may be blocking the downloader (e.g. a \
                 Cloudflare check) or its page layout may have changed"
            );
        }
        links
    };
    Ok(links)
}

fn create_directory(thread_link: &str, output: &str) -> Result<PathBuf> {
    let workpath = env::current_dir()?;
    info!("Working from {}", workpath.display());

    let thread = get_thread_info(thread_link);

    let directory = workpath
        .join(output)
        .join(thread.board)
        .join(format!("{}", thread.id));
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
fn build_app() -> Command {
    log::debug!("Building application");

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
        .infer_long_args(true)
        .dont_collapse_args_in_usage(true)
        .arg(
            Arg::new("thread")
                .short('t')
                .long("thread")
                .required(true)
                .value_name("URL")
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .help("URL of the thread"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("DIRECTORY")
                .value_hint(ValueHint::DirPath)
                .help("Output directory (Default is 'downloads')"),
        )
        // .arg(
        //     Arg::new("preserve_filenames")
        //         .short('p')
        //         .long("preserve-filenames")
        //         .action(ArgAction::SetTrue)
        //         .help("Preserve the filenames that are found on 4chan/4plebs"),
        // )
        .arg(
            Arg::new("reload")
                .short('r')
                .long("reload")
                .action(ArgAction::SetTrue)
                .help("Reload thread every t minutes to get new images"),
        )
        .arg(
            Arg::new("interval")
                .short('i')
                .long("interval")
                .value_name("INTERVAL")
                .value_parser(value_parser!(u64))
                .help("Time between each reload (in minutes. Default is 5)"),
        )
        .arg(
            Arg::new("limit")
                .short('l')
                .long("limit")
                .value_name("LIMIT")
                .value_parser(value_parser!(u64))
                .help("Time limit for execution (in minutes. Default is 120)"),
        )
        .arg(
            Arg::new("concurrent")
                .short('c')
                .long("concurrent")
                .value_name("NUM-REQUESTS")
                .value_parser(value_parser!(usize))
                .help("Number of concurrent requests (Default is 2)"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .hide(true)
                .action(ArgAction::Count)
                .help("Display debugging messages"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREAD: &str = "https://boards.4chan.org/wg/thread/6872254";

    #[test]
    fn verify_app() {
        build_app().debug_assert();
    }

    #[test]
    fn it_requires_a_thread() {
        let err = build_app().try_get_matches_from(["chan-downloader"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn it_rejects_an_empty_thread() {
        assert!(
            build_app()
                .try_get_matches_from(["chan-downloader", "-t", ""])
                .is_err()
        );
    }

    #[test]
    fn it_leaves_optional_args_unset_by_default() {
        let matches = build_app()
            .try_get_matches_from(["chan-downloader", "-t", THREAD])
            .unwrap();
        assert_eq!(matches.get_one::<String>("thread").unwrap(), THREAD);
        assert!(!matches.get_flag("reload"));
        assert_eq!(matches.get_count("verbose"), 0);
        assert!(matches.get_one::<String>("output").is_none());
        assert!(matches.get_one::<u64>("interval").is_none());
        assert!(matches.get_one::<u64>("limit").is_none());
        assert!(matches.get_one::<usize>("concurrent").is_none());
    }

    #[test]
    fn it_parses_all_args() {
        let matches = build_app()
            .try_get_matches_from([
                "chan-downloader",
                "-t",
                THREAD,
                "-o",
                "out",
                "-r",
                "-i",
                "10",
                "-l",
                "60",
                "-c",
                "4",
                "-vvv",
            ])
            .unwrap();
        assert_eq!(matches.get_one::<String>("output").unwrap(), "out");
        assert!(matches.get_flag("reload"));
        assert_eq!(*matches.get_one::<u64>("interval").unwrap(), 10);
        assert_eq!(*matches.get_one::<u64>("limit").unwrap(), 60);
        assert_eq!(*matches.get_one::<usize>("concurrent").unwrap(), 4);
        assert_eq!(matches.get_count("verbose"), 3);
    }

    #[test]
    fn it_infers_long_args() {
        let matches = build_app()
            .try_get_matches_from(["chan-downloader", "--thr", THREAD, "--conc", "3"])
            .unwrap();
        assert_eq!(*matches.get_one::<usize>("concurrent").unwrap(), 3);
    }

    #[test]
    fn it_rejects_non_numeric_values() {
        for arg in ["-i", "-l", "-c"] {
            let err = build_app()
                .try_get_matches_from(["chan-downloader", "-t", THREAD, arg, "abc"])
                .unwrap_err();
            assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
        }
    }

    #[test]
    fn it_creates_board_and_thread_directories() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("downloads");
        let created = create_directory(THREAD, output.to_str().unwrap()).unwrap();
        assert_eq!(created, output.join("wg").join("6872254"));
        assert!(created.is_dir());
        // Running again on an existing directory is fine
        assert_eq!(
            create_directory(THREAD, output.to_str().unwrap()).unwrap(),
            created
        );
    }

    #[test]
    fn it_tracks_downloaded_files() {
        let file = "/tmp/chan-downloader-test/1.png";
        assert_eq!(mark_as_downloaded(file).unwrap(), file);
        assert!(DOWNLOADED_FILES.lock().unwrap().contains(&file.to_string()));
    }
}
