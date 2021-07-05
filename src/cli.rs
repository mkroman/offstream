use std::path::PathBuf;

use clap::Clap;

#[derive(Clap, Debug)]
pub struct Opts {
    /// Sets the database path
    #[clap(short, long, default_value = "films.db", value_name = "FILE")]
    pub database_path: PathBuf,
}
