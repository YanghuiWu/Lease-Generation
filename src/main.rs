use std::process::Command;
use clam::cli::Cli;
use clam::{calculate_next_cache_size, run_this};
use clap::Parser;


fn grinding() {
    let trace_path = "./tests/clam/access_trace.csv";
    let clam_out_dir = "./tests/out";
    let miss_curve = format!("{}/clam_misses", clam_out_dir);
    let output_plot = format!("{}/.png", miss_curve);

    let mut wtr = csv::Writer::from_path(miss_curve.clone()).unwrap();
    wtr.write_record(["cache_size", "miss_ratio"]).unwrap();
    // println!("writing to file");

    let mut cache_size: usize = 1;
    while cache_size <= 256 {
        // print!("\n{}, ", cache_size);

        let mut cli = Cli::default();
        cli.input = trace_path.to_string();
        cli.output = clam_out_dir.to_string();
        cli.cache_size = cache_size as u64;
        let miss = run_this(cli);


        wtr.write_record(&[cache_size.to_string(), miss.to_string()])
            .unwrap();
        cache_size = calculate_next_cache_size(cache_size);
        // break;
        println!();
    }

    wtr.flush().expect("TODO: panic message");

    // Call the Python script to generate the plot
    Command::new("../locality_dir/constructive_opt/venv/bin/python")
        .arg("src/plot_opt_miss_ratio.py")
        .arg(miss_curve.clone())
        .arg(miss_curve).status().unwrap();
}

fn main() {
    grinding();

    // let cli = Cli::parse();
    //
    // let max_scopes = calculate_max_scopes(cli.mem_size, cli.llt_size);
    // let num_ways = calculate_num_ways(cli.set_associativity, cli.cache_size);
    // let set_mask = calculate_set_mask(cli.cache_size, num_ways);
    //
    // let re = Regex::new(r"/(clam|shel).*/(.*?)\.(txt|csv)$").unwrap();
    // let search_string = cli.input.to_lowercase();
    // let cap = re.captures(&search_string).unwrap();
    // println!("Running {} on file {}", &cap[1], &cap[2]);
    // let empirical_rate = cli.empirical_sample_rate.to_lowercase();
    //
    // let (ri_hists, samples_per_phase, misses_from_first_access, empirical_sample_rate) =
    //     clam::io::build_ri_hists(&cli.input, cli.cshel, set_mask);
    //
    // let sample_rate = if empirical_rate == "no" {
    //     cli.sampling_rate
    // } else {
    //     empirical_sample_rate
    // };
    //
    // // Create the context struct
    // let context = LeaseOperationContext {
    //     ri_hists: &ri_hists,
    //     sample_rate,
    //     samples_per_phase: &samples_per_phase,
    //     set_mask,
    //     misses_from_first_access,
    //     max_scopes,
    // };
    //
    // if cli.prl > 0 {
    //     run_prl(&cli, &context, &cap);
    // }
    //
    // run_shel_cshel(&cli, &context, &cap);
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
        cli.input = "tests/clam/access_trace.csv".to_string();
        run_this(cli);
        // run_clam(cli).unwrap();
    }
}
