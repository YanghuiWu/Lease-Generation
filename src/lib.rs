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

pub fn run_clam(cli: Cli) -> Result<(), Box<dyn Error>> {
    let max_scopes = calculate_max_scopes(cli.mem_size, cli.llt_size);
    let num_ways = calculate_num_ways(cli.set_associativity, cli.cache_size);
    let set_mask = calculate_set_mask(cli.cache_size, num_ways);

    let re = Regex::new(r"/(clam|shel).*/(.*?)\.(txt|csv)$")?;
    let search_string = cli.input.to_lowercase();
    let cap = re
        .captures(&search_string)
        .ok_or("Failed to capture regex")?;
    println!("Running {} on file {}", &cap[1], &cap[2]);
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

    run_shel_cshel(&cli, &context, &cap);

    Ok(())
}

pub fn run_prl(cli: &Cli, context: &LeaseOperationContext, cap: &regex::Captures) {
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

    generate_output_files(
        lease_results,
        cli,
        context,
        &output_file_name,
        "prl",
        &cap[2],
    )
    .unwrap();
}

pub fn run_shel_cshel(cli: &Cli, context: &LeaseOperationContext, cap: &regex::Captures) {
    println!("running {}", &cap[1]);
    let output_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], &cap[1], "leases");

    let mut lease_results = crate::lease_gen::shel_cshel(false, cli, context).unwrap();
    lease_results.prune_leases_to_fit_llt(context.ri_hists, cli.llt_size);

    generate_output_files(
        lease_results,
        cli,
        context,
        &output_file_name,
        &cap[1],
        &cap[2],
    )
    .unwrap();

    if cli.cshel {
        println!("Running C-SHEL.");
        run_cshel(cli, cap, context);
    }
}

pub fn run_cshel(cli: &Cli, cap: &regex::Captures, context: &LeaseOperationContext) {
    println!("Running C-SHEL.");
    let mut lease_results = crate::lease_gen::shel_cshel(true, cli, context).unwrap();

    lease_results.prune_leases_to_fit_llt(context.ri_hists, cli.llt_size);

    let output_file_name = format!("{}/{}_{}_{}", cli.output, &cap[2], "c-shel", "leases");
    generate_output_files(
        lease_results,
        cli,
        context,
        &output_file_name,
        "c-shel",
        &cap[2],
    )
    .unwrap();
}

pub fn generate_output_files(
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
