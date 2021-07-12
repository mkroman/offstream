use std::path::PathBuf;

use clap::Clap;

#[derive(Clap, Debug)]
#[clap(author, about, version)]
pub struct Opts {
    /// Sets the database path
    #[clap(short, long, default_value = "films.db", value_name = "FILE", env)]
    pub database_path: PathBuf,

    #[clap(flatten)]
    pub jaeger_opts: JaegerOpts,
}

#[derive(Clap, Debug)]
pub struct JaegerOpts {
    /// Sets whether jaeger exporting is enabled
    #[clap(
        long = "jaeger-enabled",
        parse(try_from_str),
        default_value = "true",
        env = "JAEGER_ENABLED"
    )]
    pub enabled: bool,

    /// Sets the jaeger service name
    #[clap(long = "jaeger-service-name", default_value = env!("CARGO_PKG_NAME"), env = "JAEGER_SERVICE_NAME")]
    pub service_name: String,
}
