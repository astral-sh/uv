use std::error::Error;
use std::hint::black_box;

// SAFETY: This declaration matches the uint64_t signature in native.c.
unsafe extern "C" {
    fn capability_native(seed: u64) -> u64;
}

#[repr(C)]
struct Record {
    count: u64,
    tag: u64,
}

enum Choice {
    Empty,
    Present { value: u64, tag: u64 },
}

// SAFETY: This standalone fixture owns the exported checkpoint symbol.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn capability_checkpoint(marker: u64) {
    black_box(marker);
}

#[inline(never)]
fn live_values(seed: u64) -> u64 {
    let live_scalar = black_box(seed.wrapping_mul(3).wrapping_add(7));
    let constant = 42_u64;
    let record = Record {
        count: live_scalar,
        tag: seed.wrapping_add(6),
    };
    let tuple = (seed, live_scalar);
    let optional = Some(seed.wrapping_add(5));
    let choice = if seed == 0 {
        Choice::Empty
    } else {
        Choice::Present {
            value: seed,
            tag: 91,
        }
    };
    let text = String::from("capability-probe");
    let items = vec![seed, live_scalar, constant];
    // Taking addresses provides a positive control for memory-resident values.
    black_box((&record, &tuple, &optional, &choice, &text, &items));
    capability_checkpoint(1);
    black_box((&record, &tuple, &optional, &choice, &text, &items));
    let selected = match choice {
        Choice::Empty => 0,
        Choice::Present { value, tag } => value.wrapping_add(tag),
    };
    black_box(live_scalar)
        .wrapping_add(constant)
        .wrapping_add(selected)
}

#[inline(never)]
fn optimized_values(seed: u64) -> u64 {
    let constant = 42_u64;
    let unused_value = seed.wrapping_mul(13);
    let _ = unused_value;
    let lower_byte = seed & 0xff;
    let bucket = lower_byte >> 4;
    let consumed = black_box(seed.wrapping_mul(5));
    black_box(consumed);
    capability_checkpoint(2);
    black_box(bucket).wrapping_add(constant)
}

#[inline(never)]
fn shadowed_values(seed: u64) -> u64 {
    let shadowed = seed.wrapping_add(10);
    {
        let shadowed = seed.wrapping_add(20);
        capability_checkpoint(3);
        black_box(shadowed);
    }
    capability_checkpoint(4);
    black_box(shadowed)
}

#[inline(always)]
fn inline_leaf(seed: u64) -> u64 {
    let inline_value = seed.wrapping_mul(2);
    capability_checkpoint(5);
    black_box(inline_value).wrapping_add(1)
}

#[inline(always)]
fn inline_middle(seed: u64) -> u64 {
    inline_leaf(seed).wrapping_add(3)
}

#[inline(never)]
fn inline_root(seed: u64) -> u64 {
    inline_middle(seed).wrapping_add(7)
}

#[inline(never)]
fn split_record(seed: u64) -> u64 {
    let partial = Record {
        count: seed.wrapping_add(2),
        tag: seed.wrapping_mul(101),
    };
    capability_checkpoint(6);
    black_box(partial.count)
}

#[inline(never)]
fn last_use(seed: u64) -> u64 {
    let temporary = black_box(seed.wrapping_mul(5));
    capability_checkpoint(7);
    black_box(temporary);
    capability_checkpoint(8);
    black_box(seed).wrapping_add(1)
}

#[inline(never)]
fn line_probe(seed: u64) -> u64 {
    let mut value = black_box(seed); // STEP_START
    value = value.wrapping_add(3);
    value = black_box(value);
    value = value.wrapping_mul(2);
    capability_checkpoint(10);
    black_box(value)
}

#[inline(never)]
fn tail_leaf(seed: u64) -> u64 {
    capability_checkpoint(11);
    black_box(seed).wrapping_add(41)
}

#[inline(never)]
fn tail_root(seed: u64) -> u64 {
    tail_leaf(seed)
}

fn exercise(seed: u64) -> u64 {
    let result = live_values(seed)
        .wrapping_add(optimized_values(seed))
        .wrapping_add(shadowed_values(seed))
        .wrapping_add(inline_root(seed))
        .wrapping_add(split_record(seed))
        .wrapping_add(last_use(seed));
    // SAFETY: capability_native accepts every u64 and has no pointer arguments.
    let native = unsafe { capability_native(seed) };
    result
        .wrapping_add(native)
        .wrapping_add(line_probe(seed))
        .wrapping_add(tail_root(seed))
}

fn main() -> Result<(), Box<dyn Error>> {
    let argument = std::env::args().nth(1);
    if argument.as_deref() == Some("train") {
        let mut checksum = 0_u64;
        for seed in 0..64 {
            checksum = checksum.wrapping_add(exercise(black_box(seed)));
        }
        println!("{checksum}");
    } else {
        let seed = if let Some(value) = argument {
            value.parse()?
        } else {
            17
        };
        println!("{}", exercise(black_box(seed)));
    }
    Ok(())
}
