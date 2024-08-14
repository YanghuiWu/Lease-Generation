use std::collections::HashMap;
use clap::Parser;
use regex::Regex;
use clam;
use clam::lease_gen::RIHists;

#[derive(Parser)]
#[command(name = "clam", version = "1.0", author = "B. Reber <breber@cs.rochester.edu>, M. Gould <mdg2838@rit.edu>", about = "Lease assignment generator for phased traces")]
struct Cli {
    /// Sets the input file name
    input: String,

    /// Sets the output file location
    output: String,

    /// Target cache size for algorithms
    #[arg(short = 's', long, required = true)]
    cache_size: u64,

    /// Set associativity of the cache being targeted
    #[arg(short = 'a', long, default_value = "0")]
    set_associativity: u64,

    /// Calculate leases for PRL (only for non-phased sampling files)
    #[arg(short = 'p', long, default_value = "5")]
    prl: u64,

    /// Calculate leases for CSHEL
    #[arg(short = 'c', long)]
    cshel: bool,

    /// Output information about lease assignment
    #[arg(short, long)]
    verbose: bool,

    /// Number of elements in the lease lookup table
    #[arg(short = 'L', long, default_value = "128")]
    llt_size: u64,

    /// Total memory allocated for lease information
    #[arg(short = 'M', long, default_value = "65536")]
    mem_size: u64,

    /// Bit width available for discretized short lease probability
    #[arg(short = 'D', long, default_value = "9")]
    discretize_width: u64,

    /// Enable even more information about lease assignment
    #[arg(short = 'd', long)]
    debug: bool,

    /// Benchmark sampling rate
    #[arg(short = 'S', long, default_value = "256")]
    sampling_rate: u64,

    /// Use given or empirically derived sampling rate
    #[arg(short = 'E', long, default_value = "yes")]
    empirical_sample_rate: String,
}

fn main() {
    let cli = Cli::parse();

    let max_scopes = calculate_max_scopes(cli.mem_size, cli.llt_size);
    let num_ways = calculate_num_ways(cli.set_associativity, cli.cache_size);
    let set_mask = calculate_set_mask(cli.cache_size, num_ways);

    let re = Regex::new(r"/(clam|shel).*/(.*?)\.(txt|csv)$").unwrap();
    let search_string = cli.input.to_lowercase();
    let cap = re.captures(&search_string).unwrap();
    let empirical_rate = cli.empirical_sample_rate.to_lowercase();

    let (ri_hists, samples_per_phase, misses_from_first_access, empirical_sample_rate) =
        clam::io::build_ri_hists(&cli.input, cli.cshel, set_mask);

    let sample_rate = if empirical_rate == "no" {
        cli.sampling_rate
    } else {
        empirical_sample_rate
    };

    if cli.prl > 0 {
        run_prl(
            &cli, &cap, set_mask, sample_rate, &ri_hists, &samples_per_phase, misses_from_first_access, max_scopes,
        );
    }

    run_shel_cshel(
        &cli, &cap, set_mask, sample_rate, &ri_hists, &samples_per_phase, misses_from_first_access, max_scopes,
    );
}

fn calculate_max_scopes(mem_size: u64, llt_size: u64) -> u64 {
    mem_size / ((2 * llt_size + 16) * 4)
}

fn calculate_num_ways(set_associativity: u64, cache_size: u64) -> u64 {
    match set_associativity {
        0 => cache_size,
        sa if sa > cache_size => {
            println!("The number of ways exceeds number of blocks in cache");
            panic!();
        }
        sa => sa,
    }
}

fn calculate_set_mask(cache_size: u64, num_ways: u64) -> u32 {
    (cache_size as f64 / num_ways as f64) as u32 - 1
}

fn run_prl(
    cli: &Cli, cap: &regex::Captures, set_mask: u32, sample_rate: u64,
    ri_hists: &RIHists, samples_per_phase: &HashMap<u64,u64>, misses_from_first_access: usize, max_scopes: u64,
) {
    let (binned_ri_distributions, binned_freqs, bin_width) =
        clam::io::get_prl_hists(&cli.input, cli.prl, set_mask);

    if &cap[1] == "shel" {
        panic!("Error! You can only use prl on sampling files with a single phase!");
    }

    let output_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], "prl", "leases");

    let (leases, dual_leases, lease_hits, trace_length) = clam::lease_gen::prl(
        bin_width, ri_hists, &binned_ri_distributions, &binned_freqs, sample_rate,
        cli.cache_size, cli.discretize_width, samples_per_phase, cli.verbose, cli.debug, set_mask,
    ).unwrap();

    let (leases, dual_leases) = clam::lease_gen::prune_leases_to_fit_llt(
        leases, dual_leases, ri_hists, cli.llt_size,
    );

    let lease_vectors = clam::io::dump_leases(
        leases, dual_leases, lease_hits, trace_length, &output_file_name, sample_rate, misses_from_first_access,
    );

    let output_lease_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], "prl", "lease.c");

    clam::io::gen_lease_c_file(
        lease_vectors, cli.llt_size, max_scopes, cli.mem_size, output_lease_file_name, cli.discretize_width,
    );
}

fn run_shel_cshel(
    cli: &Cli, cap: &regex::Captures, set_mask: u32, sample_rate: u64,
    ri_hists: &RIHists, samples_per_phase: &HashMap<u64,u64>, misses_from_first_access: usize, max_scopes: u64,
) {
    println!("running {}", &cap[1]);
    let output_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], &cap[1], "leases");

    let (leases, dual_leases, lease_hits, trace_length) = clam::lease_gen::shel_cshel(
        false, ri_hists, cli.cache_size, sample_rate, samples_per_phase, cli.discretize_width,
        cli.verbose, cli.debug, set_mask,
    ).unwrap();

    let (leases, dual_leases) = clam::lease_gen::prune_leases_to_fit_llt(
        leases, dual_leases, ri_hists, cli.llt_size,
    );

    let lease_vectors = clam::io::dump_leases(
        leases, dual_leases, lease_hits, trace_length, &output_file_name, sample_rate, misses_from_first_access,
    );

    let output_lease_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], &cap[1], "lease.c");

    clam::io::gen_lease_c_file(
        lease_vectors, cli.llt_size, max_scopes, cli.mem_size, output_lease_file_name, cli.discretize_width,
    );

    if cli.cshel {
        println!("Running C-SHEL.");
        run_cshel(cli, cap, set_mask, sample_rate, ri_hists, samples_per_phase, misses_from_first_access, max_scopes);
    }
}

fn run_cshel(
    cli: &Cli, cap: &regex::Captures, set_mask: u32, sample_rate: u64,
    ri_hists: &RIHists, samples_per_phase: &HashMap<u64,u64>, misses_from_first_access: usize, max_scopes: u64,
) {
    let (leases, dual_leases, lease_hits, trace_length) = clam::lease_gen::shel_cshel(
        true, ri_hists, cli.cache_size, sample_rate, samples_per_phase, cli.discretize_width,
        cli.verbose, cli.debug, set_mask,
    ).unwrap();

    let (leases, dual_leases) = clam::lease_gen::prune_leases_to_fit_llt(
        leases, dual_leases, ri_hists, cli.llt_size,
    );

    let output_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], "c-shel", "leases");
    let output_lease_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], "c-shel", "lease.c");

    let lease_vectors = clam::io::dump_leases(
        leases, dual_leases, lease_hits, trace_length, &output_file_name, sample_rate, misses_from_first_access,
    );

    clam::io::gen_lease_c_file(
        lease_vectors, cli.llt_size, max_scopes, cli.mem_size, output_lease_file_name, cli.discretize_width,
    );
}



// fn main_old() {
//     let cli = Cli::parse();
//
//     let cache_size = cli.cache_size;
//     let perl_bin_num = cli.prl;
//     let llt_size = cli.llt_size;
//     let mem_size = cli.mem_size;
//     // Get maximum number of scopes that can fit in given memory size with given llt size
//     let max_scopes = mem_size / ((2 * llt_size + 16) * 4);
//     let discretize_width = cli.discretize_width;
//     let verbose = cli.verbose;
//     let debug = cli.debug;
//     let cshel = cli.cshel;
//     let prl = cli.prl > 0;
//
//     let re = Regex::new(r"/(clam|shel).*/(.*?)\.(txt|csv)$").unwrap();
//     let search_string = cli.input.to_lowercase();
//     let cap = re.captures(&search_string).unwrap();
//     let empirical_rate = cli.empirical_sample_rate.to_lowercase();
//
//     // If associativity not specified, set as fully associative
//     let num_ways = if cli.set_associativity == 0 {
//         cache_size
//     } else if cli.set_associativity > cache_size {
//         println!("The number of ways exceeds number of blocks in cache");
//         panic!();
//     } else {
//         cli.set_associativity
//     };
//
//     // Get mask for set bits
//     let set_mask = (cache_size as f64 / num_ways as f64) as u32 - 1;
//
//     // Generate distributions
//     let (ri_hists, samples_per_phase, misses_from_first_access, empirical_sample_rate) =
//         clam::io::build_ri_hists(&cli.input, cshel, set_mask);
//
//     // If specified used empirical sampling rate else use given or default rate
//     let sample_rate = if empirical_rate == "no" {
//         cli.sampling_rate
//     } else {
//         empirical_sample_rate
//     };
//
//     let mut output_file_name: String;
//
//     if prl {
//         // Generate bins
//         let (binned_ri_distributions, binned_freqs, bin_width) =
//             clam::io::get_prl_hists(&cli.input, perl_bin_num, set_mask);
//
//         // Compose output file name
//         if &cap[1] == "shel" {
//             panic!("Error! You can only use prl on sampling files with a single phase!");
//         }
//         output_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], "prl", "leases");
//
//         // Generate prl leases
//         let (leases, dual_leases, lease_hits, trace_length) = clam::lease_gen::prl(
//             bin_width,
//             &ri_hists,
//             &binned_ri_distributions,
//             &binned_freqs,
//             sample_rate,
//             cache_size,
//             discretize_width,
//             &samples_per_phase,
//             verbose,
//             debug,
//             set_mask,
//         ).unwrap();
//
//         let (leases, dual_leases) = clam::lease_gen::prune_leases_to_fit_llt(
//             leases,
//             dual_leases,
//             &ri_hists,
//             llt_size,
//         );
//
//         println!("running PRL");
//
//         let lease_vectors = clam::io::dump_leases(
//             leases,
//             dual_leases,
//             lease_hits,
//             trace_length,
//             &output_file_name,
//             sample_rate,
//             misses_from_first_access,
//         );
//
//         let output_lease_file_name = format!(
//             "{}/{}_{}_{}",
//             cli.output, &cap[2], "prl", "lease.c"
//         );
//
//         clam::io::gen_lease_c_file(
//             lease_vectors,
//             llt_size,
//             max_scopes,
//             mem_size,
//             output_lease_file_name,
//             discretize_width,
//         );
//     }
//
//     println!("running {}", &cap[1]);
//     output_file_name = format!(
//         "{}/{}_{}_{}",
//         cli.output, &cap[2], &cap[1], "leases"
//     );
//
//     // Generates based on input file phases, CLAM or SHEL
//     let (leases, dual_leases, lease_hits, trace_length) = clam::lease_gen::shel_cshel(
//         false,
//         &ri_hists,
//         cache_size,
//         sample_rate,
//         &samples_per_phase,
//         discretize_width,
//         verbose,
//         debug,
//         set_mask,
//     ).unwrap();
//
//     let (leases, dual_leases) = clam::lease_gen::prune_leases_to_fit_llt(
//         leases,
//         dual_leases,
//         &ri_hists,
//         llt_size,
//     );
//
//     let lease_vectors = clam::io::dump_leases(
//         leases,
//         dual_leases,
//         lease_hits,
//         trace_length,
//         &output_file_name,
//         sample_rate,
//         misses_from_first_access,
//     );
//
//     let output_lease_file_name = format!(
//         "{}/{}_{}_{}",
//         cli.output, &cap[2], &cap[1], "lease.c"
//     );
//
//     // Generate lease file
//     clam::io::gen_lease_c_file(
//         lease_vectors,
//         llt_size,
//         max_scopes,
//         mem_size,
//         output_lease_file_name,
//         discretize_width,
//     );
//
//     // Generate CSHEL if option specified
//     if prl {
//         // Generate leases
//         println!("Running C-SHEL.");
//         let (leases, dual_leases, lease_hits, trace_length) = clam::lease_gen::shel_cshel(
//             true,
//             &ri_hists,
//             cache_size,
//             sample_rate,
//             &samples_per_phase,
//             discretize_width,
//             verbose,
//             debug,
//             set_mask,
//         ).unwrap();
//
//         let (leases, dual_leases) = clam::lease_gen::prune_leases_to_fit_llt(
//             leases,
//             dual_leases,
//             &ri_hists,
//             llt_size,
//         );
//
//         // Compose output file name
//         output_file_name = format!(
//             "{}/{}_{}_{}",
//             cli.output, &cap[2], "c-shel", "leases"
//         );
//
//         // Output to file
//         let output_lease_file_name = format!(
//             "{}/{}_{}_{}",
//             cli.output, &cap[2], "c-shel", "lease.c"
//         );
//
//         let lease_vectors = clam::io::dump_leases(
//             leases,
//             dual_leases,
//             lease_hits,
//             trace_length,
//             &output_file_name,
//             sample_rate,
//             misses_from_first_access,
//         );
//
//         clam::io::gen_lease_c_file(
//             lease_vectors,
//             llt_size,
//             max_scopes,
//             mem_size,
//             output_lease_file_name,
//             discretize_width,
//         );
//     }
// }
