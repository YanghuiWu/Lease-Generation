use std::{
    collections::{BinaryHeap, HashMap},
    default,
};

use crate::{cli::*, lease_gen::*};

//Output:
//leases: Hashmap<u64,u64>
//dual_leases: HashMap<u64, (f64, u64)>
//lease_hits: HashMap<u64, HashMap<u64,u64>>
//trace_length: u64
pub fn shel_cshel(cshel: bool, cli: &Cli, context: &LeaseOperationContext) -> Option<LeaseResults> {
    let mut new_lease: PPUC;
    let mut cost_per_phase: HashMap<u64, HashMap<u64, u64>> = HashMap::new();
    let mut budget_per_phase: HashMap<u64, u64> = HashMap::new();
    let mut leases = HashMap::new(); //{ri, lease}
    let mut dual_leases: HashMap<u64, (f64, u64)> = HashMap::new(); //{ref_id, (alpha, long_lease)}
    let mut trace_length: u64 = 0;
    let mut lease_hits = HashMap::new();
    let mut dual_lease_phases: Vec<u64> = Vec::new();
    //{phase,(cost with alpha, cost if alpha was 1, ref ID)}
    let mut past_lease_values: HashMap<u64, (u64, u64)> = HashMap::new();
    let mut last_lease_cost: HashMap<u64, HashMap<u64, (u64, u64, u64)>> = HashMap::new();

    let num_sets = context.set_mask as u64 + 1; // default set_mask value: 0
    let phase_ids: Vec<&u64> = context.samples_per_phase.keys().collect();

    //since we can't run CSHEL without also running SHEL, don't output RI history twice
    if !cshel && cli.verbose {
        println!("---------Dump RI Hists------------");
        super::io::debug::print_ri_hists(context.ri_hists);
        println!("---------Dump Samples Per Phase---");
        println!("{:?}", &context.samples_per_phase);
    }

    //threshold for meaningful dual lease
    let min_alpha: f64 = 1.0
        - (((2 << (cli.discretize_width - 1)) as f64) - 1.5f64)
            / (((2 << (cli.discretize_width - 1)) as f64) - 1.0f64);
    //initialize ppucs
    let mut ppuc_tree = BinaryHeap::new();

    for (&ref_id, ri_hist) in context.ri_hists.ri_hists.iter() {
        let ppuc_vec = get_ppuc(ref_id, 0, ri_hist);
        for ppuc in ppuc_vec.iter() {
            ppuc_tree.push(*ppuc);
        }
    }

    while let Some(lease) = ppuc_tree.pop() {
        // Process the `lease` here
        *lease_hits
            .entry(lease.ref_id)
            .or_insert(HashMap::new())
            .entry(lease.lease)
            .or_insert(0) += lease.new_hits;
    }

    // reinitalize ppuc tree, assuming a base lease of 1
    for (&ref_id, ri_hist) in context.ri_hists.ri_hists.iter() {
        push_highest_ppuc(&mut ppuc_tree, ref_id, 1, ri_hist);
    }

    //initialize cost + budget
    for (&phase, &num) in context.samples_per_phase.iter() {
        budget_per_phase
            .entry(phase)
            .or_insert(num * cli.cache_size / num_sets * context.sample_rate);
        trace_length += num * context.sample_rate;
    }

    if cli.verbose {
        println!(
            "
        ---------------------
        Initial budget per phase:
        {:?}
        ---------------------",
            budget_per_phase
        );
    }
    //initialize leases to a default value of 1
    for (&ref_id, _) in context.ri_hists.ri_hists.iter() {
        leases.insert(ref_id & 0xFFFFFFFF, 1);
        let phase = (ref_id & 0xFF000000) >> 24;
        let phase_id_ref = ref_id & 0xFFFFFFFF;
        // get cost of assigning a lease of 1 for each set
        for set in 0..num_sets {
            let set_phase_id_ref = phase_id_ref | (set << 32);
            let new_cost = match cshel {
                true => cshel_phase_ref_cost(
                    context.sample_rate,
                    phase,
                    set_phase_id_ref,
                    0,
                    1,
                    context.ri_hists,
                ),
                false => shel_phase_ref_cost(
                    context.sample_rate,
                    phase,
                    set_phase_id_ref,
                    0,
                    1,
                    context.ri_hists,
                ),
            };
            *cost_per_phase
                .entry(phase)
                .or_default()
                .entry(set)
                .or_insert(0) += new_cost;
        }
    }
    if cli.verbose {
        println!("costs per phase{:?}", cost_per_phase);
    }

    // print ppuc tree
    // println!("PPUC haha tree:");
    // for ppuc in ppuc_tree.clone() {
    //     println!("{:?}", ppuc);
    // }

    loop {
        new_lease = match ppuc_tree.pop() {
            //TERMINATION CONDITION 1
            Some(i) => i,
            None => {
                return Some(LeaseResults {
                    leases,
                    dual_leases,
                    lease_hits,
                    trace_length,
                });
            }
        };
        let phase = (new_lease.ref_id & 0xFFFFFFFF) >> 24;
        let ref_id = new_lease.ref_id & 0xFFFFFFFF;

        //continue to pop until we have a ppuc with the right base_lease
        if let Some(&old_lease) = leases.get(&ref_id) {
            if new_lease.old_lease != old_lease {
                continue;
            }
        }
        // else {
        //     // Handle the case where ref_id is not in leases
        //     // For example, skip this iteration
        //     continue;
        // }

        let mut set_full = false;
        for set in 0..num_sets {
            if cost_per_phase.get(&phase).unwrap().get(&set).unwrap()
                == budget_per_phase.get(&phase).unwrap()
            {
                set_full = true;
                break;
            }
        }

        //if any set in phase is full, skip
        if set_full {
            continue;
        }
        //if we've already assigned dual leases to all phases, end
        if dual_lease_phases.len() == cost_per_phase.len() {
            //TERMINATION CONDITION 2
            return Some(LeaseResults {
                leases,
                dual_leases,
                lease_hits,
                trace_length,
            });
        }
        //if we've already assigned a dual lease for the phase
        if dual_lease_phases.contains(&phase) {
            continue;
        }

        // default lease is the minimum lease value among all the reference in leases
        let default_lease = leases.values().cloned().min().unwrap_or(0);

        let old_lease = *leases.get(&ref_id).unwrap();
        //check for capacity
        let mut acceptable_lease = true;
        let mut new_phase_ref_cost: HashMap<u64, HashMap<u64, u64>> = HashMap::new();
        for (&phase, current_cost) in cost_per_phase.iter() {
            //get cost of assigning a lease of 1 for each set
            for set in 0..num_sets {
                let set_phase_id_ref = ref_id | (set << 32);
                let additional_cost = match cshel {
                    true => cshel_phase_ref_cost(
                        context.sample_rate,
                        phase,
                        set_phase_id_ref,
                        old_lease,
                        new_lease.lease,
                        context.ri_hists,
                    ),
                    false => shel_phase_ref_cost(
                        context.sample_rate,
                        phase,
                        set_phase_id_ref,
                        old_lease,
                        new_lease.lease,
                        context.ri_hists,
                    ),
                };

                new_phase_ref_cost
                    .entry(phase)
                    .or_default()
                    .entry(set)
                    .or_insert(additional_cost);
                if (additional_cost + current_cost.get(&set).unwrap())
                    > *budget_per_phase.get(&phase).unwrap()
                {
                    acceptable_lease = false;
                }
            }
        }
        if cli.verbose & cli.debug {
            println!("\nDebug: budgets per phase {:?}", &budget_per_phase);
            println!("Debug: Current cost budgets {:?}", &cost_per_phase);
            println!("Debug: NEW_PHASE_REF_COST {:?}", &new_phase_ref_cost);
        }
        if acceptable_lease {
            //update cache use
            for (phase, phase_set_costs) in cost_per_phase.iter_mut() {
                for (set, set_costs) in phase_set_costs.iter_mut() {
                    *set_costs += new_phase_ref_cost.get(phase).unwrap().get(set).unwrap();
                }
            }
            let phase = (new_lease.ref_id & 0xFF000000) >> 24;
            //store lease value we assign to the reference and
            //the value of the previously assigned lease for that reference
            past_lease_values.insert(
                new_lease.ref_id & 0xFFFFFFFF,
                (
                    new_lease.lease,
                    *leases.get(&(&new_lease.ref_id & 0xFFFFFFFF)).unwrap(),
                    // .unwrap_or(&default_lease),
                ),
            );

            if last_lease_cost.get_mut(&phase).is_none() {
                last_lease_cost.insert(phase, HashMap::new());
            }
            for set in 0..num_sets {
                last_lease_cost.get_mut(&phase).unwrap().insert(
                    set,
                    (
                        *new_phase_ref_cost.get(&phase).unwrap().get(&set).unwrap(),
                        *new_phase_ref_cost.get(&phase).unwrap().get(&set).unwrap(),
                        ref_id & 0xFFFFFFFF,
                    ),
                );
            }
            //update leases
            leases.insert(new_lease.ref_id & 0xFFFFFFFF, new_lease.lease);
            //push new ppucs
            push_highest_ppuc(
                &mut ppuc_tree,
                ref_id,
                new_lease.lease,
                context.ri_hists.ri_hists.get(&new_lease.ref_id).unwrap(),
            );

            if cli.verbose {
                print!(
                    "Assigned lease {:x} to reference ({},{:x}). ",
                    new_lease.lease,
                    (new_lease.ref_id & 0xFF000000) >> 24,
                    new_lease.ref_id & 0x00FFFFFF
                );
            }
        } else {
            // // println!("lease length: {}", leases.len());
            // // if the leases.len() is greater than cli.llt_size, we need to evict the reference that has the lowest ppuc amount the references in leases
            // let mut  pruned = false;
            // while leases.len() >= cli.llt_size.try_into().unwrap() {
            //     pruned = true;
            //     if cli.verbose {
            //         println!(
            //             "Evicting reference {:x} with lease {}",
            //             *leases.keys().min().unwrap(),
            //             *leases.values().min().unwrap()
            //         );
            //     }

            //     // Calculate the ppuc for each reference with a given base of lease value of 1 and remove the reference with the lowest ppuc
            //     let mut uc_tree: Vec<(u64, f64)> = Vec::new();
            //     for (ref_id, lease) in leases.iter_mut() {
            //         let uc = marginal_utility_cost(
            //             *lease,
            //             default_lease,
            //             context.ri_hists.ri_hists.get(ref_id).unwrap(),
            //         );
            //         uc_tree.push((*ref_id, uc));
            //     }

            //     // Remove the reference with the lowest ppuc
            //     let min_ref = uc_tree
            //         .iter()
            //         .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            //         .unwrap()
            //         .0;
            //     //update cache use
            //     let mut loop_count = 0;
            //     for (phase, phase_set_costs) in cost_per_phase.iter_mut() {
            //         for (set, set_costs) in phase_set_costs.iter_mut() {
            //             loop_count += 1;
            //             print!("handling ref {:x} ", min_ref);
            //             *set_costs -= calculate_cost(
            //                 leases.get(&min_ref).unwrap(),
            //                 default_lease,
            //                 context.ri_hists.ri_hists.get(&(min_ref | (set << 32))).unwrap(),
            //             );
            //         }
            //     }
            //     println!("Processed {} loops", loop_count);
            //     // lease_hits.remove(&min_ref);
            //     leases.remove(&min_ref);
            //     // Also remove the dual lease if it exists
            //     dual_leases.remove(&min_ref);

            //     push_highest_ppuc(
            //         &mut ppuc_tree,
            //         min_ref,
            //         1,
            //         context.ri_hists.ri_hists.get(&min_ref).unwrap(),
            //     );
            // }
            // if pruned {
            //     continue;
            // }
            //unacceptable lease, must assign a dual lease
            let mut alpha = 1.0;
            let mut current_phase_alpha = 1.0;
            for (&phase, phase_set_current_cost) in cost_per_phase.iter() {
                let set_budget = *budget_per_phase.get(&phase).unwrap();
                for (&set, &current_set_cost) in phase_set_current_cost.iter() {
                    let &set_phase_ref_cost =
                        new_phase_ref_cost.get(&phase).unwrap().get(&set).unwrap();
                    if set_phase_ref_cost > 0 {
                        // TODO: Fix this
                        // if set_budget < current_set_cost {
                        //     println!(
                        //         "
                        //     ERROR: current cost exceeds budget
                        //     *budget_per_phase.get(&phase)=.unwrap():  {}
                        //     set:                                     {}
                        //     current_set_cost:                        {}
                        //     ",
                        //         set_budget, set, current_set_cost
                        //     );
                        //     panic!();
                        // }

                        let remaining_budget = set_budget - current_set_cost;
                        //get the best alpha for any set  (ignoring other phases) that we want for the current reference
                        if phase == (new_lease.ref_id & 0xFF000000) >> 24 {
                            current_phase_alpha = super::helpers::float_min(
                                current_phase_alpha,
                                remaining_budget as f64 / set_phase_ref_cost as f64,
                            );
                        }
                        alpha = super::helpers::float_min(
                            alpha,
                            remaining_budget as f64 / set_phase_ref_cost as f64,
                        );
                    }
                }
            }
            //if the alpha we wish to assign would result in
            //a long lease that is never used because the short lease
            //probabiliy will be 1 after descretizing, don't assign dual lease.

            // if current_phase_alpha < min_alpha {
            //     println!("Assigning lease {:x} with percentage {} to reference ({},{:x}) would not be meaningful.",
            //              new_lease.lease, current_phase_alpha, (new_lease.ref_id & 0xFF000000) >> 24,
            //              new_lease.ref_id & 0x00FFFFFF);
            //     continue;
            // }

            if alpha > min_alpha {
                //update cache use
                for (phase, phase_set_costs) in cost_per_phase.iter_mut() {
                    let mut set_budget = *budget_per_phase.get(phase).unwrap();
                    for (set, set_costs) in phase_set_costs.iter_mut() {
                        *set_costs += (*new_phase_ref_cost.get(phase).unwrap().get(set).unwrap()
                            as f64
                            * alpha)
                            .round() as u64;
                        //fix floating point precision error leading
                        //to "overallocation" or underallocation
                        if set_costs > &mut set_budget {
                            *set_costs = set_budget;
                        }
                    }
                }
            }

            if cshel {
                //if there's no alpha that would assign a meaningful dual lease
                //that wouldn't put other phases over budget
                if alpha <= min_alpha {
                    let mut new_costs = HashMap::new();
                    let mut new_alpha = HashMap::new();
                    let mut adjust_lease = true;
                    let mut phase_alpha = 1.0;
                    for phase in &phase_ids {
                        for set in 0..num_sets {
                            let set_phase_ref_cost =
                                new_phase_ref_cost.get(phase).unwrap().get(&set).unwrap();
                            //if the phase would be effected by the lease assignment
                            if set_phase_ref_cost > &0 {
                                //get phases that would be over budgeted by assigning the current lease.
                                //then subtract the cost of their prior dual lease (which may be, due to the default, a non-dual lease)
                                //and then add the spillover cost from the new leases
                                let past_cost_actual = if !last_lease_cost.contains_key(phase)
                                    || last_lease_cost.get(phase).unwrap().get(&set).is_none()
                                {
                                    0
                                } else {
                                    last_lease_cost.get(phase).unwrap().get(&set).unwrap().0
                                };
                                if new_costs.get(&phase).is_none() {
                                    new_costs.insert(phase, HashMap::new());
                                }
                                let new_cost =
                                    cost_per_phase.get(phase).unwrap().get(&set).unwrap()
                                        - past_cost_actual
                                        + (*set_phase_ref_cost as f64 * current_phase_alpha).round()
                                            as u64;
                                new_costs.get_mut(&phase).unwrap().insert(set, new_cost);
                                //if no lease adjustment can be made to keep the phase from being over budget
                                if new_costs.get(&phase).unwrap().get(&set).unwrap()
                                    > budget_per_phase.get(phase).unwrap()
                                {
                                    adjust_lease = false;
                                    break;
                                }
                                let remaining_budget = *budget_per_phase.get(phase).unwrap()
                                    - new_costs.get(&phase).unwrap().get(&set).unwrap();
                                //if cost of last lease was zero i.e., no prior lease for phase, then alpha will be 1 and will not be adjusted
                                let past_cost_max = if past_cost_actual != 0 {
                                    last_lease_cost.get(phase).unwrap().get(&set).unwrap().1
                                } else {
                                    0
                                };
                                if past_cost_max != 0 {
                                    //if previous long lease didn't fill phase, could be greater than one
                                    let set_phase_alpha = super::helpers::float_min(
                                        1.0,
                                        remaining_budget as f64 / past_cost_max as f64,
                                    );
                                    if set_phase_alpha <= min_alpha {
                                        let old_phase_ref = last_lease_cost
                                            .get(phase)
                                            .unwrap()
                                            .get(&set)
                                            .unwrap()
                                            .2;
                                        dual_leases.get(&old_phase_ref).unwrap().1;
                                        // println!("Assigning adjusted dual lease {:x} with percentage {} to reference ({},{:x}) would not be meaningful.",
                                        //          new_lease.lease, set_phase_alpha, phase, old_phase_ref);
                                        adjust_lease = false;
                                        break;
                                    }
                                    //need the minimum alpha of any set in the phase
                                    else if set_phase_alpha < phase_alpha {
                                        phase_alpha = set_phase_alpha;
                                    }

                                    new_alpha.insert(phase, phase_alpha);
                                }
                            }
                            //new costs is equal to old cost
                            else {
                                if new_costs.get(&phase).is_none() {
                                    new_costs.insert(phase, HashMap::new());
                                }
                                new_costs.get_mut(&phase).unwrap().insert(
                                    set,
                                    *cost_per_phase.get(phase).unwrap().get(&set).unwrap(),
                                );
                            }
                        }
                    }
                    if adjust_lease {
                        for phase in &phase_ids {
                            //if adjusting lease
                            for set in 0..num_sets {
                                if new_alpha.contains_key(phase) {
                                    let old_phase_cost_max =
                                        last_lease_cost.get(phase).unwrap().get(&set).unwrap().1;
                                    let old_phase_ref =
                                        last_lease_cost.get(phase).unwrap().get(&set).unwrap().2;
                                    let new_phase_cost = (old_phase_cost_max as f64
                                        * new_alpha.get(phase).unwrap())
                                        as u64;

                                    //if phase had a dual lease
                                    if dual_lease_phases.contains(phase) {
                                        dual_leases.insert(
                                            old_phase_ref,
                                            (
                                                *new_alpha.get(&phase).unwrap(),
                                                dual_leases.get(&old_phase_ref).unwrap().1,
                                            ),
                                        );
                                    }
                                    //if we are not currently assigning a dual lease to this phase
                                    //generate dual lease from the past two lease values of the last reference assigned in this phase
                                    else if **phase != new_lease.ref_id >> 24 {
                                        //set prior single lease as long lease value with new alpha
                                        dual_leases.insert(
                                            old_phase_ref,
                                            (
                                                *new_alpha.get(&phase).unwrap(),
                                                past_lease_values.get(&old_phase_ref).unwrap().0,
                                            ),
                                        );
                                        //set the lease two references back as the short lease value
                                        leases.insert(
                                            old_phase_ref,
                                            past_lease_values.get(&old_phase_ref).unwrap().1,
                                        );
                                        dual_lease_phases.push(**phase);
                                    }

                                    last_lease_cost.get_mut(phase).unwrap().insert(
                                        set,
                                        (new_phase_cost, old_phase_cost_max, old_phase_ref),
                                    );
                                    //update phase costs
                                    cost_per_phase.get_mut(phase).unwrap().insert(
                                        set,
                                        *new_costs.get(phase).unwrap().get(&set).unwrap()
                                            + new_phase_cost,
                                    );
                                }
                                //if not adjusting the lease
                                else {
                                    //update phase costs
                                    cost_per_phase.get_mut(phase).unwrap().insert(
                                        set,
                                        *new_costs.get(phase).unwrap().get(&set).unwrap(),
                                    );
                                    //fix floating point precision error leading to "overallocation"
                                    if cost_per_phase.get(phase).unwrap().get(&set).unwrap()
                                        > budget_per_phase.get(phase).unwrap()
                                    {
                                        cost_per_phase
                                            .get_mut(phase)
                                            .unwrap()
                                            .insert(set, *budget_per_phase.get(phase).unwrap());
                                    }
                                }
                            }
                        }
                        alpha = current_phase_alpha;
                    } else {
                        //if we can't assign a dual lease without overflowing a phase
                        //without adjustment of past dual leases, with adjustment of past dual leases,
                        //or in the the unlikely case a phase is full with no dual lease

                        println!(
                            "Unable to assign lease {:x} with percentage {} to reference ({},{:x})",
                            new_lease.lease,
                            current_phase_alpha,
                            (new_lease.ref_id & 0xFF000000) >> 24,
                            new_lease.ref_id & 0x00FFFFFF
                        );
                        continue;
                    }
                }
            }

            let phase = (new_lease.ref_id & 0xFF000000) >> 24;

            //detect if set full
            let mut set_full = false;
            for set in 0..num_sets {
                if cost_per_phase.get(&phase).unwrap().get(&set).unwrap()
                    == budget_per_phase.get(&phase).unwrap()
                {
                    set_full = true;
                    break;
                }
            }
            //if last lease was a dual lease with alpha of 1 that didn't fill the budget, then it is actually a short lease and adjustments can be made to ensure
            //there is only 1 dual lease per phase.
            if alpha == 1.0 && !set_full {
                //update leases
                leases.insert(new_lease.ref_id & 0xFFFFFFFF, new_lease.lease);

                //push new ppucs
                push_highest_ppuc(
                    &mut ppuc_tree,
                    ref_id,
                    new_lease.lease,
                    context.ri_hists.ri_hists.get(&new_lease.ref_id).unwrap(),
                );

                if cli.verbose {
                    println!(
                        "Assigned lease {:x} to reference ({},{:x}).",
                        new_lease.lease,
                        (new_lease.ref_id & 0xFF000000) >> 24,
                        new_lease.ref_id & 0x00FFFFFF
                    );
                }
            } else {
                //add dual lease
                //store cost of dual lease and store cost of lease with no dual lease and the reference for that lease
                for set in 0..num_sets {
                    if last_lease_cost.get_mut(&phase).is_none() {
                        last_lease_cost.entry(phase).or_default();
                    }

                    last_lease_cost.get_mut(&phase).unwrap().insert(
                        set,
                        (
                            (*new_phase_ref_cost.get(&phase).unwrap().get(&set).unwrap() as f64
                                * alpha)
                                .round() as u64,
                            *new_phase_ref_cost.get(&phase).unwrap().get(&set).unwrap(),
                            ref_id & 0xFFFFFFFF,
                        ),
                    );
                }

                dual_lease_phases.push(phase);
                //update dual lease HashMap
                dual_leases.insert(new_lease.ref_id & 0xFFFFFFFF, (alpha, new_lease.lease));

                if cli.verbose {
                    println!(
                        "Assigned dual lease ({:x},{}) to reference ({},{:x}).",
                        new_lease.lease,
                        alpha,
                        (new_lease.ref_id & 0xFF000000) >> 24,
                        new_lease.ref_id & 0x00FFFFFF
                    );
                }
            }
        } //unacceptable lease

        if cli.verbose & cli.debug {
            for (phase, num) in context.samples_per_phase.iter() {
                for set in 0..num_sets {
                    println!(
                        "Debug: phase: {}. set: {} Allocation: {}",
                        phase,
                        set,
                        cost_per_phase.get(phase).unwrap().get(&set).unwrap()
                            / (num * context.sample_rate)
                    );
                }
            }
            /*
                println!("Debug: phase: {}",phase);
                println!("Debug:    cost_per_phase:   {:?}",
                         cost_per_phase.get(&phase).unwrap());
                println!("Debug:    budget_per_phase: {:?}",
                         budget_per_phase.get(&phase).unwrap());
            }*/
        }

        if cli.verbose {
            let mut hits_from_old_lease = 0;

            if lease_hits
                .get(&new_lease.ref_id)
                .unwrap()
                .get(&old_lease)
                .is_some()
            {
                hits_from_old_lease = *lease_hits
                    .get(&new_lease.ref_id)
                    .unwrap()
                    .get(&old_lease)
                    .unwrap();
            }
            let mut hits_from_new_lease =
                *lease_hits.get(&new_lease.ref_id)?.get(&new_lease.lease)?;
            let long_lease_percentage: f64;
            if dual_leases.contains_key(&new_lease.ref_id) {
                long_lease_percentage = dual_leases.get(&new_lease.ref_id).unwrap().0;
                let hits_without_dual = hits_from_new_lease;

                hits_from_new_lease = hits_without_dual
                    - (hits_without_dual as f64 * (1.0 - long_lease_percentage)) as u64
                    + ((1.0 - long_lease_percentage) * hits_from_old_lease as f64) as u64;
            }
            println!(
                "Additional hits from assigned lease:{}",
                (hits_from_new_lease - hits_from_old_lease) * context.sample_rate
            );
        }
    }
}

fn push_highest_ppuc(
    ppuc_tree: &mut BinaryHeap<PPUC>,
    ref_id: u64,
    base_lease: u64,
    ri_hist: &HashMap<u64, (u64, HashMap<u64, (u64, u64)>)>,
) {
    let ppuc_vec = get_ppuc(ref_id, base_lease, ri_hist);
    if let Some(&highest_ppuc) = ppuc_vec
        .iter()
        .max_by(|a, b| a.ppuc.partial_cmp(&b.ppuc).unwrap())
    {
        ppuc_tree.push(highest_ppuc);
    }
}

fn marginal_utility_cost(
    lease: u64,
    base_lease: u64,
    ri_hist: &HashMap<u64, (u64, HashMap<u64, (u64, u64)>)>,
) -> f64 {
    // If the base lease is zero, return 0 to avoid division by zero
    if base_lease == 0 {
        return 0.0;
    }

    let mut hits = 0;
    let mut cost = 0;
    let mut bhits = 0;
    let mut bcost = 0;

    // Iterate through the reuse interval histogram (RI histogram)
    for (ri, (count, _)) in ri_hist.iter() {
        if *ri <= lease {
            hits += *count;
            cost += *count * *ri;
        } else {
            cost += *count * lease;
        }

        if *ri <= base_lease {
            bhits += *count;
            bcost += *count * *ri;
        } else {
            bcost += *count * base_lease;
        }
    }

    (hits - bhits) as f64 / (cost - bcost) as f64
}

fn calculate_cost(
    lease: &u64,
    base_lease: u64,
    ri_hist: &HashMap<u64, (u64, HashMap<u64, (u64, u64)>)>,
) -> u64 {
    // Example logic: Replace this with the actual cost calculation logic
    let mut cost = 0;
    for (ri, (count, _)) in ri_hist.iter() {
        if ri > &base_lease {
            if *ri <= *lease {
                cost += count * *ri;
            } else {
                cost += count * lease;
            }
        }
    }
    // println!(
    //     "Calculating cost for lease: {}, base_lease: {}, cost: {}",
    //     lease, base_lease, cost
    // );
    cost
}
