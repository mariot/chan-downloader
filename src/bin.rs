#[macro_use]
extern crate clap;

use clap::App;

use chandownloader::download_thread;

fn main() {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let thread = matches.value_of("thread").unwrap();
    let output = matches.value_of("output").unwrap_or("downloads");
    download_thread(thread, &output);
}
