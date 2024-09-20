use clam::cli::Cli;
use clam::lease_gen::LeaseOperationContext;
use clam::utils::*;
use clam::{run_prl, run_shel_cshel};
use clap::Parser;
use regex::Regex;

fn main() {
    let cli = Cli::parse();

    let max_scopes = calculate_max_scopes(cli.mem_size, cli.llt_size);
    let num_ways = calculate_num_ways(cli.set_associativity, cli.cache_size);
    let set_mask = calculate_set_mask(cli.cache_size, num_ways);

    let re = Regex::new(r"/(clam|shel).*/(.*?)\.(txt|csv)$").unwrap();
    let search_string = cli.input.to_lowercase();
    let cap = re.captures(&search_string).unwrap();
    println!("Running {} on file {}", &cap[1], &cap[2]);
    let empirical_rate = cli.empirical_sample_rate.to_lowercase();

    let (ri_hists, samples_per_phase, misses_from_first_access, empirical_sample_rate) =
        clam::io::build_ri_hists(&cli.input, cli.cshel, set_mask);

    let sample_rate = if empirical_rate == "no" {
        cli.sampling_rate
    } else {
        empirical_sample_rate
    };

    // Create the context struct
    let context = LeaseOperationContext {
        ri_hists: &ri_hists,
        sample_rate,
        samples_per_phase: &samples_per_phase,
        set_mask,
        misses_from_first_access,
        max_scopes,
    };

    if cli.prl > 0 {
        run_prl(&cli, &context, &cap);
    }

    run_shel_cshel(&cli, &context, &cap);
}

// tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main() {
        // let cli = Cli {
        //     input: "input.txt".to_string(),
        //     output: "output.txt".to_string(),
        //     cache_size: 256,
        //     set_associativity: 0,
        //     prl: 0,
        //     cshel: false,
        //     verbose: false,
        //     llt_size: 128,
        //     mem_size: 65536,
        //     discretize_width: 9,
        //     debug: false,
        //     sampling_rate: 256,
        //     empirical_sample_rate: "yes".to_string(),
        // };
        let mut cli = Cli::default();
        cli.input = "tests/clam/gemm_small_trace.csv".to_string();
        // run_clam(cli).unwrap();
    }
}
