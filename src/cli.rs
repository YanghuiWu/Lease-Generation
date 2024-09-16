use clap::Parser;

#[derive(Parser)]
#[command(
    name = "clam",
    version = "2.0",
    author = "B. Reber <breber@cs.rochester.edu>, M. Gould <mdg2838@rit.edu>",
    about = "Lease assignment generator for phased traces"
)]
pub struct Cli {
    /// Sets the input file name
    pub input: String,

    /// Sets the output file location
    pub output: String,

    /// Target cache size for algorithms
    #[arg(short = 's', long, required = true)]
    pub cache_size: u64,

    /// Set associativity of the cache being targeted
    #[arg(short = 'a', long, default_value = "0")]
    pub set_associativity: u64,

    /// Calculate leases for PRL (only for non-phased sampling files)
    #[arg(short = 'p', long, default_value = "0", default_missing_value = "5")]
    pub prl: u64,

    /// Calculate leases for CSHEL
    #[arg(short = 'c', long)]
    pub cshel: bool,

    /// Output information about lease assignment
    #[arg(short, long)]
    pub verbose: bool,

    /// Number of elements in the lease lookup table
    #[arg(short = 'L', long, default_value = "128")]
    pub llt_size: u64,

    /// Total memory allocated for lease information
    #[arg(short = 'M', long, default_value = "65536")]
    pub mem_size: u64,

    /// Bit width available for discretized short lease probability
    #[arg(short = 'D', long, default_value = "9")]
    pub discretize_width: u64,

    /// Enable even more information about lease assignment
    #[arg(short = 'd', long)]
    pub debug: bool,

    /// Benchmark sampling rate
    #[arg(short = 'S', long, default_value = "256")]
    pub sampling_rate: u64,

    /// Use given or empirically derived sampling rate
    #[arg(short = 'E', long, default_value = "yes")]
    pub empirical_sample_rate: String,
}