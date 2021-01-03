chan-downloader
===============

CLI to download all images/webms of a 4chan thread.
If you use the reload flag, previously saved image won't be redownloaded.

```
USAGE:
    chan-downloader [FLAGS] [OPTIONS] --thread <thread>

FLAGS:
    -h, --help       Prints help information
    -r, --reload     Reload thread every t minutes to get new images
    -V, --version    Prints version information

OPTIONS:
    -i, --interval <interval>    Time between each reload (in minutes. Default is 5)
    -l, --limit <limit>          Time limit for execution (in minutes. Default is 120)
    -o, --output <output>        Output directory (Default is 'downloads')
    -t, --thread <thread>        URL of the thread
```

chan_downloader
===============
You can also use chan_downloader, the library used

## save_image
Saves the image from the url to the given path. Returns the path on success
```
use reqwest::Client;
use std::env;
use std::fs::remove_file;
let client = Client::new();
let workpath = env::current_dir().unwrap().join("1489266570954.jpg");
let url = "https://i.4cdn.org/wg/1489266570954.jpg";
let answer = chan_downloader::save_image(url, workpath.to_str().unwrap(), &client).unwrap();

assert_eq!(workpath.to_str().unwrap(), answer);
remove_file(answer).unwrap();
```

## get_page_content
Returns the page content from the given url.
```
use reqwest::Client;
let client = Client::new();
let url = "https://boards.4chan.org/wg/thread/6872254";
match chan_downloader::get_page_content(url, &client) {
    Ok(page) => println!("Content: {}", page),
    Err(err) => eprintln!("Error: {}", err),
}
```

## get_thread_infos
Returns the board name and thread id.
```
let url = "https://boards.4chan.org/wg/thread/6872254";
let (board_name, thread_id) = chan_downloader::get_thread_infos(url);
///
assert_eq!(board_name, "wg");
assert_eq!(thread_id, "6872254");
```

## get_image_links
Returns the links and the number of links from a page. Note that the links are doubled.
```
use reqwest::Client;
let client = Client::new();
let url = "https://boards.4chan.org/wg/thread/6872254";
match chan_downloader::get_page_content(url, &client) {
    Ok(page_string) => {
        let (links_iter, number_of_links) = chan_downloader::get_image_links(page_string.as_str());

        assert_eq!(number_of_links, 4);

        for cap in links_iter.step_by(2) {
            println!("{} and {}", &cap[1], &cap[2]);
        }
    },
    Err(err) => eprintln!("Error: {}", err),
}
```
