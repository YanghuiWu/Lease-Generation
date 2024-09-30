use crate::cli::Cli;
use crate::io::build_ri_hists;
use crate::lease_gen::{LeaseOperationContext, LeaseResults};
use crate::utils::*;
use regex::Regex;
use std::error::Error;

pub mod cli;
/// Small miscellaneous functions used
mod helpers;
/// Functions for parsing input files, debug prints, and lease output
pub mod io;
/// Core algorithms
pub mod lease_gen;
pub mod utils;

pub fn run_this(cli: Cli) -> f64 {
    let max_scopes = calculate_max_scopes(cli.mem_size, cli.llt_size);
    let num_ways = calculate_num_ways(cli.set_associativity, cli.cache_size);
    let set_mask = calculate_set_mask(cli.cache_size, num_ways);
    println!("num_ways: {}, set_mask: {}", num_ways, set_mask);

    let re = Regex::new(r"/(clam|shel).*/(.*?)\.(txt|csv)$").unwrap();
    let search_string = cli.input.to_lowercase();
    let cap = re
        .captures(&search_string)
        .ok_or("Failed to capture regex").unwrap();
    let empirical_rate = cli.empirical_sample_rate.to_lowercase();

    let (ri_hists, samples_per_phase, misses_from_first_access, empirical_sample_rate) =
        build_ri_hists(&cli.input, cli.cshel, set_mask);

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

    run_shel_cshel(&cli, &context, &cap)
}

pub fn run_prl(cli: &Cli, context: &LeaseOperationContext, cap: &regex::Captures) -> f64 {
    let (binned_ri_distributions, binned_freqs, bin_width) =
        crate::io::get_prl_hists(&cli.input, cli.prl, context.set_mask);

    if &cap[1] == "shel" {
        panic!("Error! You can only use prl on sampling files with a single phase!");
    }

    let output_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], "prl", "leases");

    let mut lease_results = crate::lease_gen::prl(
        cli,
        context,
        bin_width,
        &binned_ri_distributions,
        &binned_freqs,
    )
        .unwrap();
    lease_results.prune_leases_to_fit_llt(context.ri_hists, cli.llt_size);

    // generate_output_files(
    //     lease_results,
    //     cli,
    //     context,
    //     &output_file_name,
    //     "prl",
    //     &cap[2],
    // ).unwrap();
    get_misses(lease_results, context, cli)
}

pub fn run_shel_cshel(cli: &Cli, context: &LeaseOperationContext, cap: &regex::Captures) -> f64 {
    println!("running {}", &cap[1]);
    let output_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], &cap[1], "leases");

    let mut lease_results = crate::lease_gen::shel_cshel(false, cli, context).unwrap();
    lease_results.prune_leases_to_fit_llt(context.ri_hists, cli.llt_size);

    // generate_output_files(
    //     lease_results,
    //     cli,
    //     context,
    //     &output_file_name,
    //     &cap[1],
    //     &cap[2],
    // ).unwrap();
    //
    // if cli.cshel {
    //     println!("Running C-SHEL.");
    //     run_cshel(cli, cap, context);
    // }
    get_misses(lease_results, context, cli)
}

pub fn run_cshel(cli: &Cli, cap: &regex::Captures, context: &LeaseOperationContext) {
    println!("Running C-SHEL.");
    let mut lease_results = crate::lease_gen::shel_cshel(true, cli, context).unwrap();

    lease_results.prune_leases_to_fit_llt(context.ri_hists, cli.llt_size);

    let output_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], "c-shel", "leases");
    // generate_output_files(
    //     lease_results,
    //     cli,
    //     context,
    //     &output_file_name,
    //     "c-shel",
    //     &cap[2],
    // ).unwrap();
}


pub fn get_misses(
    lease_results: LeaseResults,
    context: &LeaseOperationContext,
    cli: &Cli,
) -> f64 {
    let (length, misses) = io::dump_leases(
        lease_results,
        &cli.output,
        context.sample_rate,
        context.misses_from_first_access,
    );

    let miss_rate:f64 = misses as f64 / length as f64;
    println!("length: {}, hits: {}, misses: {}", length, length - misses, miss_rate);

    miss_rate

    // let (length, hits) = io::dump_leases(
    //     lease_results,
    //     &cli.output,
    //     context.sample_rate,
    //     context.misses_from_first_access,
    // );
    //
    // let miss_rate:f64 = (length - hits) as f64 / length as f64;
    // println!("length: {}, hits: {}, misses: {}", length, hits, miss_rate);
    //
    // miss_rate
}

pub fn calculate_next_cache_size(cache_size: usize) -> usize {
    if cache_size == 1 {
        2
    } else if cache_size < 34 {
        cache_size + 2
    } else {
        let mut target = (cache_size * 11 + 5) / 10; // Equivalent to rounding cache_size * 1.1
        if target % 2 != 0 {
            target += 1; // Ensure target is even
        }
        let next_power_of_two = (cache_size + 1).next_power_of_two();
        if target < next_power_of_two {
            target
        } else {
            next_power_of_two
        }
    }
}


pub fn generate_output_files_(
    lease_results: LeaseResults,
    cli: &Cli,
    context: &LeaseOperationContext,
    output_file_name: &str,
    method: &str,
    cap_index: &str,
) -> Result<(), Box<dyn Error>> {
    let lease_vectors = crate::io::dump_leases(
        lease_results,
        output_file_name,
        context.sample_rate,
        context.misses_from_first_access,
    );

    // let output_lease_file_name = format!("{}/{}_{}_{}", cli.output, cap_index, method, "lease.c");
    //
    // crate::io::gen_lease_c_file(
    //     lease_vectors,
    //     cli,
    //     context.max_scopes,
    //     output_lease_file_name,
    // );

    Ok(())
}
