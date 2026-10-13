//! Bounded Myers search over cached value hashes.

use crate::{Value, patch::ArrayPatchEdit};

// Keep high-churn inputs bounded while accommodating small edits to large
// arrays. Trace cells are usize offsets (at most 2 MiB on a 64-bit target).
const SEARCH_WORK_LIMIT: usize = 1_000_000;
const TRACE_CELLS_LIMIT: usize = 262_144;

/// None means the search budget was exhausted; no partial patch is returned.
pub(super) fn array_diff(old: &[Value], new: &[Value]) -> Option<Vec<ArrayPatchEdit>> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let old = &old[prefix..];
    let new = &new[prefix..];
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old = &old[..old.len() - suffix];
    let new = &new[..new.len() - suffix];
    if old.is_empty() && new.is_empty() {
        return Some(Vec::new());
    }
    if old.is_empty() || new.is_empty() {
        return Some(vec![ArrayPatchEdit {
            start: prefix,
            delete_count: old.len(),
            values: new.to_vec(),
        }]);
    }

    let mut trace: Vec<Vec<usize>> = Vec::new();
    let mut cells = 0;
    let mut work = SEARCH_WORK_LIMIT;
    for distance in 0..=old.len().checked_add(new.len())? {
        cells += distance + 1;
        if cells > TRACE_CELLS_LIMIT {
            return None;
        }
        let mut row = Vec::with_capacity(distance + 1);
        for index in 0..=distance {
            work = work.checked_sub(1)?;
            let diagonal = 2 * index as isize - distance as isize;
            let mut x = if distance == 0 {
                0
            } else {
                let previous = &trace[distance - 1];
                if insertion(previous, index, distance) {
                    previous[index]
                } else {
                    previous[index - 1] + 1
                }
            };
            let mut y = (x as isize - diagonal) as usize;
            while x < old.len() && y < new.len() {
                work = work.checked_sub(1)?;
                if old[x] != new[y] {
                    break;
                }
                x += 1;
                y += 1;
            }
            row.push(x);
            if x >= old.len() && y >= new.len() {
                trace.push(row);
                return Some(reconstruct(&trace, old.len(), new, prefix));
            }
        }
        trace.push(row);
    }
    unreachable!("Myers search must reach the end within the combined lengths")
}

/// Prefer deletion on ties, identically during search and reconstruction.
fn insertion(previous: &[usize], index: usize, distance: usize) -> bool {
    index == 0 || (index != distance && previous[index - 1] < previous[index])
}

fn reconstruct(
    trace: &[Vec<usize>],
    mut x: usize,
    new: &[Value],
    prefix: usize,
) -> Vec<ArrayPatchEdit> {
    let mut y = new.len();
    // Store only edits, not matching elements, to keep reconstruction bounded by D.
    let mut operations = Vec::with_capacity(trace.len() - 1);
    for distance in (1..trace.len()).rev() {
        let diagonal = x as isize - y as isize;
        let index = ((diagonal + distance as isize) / 2) as usize;
        let previous = &trace[distance - 1];
        let insert = insertion(previous, index, distance);
        let previous_diagonal = diagonal + if insert { 1 } else { -1 };
        let previous_x = previous[if insert { index } else { index - 1 }];
        let previous_y = (previous_x as isize - previous_diagonal) as usize;
        operations.push((previous_x, insert.then_some(previous_y)));
        x = previous_x;
        y = previous_y;
    }

    let mut edits: Vec<ArrayPatchEdit> = Vec::new();
    for (start, inserted) in operations.into_iter().rev() {
        let start = prefix + start;
        if !edits
            .last()
            .is_some_and(|edit| edit.start + edit.delete_count == start)
        {
            edits.push(ArrayPatchEdit {
                start,
                delete_count: 0,
                values: Vec::new(),
            });
        }
        let edit = edits.last_mut().expect("an edit was just created");
        match inserted {
            Some(index) => edit.values.push(new[index].clone()),
            None => edit.delete_count += 1,
        }
    }
    edits.reverse();
    edits
}
