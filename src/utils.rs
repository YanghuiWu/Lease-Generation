pub fn calculate_max_scopes(mem_size: u64, llt_size: u64) -> u64 {
    mem_size / ((2 * llt_size + 16) * 4)
}

pub fn calculate_num_ways(set_associativity: u64, cache_size: u64) -> u64 {
    match set_associativity {
        0 => cache_size,
        sa if sa > cache_size => {
            println!("The number of ways exceeds number of blocks in cache");
            panic!();
        }
        sa => sa,
    }
}

pub fn calculate_set_mask(cache_size: u64, num_ways: u64) -> u32 {
    if num_ways == 0 {
        panic!("Number of ways cannot be zero.");
    }
    let sets = cache_size / num_ways;
    if sets == 0 {
        panic!("Number of sets cannot be zero.");
    }
    (sets - 1) as u32
}

// Show me some example result for the calculate_set_mask function for fully associative cache
// Example: calculate_set_mask(1024, 1)
// return 1023

//  How to use this mask
//  suppose cache address is 0xffffffff
//  then the set is 0xffffffff & calculate_set_mask(1024, 1)
//  then it will be 0x3ff
