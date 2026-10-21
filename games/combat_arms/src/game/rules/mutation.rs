//! Mutation boxes' pick (specs/combat_arms/game_modes.md S12, Open
//! question 27): which of the ten box items a toucher gets, weighted by
//! the toucher's condition (lose, even, win), and the revenge buff's item.
//!
//! - Condition: TDM by the team score gap in percent (below -15 lose,
//!   above 15 win, else even); One Man Army by the toucher's K / (K + D) in
//!   percent (0-45 lose, 46-48 even, 49-100 win).
//! - Pick: each item's weight for that condition (data/modes.toml
//!   `mutation_weights`, S12's table), one draw q in 0..W over the sum W.
//!
//! Known shortcuts: no boxes are placed (the room's mutation option isn't
//! there) and the items' effects are gear_perks.md's: this is the pick
//! only. The TDM gap is our reading: (own - enemy) / G in percent (S12
//! doesn't say what the percent is of). S12's case 76 gives the win
//! column's sum as 125; the table's win weights sum to 140 (the table is
//! used).

/// The toucher's condition: 0 lose, 1 even, 2 win.
pub fn condition_tdm(own: i32, enemy: i32, goal: u32) -> usize {
    let gap = if goal == 0 { 0.0 } else { (own - enemy) as f32 * 100.0 / goal as f32 };
    if gap < -15.0 {
        0
    } else if gap > 15.0 {
        2
    } else {
        1
    }
}

/// One Man Army's condition from kills and deaths.
pub fn condition_ffa(kills: u32, deaths: u32) -> usize {
    let total = kills + deaths;
    let p = if total == 0 { 0 } else { kills * 100 / total };
    match p {
        0..=45 => 0,
        46..=48 => 1,
        _ => 2,
    }
}

/// The item for draw `q` in 0..W (W the sum of `weights`' column
/// `condition`): the first whose running sum passes q.
pub fn pick(weights: &[[u32; 3]], condition: usize, q: u32) -> Option<usize> {
    let mut sum = 0;
    for (i, w) in weights.iter().enumerate() {
        sum += w[condition.min(2)];
        if q < sum {
            return Some(i);
        }
    }
    None
}

/// W for a condition.
pub fn total(weights: &[[u32; 3]], condition: usize) -> u32 {
    weights.iter().map(|w| w[condition.min(2)]).sum()
}

/// The revenge buff is item 10 of the same table (not in boxes).
pub const REVENGE_ITEM: usize = 10;
