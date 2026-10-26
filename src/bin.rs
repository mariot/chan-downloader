// TODO: Implement --preserve-filenames
//       This would preserve the filenames that are given to the files on the
//       given website. It can be accomplished, by using their API.
//       Example API URLs:
// 4plebs: https://archive.4plebs.org/_/api/chan/thread?board=x&num=32661196
//  4chan: https://a.4cdn.org/po/thread/570368.json

use futures::stream::StreamExt;
use std::{
    collections::HashSet,
    env,
    fs::{create_dir_all, remove_dir},
    io::Write,
    path::PathBuf,
    sync::Once,
    time::{Duration, Instant},
};

use anyhow::{Context, Error, Result, anyhow, bail};
use chan_downloader::{
    Link,
    Page,
    client,
    get_api_image_links,
    get_api_url,
    get_image_links,
    get_page_content_since,
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

/// Run `initialize_logging` one time
///
/// The place where this is used should only be ran once,
/// but this is a precaution
static ONCE: Once = Once::new();

/// Minimum time between two thread requests (4chan API rules)
const REQUEST_INTERVAL: Duration = Duration::from_secs(1);

/// A thread being downloaded
struct ThreadJob {
    link:          String,
    directory:     PathBuf,
    /// `Last-Modified` header of the last load, to only reload if it changed
    last_modified: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = build_app().get_matches();
    let verbosity = matches.get_one::<u8>("verbose").expect("Count always defaulted");

    initialize_logging(*verbosity);

    let threads: Vec<&String> = matches
        .get_many::<String>("thread")
        .context("failed to get 'thread' value")?
        .collect();
    let output = matches
        .get_one::<String>("output")
        .map_or_else(|| String::from("downloads"), Clone::clone);
    let reload = matches.get_flag("reload");
    let interval = matches.get_one::<u64>("interval").unwrap_or(&5_u64);
    let limit = matches.get_one::<u64>("limit").unwrap_or(&120_u64);
    let concurrent = matches.get_one::<usize>("concurrent").unwrap_or(&8_usize);

    info!("Downloading images from {:?} to {}", threads, output);

    let mut jobs = threads
        .iter()
        .map(|thread| {
            Ok(ThreadJob {
                link:          thread.to_string(),
                directory:     create_directory(thread, &output)?,
                last_modified: None,
            })
        })
        .collect::<Result<Vec<ThreadJob>>>()?;
    let total = jobs.len();
    let mut errors: Vec<Error> = Vec::new();
    let client = client()?;
    let mut downloaded = HashSet::new();

    let start = Instant::now();
    let wait_time = Duration::from_secs(60 * interval);
    let limit_time = if reload {
        Duration::from_secs(60 * limit)
    } else {
        Duration::from_secs(0)
    };
    loop {
        let load_start = Instant::now();
        let failed = download_threads(&client, &mut jobs, &mut downloaded, *concurrent).await?;
        // A failed thread doesn't stop the others, and isn't reloaded
        for (index, err) in failed.into_iter().rev() {
            let job = jobs.remove(index);
            // Don't leave an empty directory behind (fails if it has files)
            let _ = remove_dir(&job.directory);
            if total > 1 {
                eprintln!("Error: {err:#}");
            }
            errors.push(err);
        }
        let runtime = start.elapsed();
        let load_runtime = load_start.elapsed();
        if jobs.is_empty() || runtime > limit_time {
            info!("Runtime exceeded, exiting.");
            break;
        };
        if let Some(remaining) = wait_time.checked_sub(load_runtime) {
            info!("Schedule slice has time left over; sleeping for {:?}", remaining);
            tokio::time::sleep(remaining).await;
        }
        info!("Downloader executed one more time for {:?}", load_runtime);
    }

    match errors.len() {
        0 => Ok(()),
        _ if total == 1 => Err(errors.remove(0)),
        failed => bail!("{failed} of {total} threads failed"),
    }
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

/// Downloads the new files of all threads through one shared queue of
/// `concurrent` downloads. `downloaded` holds the files already saved, to skip
/// them on reloads. Returns the errors of the threads that failed, by index in
/// `jobs`.
async fn download_threads(
    client: &Client,
    jobs: &mut [ThreadJob],
    downloaded: &mut HashSet<PathBuf>,
    concurrent: usize,
) -> Result<Vec<(usize, Error)>> {
    let start = Instant::now();
    let mut errors = Vec::new();
    let mut links_per_thread = vec![0; jobs.len()];
    let mut queue = Vec::new();

    for (index, job) in jobs.iter_mut().enumerate() {
        if index > 0 {
            tokio::time::sleep(REQUEST_INTERVAL).await;
        }
        match get_links(&job.link, client, job.last_modified.as_deref()).await {
            Ok(None) => info!("{} not modified since last load", job.link),
            Ok(Some((links, last_modified))) => {
                info!("Loaded content from {}", job.link);
                job.last_modified = last_modified;
                links_per_thread[index] = links.len();
                for link in links {
                    let path = job.directory.join(&link.name);
                    if downloaded.contains(&path) {
                        info!("Image {} previously downloaded. Skipped", path.display());
                    } else if path.exists() {
                        info!("Image {} already exists. Skipped", path.display());
                        downloaded.insert(path);
                    } else {
                        queue.push((index, format!("https:{}", link.url), path));
                    }
                }
            },
            Err(err) => {
                error!("Failed to get content from {}", job.link);
                errors.push((index, err));
            },
        }
    }

    let pb = ProgressBar::new(queue.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green.bold} [{elapsed_precise}] [{bar:40.cyan.bold/blue}] {pos}/{len} {msg} \
                 ({eta})",
            )
            .context("failed to build progress bar")?
            .progress_chars("#>-"),
    );
    let label = match jobs {
        [job] => job.link.clone(),
        _ => format!("{} threads", jobs.len()),
    };
    pb.set_message(label.clone());
    pb.tick();

    let results: Vec<(usize, PathBuf, bool)> = futures::stream::iter(queue)
        .map(|(index, url, path)| {
            let pb = &pb;
            async move {
                let saved = match save_image(&url, path.to_str().unwrap(), client).await {
                    Ok(saved) => {
                        info!("Saved image to {}", saved);
                        true
                    },
                    Err(err) => {
                        error!("Couldn't save image {}", path.display());
                        pb.suspend(|| eprintln!("Error: {err}"));
                        false
                    },
                };
                pb.inc(1);
                (index, path, saved)
            }
        })
        .buffer_unordered(concurrent)
        .collect()
        .await;

    pb.finish_with_message(format!("{label} done"));
    info!("Done in {:?}", start.elapsed());

    let mut failed_per_thread = vec![0; jobs.len()];
    for (index, path, saved) in results {
        if saved {
            downloaded.insert(path);
        } else {
            failed_per_thread[index] += 1;
        }
    }
    for (index, job) in jobs.iter().enumerate() {
        let (failed, total) = (failed_per_thread[index], links_per_thread[index]);
        if failed > 0 && failed == total {
            errors.push((
                index,
                anyhow!(
                    "could not download any of the {total} files of {}: the site may be blocking the \
                     downloader (e.g. a Cloudflare check)",
                    job.link
                ),
            ));
        } else if failed > 0 {
            eprintln!(
                "Warning: {failed} of {total} files of {} could not be downloaded",
                job.link
            );
        }
    }
    errors.sort_by_key(|(index, _)| *index);
    Ok(errors)
}

/// Returns the image links of a thread and its `Last-Modified` header: from the
/// API for 4chan, from the page for other sites. Returns `None` if it wasn't
/// modified since `last_modified`. Fails if there are no links, so that a
/// blocked or changed page is not silently treated as an empty thread.
async fn get_links(
    thread_link: &str,
    client: &Client,
    last_modified: Option<&str>,
) -> Result<Option<(Vec<Link>, Option<String>)>> {
    let api_url = get_api_url(thread_link);
    let url = api_url.as_deref().unwrap_or(thread_link);
    let (content, last_modified) = match get_page_content_since(url, client, last_modified)
        .await
        .with_context(|| match &api_url {
            Some(api_url) => format!("failed to load thread {thread_link} from {api_url}"),
            None => format!("failed to load {thread_link}"),
        })? {
        Page::NotModified => return Ok(None),
        Page::Modified { content, last_modified } => (content, last_modified),
    };

    let links = if let Some(api_url) = &api_url {
        let board = get_thread_info(thread_link).board;
        let links = get_api_image_links(&content, &board)
            .with_context(|| format!("unexpected response from {api_url}"))?;
        if links.is_empty() {
            bail!("no images found in {thread_link}");
        }
        links
    } else {
        let links = get_image_links(&content);
        if links.is_empty() {
            bail!(
                "no images found in {thread_link}: the site may be blocking the downloader (e.g. a \
                 Cloudflare check) or its page layout may have changed"
            );
        }
        links
    };
    Ok(Some((links, last_modified)))
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
                .num_args(1..)
                .action(ArgAction::Append)
                .value_name("URL")
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .help("URL of the thread (several can be given)"),
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
                .help("Number of concurrent downloads (Default is 8)"),
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
    fn it_accepts_several_threads() {
        const OTHER: &str = "https://boards.4chan.org/p/thread/4535213";
        for args in [
            vec!["chan-downloader", "-t", THREAD, OTHER],
            vec!["chan-downloader", "-t", THREAD, "-t", OTHER],
            vec!["chan-downloader", "-t", THREAD, "-r", "--thread", OTHER],
        ] {
            let matches = build_app().try_get_matches_from(&args).unwrap();
            let threads: Vec<&String> = matches.get_many::<String>("thread").unwrap().collect();
            assert_eq!(threads, [THREAD, OTHER], "{args:?}");
        }
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
}
