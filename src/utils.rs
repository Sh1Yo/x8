use std::{
    fs::File,
    io::{self, BufRead, Write},
    path::Path,
};

use indicatif::{ProgressBar, MultiProgress, ProgressStyle};
use rand::Rng;
use colored::*;

use crate::{config::structs::Config, RANDOM_CHARSET};
use crate::network::{response::Response};
use crate::runner::utils::ReasonKind;

/// notify about found parameters
pub fn notify(progress_bar: &ProgressBar, config: &Config, reason_kind: ReasonKind, response: &Response, diffs: Option<&String>) {
    if config.verbose > 1 {
        match reason_kind {
            ReasonKind::Code => progress_bar.println(
            format!(
                    "{} {}",
                    response.code(),
                    response.text.len()
                )
            ),
            ReasonKind::Text => progress_bar.println(
            format!(
                    "{} {} ({})",
                    response.code,
                    response.text.len().to_string().bright_yellow(),
                    diffs.unwrap()
                )
            ),
            _ => unreachable!()
        }
    }
}

/// prints informative messages/non critical errors
pub fn info<S: Into<String>, T: std::fmt::Display>(config: &Config, id: usize, progress_bar: &ProgressBar, word: S, msg: T) {
    if config.verbose > 0 {
        progress_bar.println(format!("{} [{}] {}", color_id(id), word.into().yellow(), msg));
    }
}

/// prints errors. Progress_bar may be null in case the error happened too early (before requests)
pub fn error<T: std::fmt::Display>(msg: T, url: Option<&str>, progress_bar: Option<&ProgressBar>) {
    let message = if url.is_none() {
        format!("{} {}", "[#]".red(), msg)
    } else {
        format!("{} [{}] {}", "[#]".red(), url.unwrap(), msg)
    };

    if progress_bar.is_none() {
        writeln!(io::stdout(), "{}", message).ok();
    } else {
        progress_bar.unwrap().println(message);
    }
}

/// initialize progress bars for every url
pub fn init_progress(config: &Config) -> Vec<(String, ProgressBar)> {
    let mut url_to_progress = Vec::new();
    let m = MultiProgress::new();

    //we're creating an empty progress bar to make one empty line between progress bars and the tool's output
    let empty_line = m.add(ProgressBar::new(128));
    let sty = ProgressStyle::with_template(" ",).unwrap();
    empty_line.set_style(sty);
    empty_line.inc(1);
    url_to_progress.push((String::new(), empty_line));

    //append progress bars one after another and push them to url_to_progress
    for url in config.urls.iter() {
        let pb = m.insert_from_back(
                0,
                if config.disable_progress_bar || config.verbose < 1 {
                    ProgressBar::new(128)
                } else {
                    ProgressBar::hidden()
                }
        );

        url_to_progress.push((
            url.to_owned(),
            pb.clone()
        ));
    }

    url_to_progress
}

/// read wordlist with parameters
pub fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

/// read parameters from stdin
pub fn read_stdin_lines() -> Vec<String> {
    let stdin = io::stdin();
    stdin.lock().lines().filter_map(|x| x.ok()).collect()
}

/// generate random word of RANDOM_CHARSET chars
pub fn random_line(size: usize) -> String {
    (0..size)
        .map(|_| {
            let idx = rand::thread_rng().gen_range(0,RANDOM_CHARSET.len());
            RANDOM_CHARSET[idx] as char
        })
        .collect()
}

/// returns colored id when > 1 url is being tested in the same time
pub fn color_id(id: usize) -> String {
    if id % 7 == 0 {
        id.to_string().white()
    } else if id % 6 == 0 {
        id.to_string().bright_red()
    } else if id % 5 == 0 {
        id.to_string().bright_cyan()
    } else if id % 4 == 0 {
        id.to_string().bright_blue()
    } else if id % 3 == 0 {
        id.to_string().yellow()
    } else if id % 2 == 0 {
        id.to_string().bright_green()
    } else if id % 1 == 0 {
        id.to_string().magenta()
    } else {
        unreachable!()
    }.to_string()
}