use std::path::PathBuf;

use clap::Clap;

#[derive(Clap, Debug)]
pub struct Opts {
    /// Sets the state.json path
    #[clap(short, long, default_value = "state.json", value_name = "FILE")]
    pub state_path: PathBuf,
    /// The lower id of the range to crawl
    #[clap()]
    pub lower_id: Option<u64>,
    /// The upper id of the range to crawl
    #[clap()]
    pub upper_id: Option<u64>,
}
